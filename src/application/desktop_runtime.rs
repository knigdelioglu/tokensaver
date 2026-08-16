use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;
use rand::RngCore;
use rand::rngs::OsRng;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};

use crate::modules::aging::AgingPolicy;
use crate::modules::codex_integration::{CodexConnectionState, connection_state_with_snapshot};
use crate::modules::runtime::{
    CodexStatus, RuntimePreferences, RuntimePreferencesError, RuntimePreferencesStore,
    RuntimeStatusStore, ServiceStatus,
};
use crate::modules::telemetry::{
    DurableSavingsStore, LastOptimization, SavingsLedger, SavingsStoreError, SavingsSummary,
};
use crate::modules::transport::TransportControl;
use crate::shared::paths::control_socket_path;

use super::codex_connection::{
    CodexConnectionError, CodexConnectionRecord, PreparedCodexConnection, disconnect_native_codex,
    prepare_native_codex_connection,
};
use super::control::serve_control_socket;
use super::measurement::event_from_transport_observation;

const SNAPSHOT_FILE: &str = "codex-config-snapshot.json";
const SAVINGS_FILE: &str = "savings.json";
const PREFERENCES_FILE: &str = "runtime-preferences.json";
const OBSERVATION_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DesktopServiceState {
    Starting,
    Running,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DesktopCodexState {
    Disconnected,
    Connecting,
    Connected,
    Drifted,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SavingsView {
    pub(crate) bytes_saved: u64,
    pub(crate) estimated_tokens_saved: u64,
    pub(crate) tool_results_compacted: u64,
    pub(crate) aged_requests: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LastOptimizationView {
    pub(crate) observed_at_epoch_ms: u64,
    pub(crate) bytes_before: u64,
    pub(crate) bytes_after: u64,
    pub(crate) bytes_saved: u64,
    pub(crate) estimated_tokens_saved: u64,
    pub(crate) tool_results_compacted: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AgingPolicyView {
    pub(crate) min_bytes: usize,
    pub(crate) frontier: usize,
    pub(crate) preview_code_units: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DesktopRuntimeSnapshot {
    pub(crate) service: DesktopServiceState,
    pub(crate) codex: DesktopCodexState,
    pub(crate) saving_enabled: bool,
    pub(crate) connect_on_launch: bool,
    pub(crate) active_requests: usize,
    pub(crate) dropped_telemetry_observations: u64,
    pub(crate) policy: AgingPolicyView,
    pub(crate) session: SavingsView,
    pub(crate) today: SavingsView,
    pub(crate) all_time: SavingsView,
    pub(crate) last_optimization: Option<LastOptimizationView>,
    pub(crate) last_error: Option<String>,
}

#[derive(Debug)]
pub(crate) enum DesktopRuntimeError {
    Io(io::Error),
    Preferences(RuntimePreferencesError),
    Savings(SavingsStoreError),
    Codex(CodexConnectionError),
    ActiveRequests(usize),
    PolicyChangeRequiresDisconnect,
}

impl fmt::Display for DesktopRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "runtime I/O failed: {error}"),
            Self::Preferences(error) => write!(formatter, "runtime preferences failed: {error}"),
            Self::Savings(error) => write!(formatter, "savings persistence failed: {error}"),
            Self::Codex(error) => write!(formatter, "Codex connection failed: {error}"),
            Self::ActiveRequests(count) => write!(
                formatter,
                "cannot disconnect TokenSaver while {count} Codex request(s) are still active"
            ),
            Self::PolicyChangeRequiresDisconnect => write!(
                formatter,
                "disconnect Codex before changing aging thresholds or preview policy"
            ),
        }
    }
}

impl std::error::Error for DesktopRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Preferences(error) => Some(error),
            Self::Savings(error) => Some(error),
            Self::Codex(error) => Some(error),
            Self::ActiveRequests(_) | Self::PolicyChangeRequiresDisconnect => None,
        }
    }
}

impl From<io::Error> for DesktopRuntimeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<RuntimePreferencesError> for DesktopRuntimeError {
    fn from(error: RuntimePreferencesError) -> Self {
        Self::Preferences(error)
    }
}

impl From<SavingsStoreError> for DesktopRuntimeError {
    fn from(error: SavingsStoreError) -> Self {
        Self::Savings(error)
    }
}

impl From<CodexConnectionError> for DesktopRuntimeError {
    fn from(error: CodexConnectionError) -> Self {
        Self::Codex(error)
    }
}

struct ActiveConnection {
    record: CodexConnectionRecord,
    control: TransportControl,
    server_task: JoinHandle<()>,
    observation_task: JoinHandle<()>,
}

#[derive(Default)]
struct ControllerInner {
    connection: Option<ActiveConnection>,
}

#[derive(Clone)]
pub(crate) struct DesktopRuntimeController {
    inner: Arc<Mutex<ControllerInner>>,
    operation: Arc<Mutex<()>>,
    status: RuntimeStatusStore,
    preferences: Arc<Mutex<RuntimePreferencesStore>>,
    session_ledger: Arc<Mutex<SavingsLedger>>,
    durable_savings: Arc<Mutex<DurableSavingsStore>>,
    session_id: u64,
    snapshot_path: PathBuf,
}

impl DesktopRuntimeController {
    pub(crate) fn open(data_dir: impl Into<PathBuf>) -> Result<Self, DesktopRuntimeError> {
        let data_dir = data_dir.into();
        fs::create_dir_all(&data_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o700))?;
        }

        let preferences = RuntimePreferencesStore::open(data_dir.join(PREFERENCES_FILE))?;
        let saving_enabled = preferences.preferences().saving_enabled;
        let durable_savings = DurableSavingsStore::open(data_dir.join(SAVINGS_FILE))?;
        let status = RuntimeStatusStore::default();
        status.update(|runtime| {
            runtime.service = ServiceStatus::Starting;
            runtime.codex = CodexStatus::Disconnected;
            runtime.saving_enabled = saving_enabled;
        });

        Ok(Self {
            inner: Arc::new(Mutex::new(ControllerInner::default())),
            operation: Arc::new(Mutex::new(())),
            status,
            preferences: Arc::new(Mutex::new(preferences)),
            session_ledger: Arc::new(Mutex::new(SavingsLedger::default())),
            durable_savings: Arc::new(Mutex::new(durable_savings)),
            session_id: random_session_id(),
            snapshot_path: data_dir.join(SNAPSHOT_FILE),
        })
    }

    pub(crate) async fn initialize(&self) {
        let connect_on_launch = self
            .preferences
            .lock()
            .await
            .preferences()
            .connect_on_launch;
        if self.snapshot_path.exists() || connect_on_launch {
            if let Err(error) = self.connect().await {
                self.status.update(|runtime| {
                    runtime.service = ServiceStatus::Running;
                    runtime.codex = CodexStatus::Error;
                    runtime.last_error = Some(error.to_string());
                });
            }
        } else {
            self.status.update(|runtime| {
                runtime.service = ServiceStatus::Running;
                runtime.codex = CodexStatus::Disconnected;
                runtime.last_error = None;
            });
        }
        self.start_control_server();
    }

    fn start_control_server(&self) {
        let socket_path = match control_socket_path() {
            Ok(path) => path,
            Err(error) => {
                self.status.update(|runtime| {
                    runtime.service = ServiceStatus::Error;
                    runtime.last_error = Some(format!("CLI control path failed: {error}"));
                });
                return;
            }
        };
        let controller = self.clone();
        let status = self.status.clone();
        tokio::spawn(async move {
            if let Err(error) = serve_control_socket(socket_path, controller).await {
                status.update(|runtime| {
                    runtime.service = ServiceStatus::Error;
                    runtime.last_error = Some(format!("CLI control server failed: {error}"));
                });
            }
        });
    }

    pub(crate) async fn connect(&self) -> Result<(), DesktopRuntimeError> {
        let _operation = self.operation.lock().await;
        if self.inner.lock().await.connection.is_some() {
            self.refresh_connection_health().await;
            return Ok(());
        }

        self.preferences.lock().await.set_connect_on_launch(true)?;
        self.status.update(|runtime| {
            runtime.service = ServiceStatus::Starting;
            runtime.codex = CodexStatus::Connecting;
            runtime.last_error = None;
        });

        let preferences = self.preferences.lock().await.preferences();
        let prepared = match prepare_native_codex_connection(
            &self.snapshot_path,
            0,
            policy_from_preferences(preferences),
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(error) => {
                self.status.update(|runtime| {
                    runtime.service = ServiceStatus::Running;
                    runtime.codex = CodexStatus::Error;
                    runtime.last_error = Some(error.to_string());
                });
                return Err(error.into());
            }
        };

        let PreparedCodexConnection {
            server,
            control,
            record,
            observations,
        } = prepared;
        let status_for_server = self.status.clone();
        let server_task = tokio::spawn(async move {
            if let Err(error) = server.serve().await {
                status_for_server.update(|runtime| {
                    runtime.service = ServiceStatus::Error;
                    runtime.codex = CodexStatus::Error;
                    runtime.last_error = Some(error.to_string());
                });
            }
        });

        let session_ledger = self.session_ledger.clone();
        let durable_savings = self.durable_savings.clone();
        let session_id = self.session_id;
        let mut observations = observations;
        let observation_task = tokio::spawn(async move {
            while let Some(observation) = observations.recv().await {
                let event = event_from_transport_observation(
                    now_epoch_ms(),
                    session_id,
                    &observation,
                    None,
                );
                session_ledger.lock().await.record(event);
                durable_savings.lock().await.record(event, &local_day_key());
            }
        });

        self.inner.lock().await.connection = Some(ActiveConnection {
            record,
            control,
            server_task,
            observation_task,
        });
        self.status.update(|runtime| {
            runtime.service = ServiceStatus::Running;
            runtime.codex = CodexStatus::Connected;
            runtime.saving_enabled = preferences.saving_enabled;
            runtime.active_requests = 0;
            runtime.last_error = None;
        });
        Ok(())
    }

    /// Explicit user disconnect. Unlike safe app shutdown, this clears the
    /// persistent desire to reconnect on the next launch.
    pub(crate) async fn disconnect(&self) -> Result<(), DesktopRuntimeError> {
        self.disconnect_internal(true).await
    }

    async fn disconnect_internal(
        &self,
        clear_connect_preference: bool,
    ) -> Result<(), DesktopRuntimeError> {
        let _operation = self.operation.lock().await;
        let active = {
            let inner = self.inner.lock().await;
            inner
                .connection
                .as_ref()
                .map(|connection| (connection.record.clone(), connection.control.clone()))
        };

        let Some((record, control)) = active else {
            if clear_connect_preference {
                self.preferences.lock().await.set_connect_on_launch(false)?;
            }
            self.status.update(|runtime| {
                runtime.service = ServiceStatus::Running;
                runtime.codex = CodexStatus::Disconnected;
                runtime.active_requests = 0;
            });
            self.flush_persistent().await?;
            return Ok(());
        };

        let active_requests = control.begin_drain();
        if active_requests > 0 {
            control.resume_accepting();
            self.status
                .update(|runtime| runtime.active_requests = active_requests);
            return Err(DesktopRuntimeError::ActiveRequests(active_requests));
        }

        if let Err(error) = disconnect_native_codex(&record) {
            control.resume_accepting();
            self.status.update(|runtime| {
                runtime.service = ServiceStatus::Running;
                runtime.codex = CodexStatus::Error;
                runtime.last_error = Some(error.to_string());
            });
            return Err(error.into());
        }

        if let Some(active) = self.inner.lock().await.connection.take() {
            let ActiveConnection {
                server_task,
                mut observation_task,
                ..
            } = active;
            server_task.abort();
            let _ = server_task.await;
            if timeout(OBSERVATION_DRAIN_TIMEOUT, &mut observation_task)
                .await
                .is_err()
            {
                observation_task.abort();
                let _ = observation_task.await;
            }
        }

        if clear_connect_preference {
            self.preferences.lock().await.set_connect_on_launch(false)?;
        }
        self.status.update(|runtime| {
            runtime.service = ServiceStatus::Running;
            runtime.codex = CodexStatus::Disconnected;
            runtime.active_requests = 0;
            runtime.last_error = None;
        });
        self.flush_persistent().await?;
        Ok(())
    }

    pub(crate) async fn set_saving_enabled(
        &self,
        enabled: bool,
    ) -> Result<(), DesktopRuntimeError> {
        let _operation = self.operation.lock().await;
        self.preferences.lock().await.set_saving_enabled(enabled)?;
        let control = {
            let inner = self.inner.lock().await;
            inner
                .connection
                .as_ref()
                .map(|active| active.control.clone())
        };
        if let Some(control) = control {
            control.set_aging_enabled(enabled).await;
        }
        self.status.update(|runtime| {
            runtime.saving_enabled = enabled;
            runtime.last_error = None;
        });
        Ok(())
    }

    pub(crate) async fn set_min_bytes(&self, value: usize) -> Result<(), DesktopRuntimeError> {
        let _operation = self.operation.lock().await;
        self.ensure_disconnected_for_policy_change().await?;
        self.preferences.lock().await.set_min_bytes(value)?;
        Ok(())
    }

    pub(crate) async fn set_frontier(&self, value: usize) -> Result<(), DesktopRuntimeError> {
        let _operation = self.operation.lock().await;
        self.ensure_disconnected_for_policy_change().await?;
        self.preferences.lock().await.set_frontier(value)?;
        Ok(())
    }

    pub(crate) async fn set_preview_code_units(
        &self,
        value: usize,
    ) -> Result<(), DesktopRuntimeError> {
        let _operation = self.operation.lock().await;
        self.ensure_disconnected_for_policy_change().await?;
        self.preferences
            .lock()
            .await
            .set_preview_code_units(value)?;
        Ok(())
    }

    async fn ensure_disconnected_for_policy_change(&self) -> Result<(), DesktopRuntimeError> {
        if self.inner.lock().await.connection.is_some() {
            return Err(DesktopRuntimeError::PolicyChangeRequiresDisconnect);
        }
        Ok(())
    }

    pub(crate) async fn refresh_connection_health(&self) {
        let active = {
            let inner = self.inner.lock().await;
            inner
                .connection
                .as_ref()
                .map(|active| (active.record.clone(), active.control.clone()))
        };

        let Some((record, control)) = active else {
            self.status.update(|runtime| {
                runtime.service = ServiceStatus::Running;
                runtime.codex = CodexStatus::Disconnected;
                runtime.active_requests = 0;
            });
            return;
        };

        let connection_state =
            connection_state_with_snapshot(&record.config_path, &record.snapshot_path);
        self.status.update(|runtime| {
            runtime.active_requests = control.active_requests();
            match connection_state {
                Ok(CodexConnectionState::Connected) if runtime.service == ServiceStatus::Error => {
                    runtime.codex = CodexStatus::Error;
                }
                Ok(CodexConnectionState::Connected) => {
                    runtime.service = ServiceStatus::Running;
                    runtime.codex = CodexStatus::Connected;
                    runtime.last_error = None;
                }
                Ok(CodexConnectionState::Drifted) => {
                    runtime.codex = CodexStatus::Drifted;
                    runtime.last_error = Some(
                        "Codex configuration changed while TokenSaver was connected".to_owned(),
                    );
                }
                Ok(CodexConnectionState::NotConnected) => {
                    runtime.codex = CodexStatus::Error;
                    runtime.last_error = Some(
                        "TokenSaver transport is running but Codex no longer points at it"
                            .to_owned(),
                    );
                }
                Err(error) => {
                    runtime.codex = CodexStatus::Error;
                    runtime.last_error = Some(error.to_string());
                }
            }
        });
    }

    pub(crate) async fn snapshot(&self) -> DesktopRuntimeSnapshot {
        self.refresh_connection_health().await;
        let dropped_telemetry_observations = {
            let inner = self.inner.lock().await;
            inner
                .connection
                .as_ref()
                .map(|active| active.control.dropped_observations())
                .unwrap_or(0)
        };
        let local_day = local_day_key();
        let runtime = self.status.snapshot();
        let preferences = self.preferences.lock().await.preferences();
        let session = self
            .session_ledger
            .lock()
            .await
            .for_session(self.session_id);
        let savings = self.durable_savings.lock().await;

        DesktopRuntimeSnapshot {
            service: map_service_status(runtime.service),
            codex: map_codex_status(runtime.codex),
            saving_enabled: runtime.saving_enabled,
            connect_on_launch: preferences.connect_on_launch,
            active_requests: runtime.active_requests,
            dropped_telemetry_observations,
            policy: AgingPolicyView {
                min_bytes: preferences.min_bytes,
                frontier: preferences.frontier,
                preview_code_units: preferences.preview_code_units,
            },
            session: savings_view(session),
            today: savings_view(savings.for_day(&local_day)),
            all_time: savings_view(savings.all_time()),
            last_optimization: savings.last_optimization().map(last_optimization_view),
            last_error: runtime.last_error,
        }
    }

    pub(crate) async fn flush_persistent(&self) -> Result<(), DesktopRuntimeError> {
        self.durable_savings.lock().await.flush()?;
        Ok(())
    }

    /// Restore temporary Codex config and flush telemetry without clearing the
    /// user's desire to reconnect when TokenSaver launches again.
    pub(crate) async fn safe_shutdown(&self) -> Result<(), DesktopRuntimeError> {
        let connected = self.inner.lock().await.connection.is_some();
        if connected {
            self.disconnect_internal(false).await?;
        } else {
            self.flush_persistent().await?;
        }
        Ok(())
    }
}

fn policy_from_preferences(preferences: RuntimePreferences) -> AgingPolicy {
    AgingPolicy {
        enabled: preferences.saving_enabled,
        min_bytes: preferences.min_bytes,
        frontier: preferences.frontier,
        preview_code_units: preferences.preview_code_units,
    }
}

fn savings_view(summary: SavingsSummary) -> SavingsView {
    SavingsView {
        bytes_saved: summary.bytes_saved,
        estimated_tokens_saved: summary.estimated_tokens_saved,
        tool_results_compacted: summary.tool_results_compacted,
        aged_requests: summary.aged_requests,
    }
}

fn last_optimization_view(last: LastOptimization) -> LastOptimizationView {
    LastOptimizationView {
        observed_at_epoch_ms: last.observed_at_epoch_ms,
        bytes_before: last.bytes_before,
        bytes_after: last.bytes_after,
        bytes_saved: last.bytes_saved,
        estimated_tokens_saved: last.estimated_tokens_saved,
        tool_results_compacted: last.tool_results_compacted,
    }
}

fn map_service_status(status: ServiceStatus) -> DesktopServiceState {
    match status {
        ServiceStatus::Stopped | ServiceStatus::Starting => DesktopServiceState::Starting,
        ServiceStatus::Running => DesktopServiceState::Running,
        ServiceStatus::Error => DesktopServiceState::Error,
    }
}

fn map_codex_status(status: CodexStatus) -> DesktopCodexState {
    match status {
        CodexStatus::Disconnected => DesktopCodexState::Disconnected,
        CodexStatus::Connecting => DesktopCodexState::Connecting,
        CodexStatus::Connected => DesktopCodexState::Connected,
        CodexStatus::Drifted => DesktopCodexState::Drifted,
        CodexStatus::Error => DesktopCodexState::Error,
    }
}

fn random_session_id() -> u64 {
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    u64::from_le_bytes(bytes)
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn local_day_key() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}
