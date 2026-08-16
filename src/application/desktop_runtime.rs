use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;
use rand::rngs::OsRng;
use rand::RngCore;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::modules::aging::AgingPolicy;
use crate::modules::codex_integration::{
    connection_state_with_snapshot, CodexConnectionState,
};
use crate::modules::runtime::{
    CodexStatus, RuntimePreferencesError, RuntimePreferencesStore, RuntimeStatus,
    RuntimeStatusStore, ServiceStatus,
};
use crate::modules::telemetry::{
    DurableSavingsStore, LastOptimization, SavingsLedger, SavingsStoreError, SavingsSummary,
};
use crate::modules::transport::TransportControl;

use super::codex_connection::{
    disconnect_native_codex, prepare_native_codex_connection, CodexConnectionError,
    CodexConnectionRecord,
};
use super::measurement::event_from_transport_observation;

const SNAPSHOT_FILE: &str = "codex-config-snapshot.json";
const SAVINGS_FILE: &str = "savings.json";
const PREFERENCES_FILE: &str = "runtime-preferences.json";

#[derive(Clone, Debug)]
pub(crate) struct DesktopRuntimeSnapshot {
    pub(crate) runtime: RuntimeStatus,
    pub(crate) session: SavingsSummary,
    pub(crate) today: SavingsSummary,
    pub(crate) all_time: SavingsSummary,
    pub(crate) last_optimization: Option<LastOptimization>,
}

#[derive(Debug)]
pub(crate) enum DesktopRuntimeError {
    Io(io::Error),
    Preferences(RuntimePreferencesError),
    Savings(SavingsStoreError),
    Codex(CodexConnectionError),
}

impl fmt::Display for DesktopRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "runtime I/O failed: {error}"),
            Self::Preferences(error) => write!(formatter, "runtime preferences failed: {error}"),
            Self::Savings(error) => write!(formatter, "savings persistence failed: {error}"),
            Self::Codex(error) => write!(formatter, "Codex connection failed: {error}"),
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
    data_dir: PathBuf,
    snapshot_path: PathBuf,
}

impl DesktopRuntimeController {
    pub(crate) fn open(data_dir: impl Into<PathBuf>) -> Result<Self, DesktopRuntimeError> {
        let data_dir = data_dir.into();
        fs::create_dir_all(&data_dir)?;

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
            data_dir,
        })
    }

    pub(crate) async fn initialize(&self) {
        if self.snapshot_path.exists() {
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
    }

    pub(crate) async fn connect(&self) -> Result<(), DesktopRuntimeError> {
        let _operation = self.operation.lock().await;
        if self.inner.lock().await.connection.is_some() {
            self.refresh_connection_health().await;
            return Ok(());
        }

        self.status.update(|runtime| {
            runtime.service = ServiceStatus::Starting;
            runtime.codex = CodexStatus::Connecting;
            runtime.last_error = None;
        });

        let saving_enabled = self.preferences.lock().await.preferences().saving_enabled;
        let prepared = match prepare_native_codex_connection(
            &self.snapshot_path,
            0,
            AgingPolicy {
                enabled: saving_enabled,
                ..AgingPolicy::default()
            },
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

        let control = prepared.control.clone();
        let record = prepared.record.clone();
        let status_for_server = self.status.clone();
        let server_task = tokio::spawn(async move {
            if let Err(error) = prepared.server.serve().await {
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
        let mut observations = prepared.observations;
        let observation_task = tokio::spawn(async move {
            while let Some(observation) = observations.recv().await {
                let observed_at_epoch_ms = now_epoch_ms();
                let event = event_from_transport_observation(
                    observed_at_epoch_ms,
                    session_id,
                    &observation,
                    None,
                );
                session_ledger.lock().await.record(event);
                durable_savings
                    .lock()
                    .await
                    .record(event, &local_day_key());
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
            runtime.saving_enabled = saving_enabled;
            runtime.active_requests = 0;
            runtime.last_error = None;
        });
        Ok(())
    }

    pub(crate) async fn disconnect(&self) -> Result<(), DesktopRuntimeError> {
        let _operation = self.operation.lock().await;
        let record = {
            let inner = self.inner.lock().await;
            inner.connection.as_ref().map(|active| active.record.clone())
        };

        let Some(record) = record else {
            self.status.update(|runtime| {
                runtime.service = ServiceStatus::Running;
                runtime.codex = CodexStatus::Disconnected;
                runtime.active_requests = 0;
            });
            self.flush_persistent().await?;
            return Ok(());
        };

        if let Err(error) = disconnect_native_codex(&record) {
            self.status.update(|runtime| {
                runtime.service = ServiceStatus::Running;
                runtime.codex = CodexStatus::Error;
                runtime.last_error = Some(error.to_string());
            });
            return Err(error.into());
        }

        if let Some(active) = self.inner.lock().await.connection.take() {
            active.server_task.abort();
            active.observation_task.abort();
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
        self.preferences
            .lock()
            .await
            .set_saving_enabled(enabled)?;
        let control = {
            let inner = self.inner.lock().await;
            inner.connection.as_ref().map(|active| active.control.clone())
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

        let connection_state = connection_state_with_snapshot(
            &record.config_path,
            &record.snapshot_path,
        );
        self.status.update(|runtime| {
            runtime.active_requests = control.active_requests();
            match connection_state {
                Ok(CodexConnectionState::Connected) => {
                    if runtime.service != ServiceStatus::Error {
                        runtime.service = ServiceStatus::Running;
                    }
                    runtime.codex = CodexStatus::Connected;
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
                        "TokenSaver transport is running but Codex is no longer configured to use it"
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
        let local_day = local_day_key();
        let runtime = self.status.snapshot();
        let session = self.session_ledger.lock().await.for_session(self.session_id);
        let savings = self.durable_savings.lock().await;
        DesktopRuntimeSnapshot {
            runtime,
            session,
            today: savings.for_day(&local_day),
            all_time: savings.all_time(),
            last_optimization: savings.last_optimization(),
        }
    }

    pub(crate) async fn flush_persistent(&self) -> Result<(), DesktopRuntimeError> {
        self.durable_savings.lock().await.flush()?;
        Ok(())
    }

    pub(crate) async fn safe_shutdown(&self) -> Result<(), DesktopRuntimeError> {
        let connected = self.inner.lock().await.connection.is_some();
        if connected {
            self.disconnect().await?;
        } else {
            self.flush_persistent().await?;
        }
        Ok(())
    }

    pub(crate) fn data_dir(&self) -> &Path {
        &self.data_dir
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
