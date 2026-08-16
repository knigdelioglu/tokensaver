use std::fmt;
use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::modules::aging::AgingPolicy;
use crate::modules::codex_integration::{
    codex_config_path, connect_with_snapshot, disconnect_with_snapshot, load_config_snapshot,
    CodexConfigError, CodexConfigSnapshot, CodexPathError,
};
use crate::modules::transport::{
    BoundTransport, CallerCapability, TransportControl, TransportError, TransportObservation,
    TransportSettings,
};

#[derive(Clone, Debug)]
pub(crate) struct CodexConnectionRecord {
    pub(crate) config_path: PathBuf,
    pub(crate) snapshot_path: PathBuf,
    pub(crate) snapshot: CodexConfigSnapshot,
}

pub(crate) struct PreparedCodexConnection {
    pub(crate) server: BoundTransport,
    pub(crate) control: TransportControl,
    pub(crate) record: CodexConnectionRecord,
    pub(crate) observations: mpsc::UnboundedReceiver<TransportObservation>,
}

#[derive(Debug)]
pub(crate) enum CodexConnectionError {
    CodexPath(CodexPathError),
    Config(CodexConfigError),
    Transport(TransportError),
    InvalidPersistedEndpoint(String),
}

impl fmt::Display for CodexConnectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CodexPath(error) => write!(formatter, "failed to locate Codex config: {error}"),
            Self::Config(error) => write!(formatter, "failed to update Codex config: {error}"),
            Self::Transport(error) => write!(formatter, "failed to prepare TokenSaver transport: {error}"),
            Self::InvalidPersistedEndpoint(value) => write!(
                formatter,
                "stored TokenSaver endpoint is invalid and cannot be reused safely: {value:?}"
            ),
        }
    }
}

impl std::error::Error for CodexConnectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CodexPath(error) => Some(error),
            Self::Config(error) => Some(error),
            Self::Transport(error) => Some(error),
            Self::InvalidPersistedEndpoint(_) => None,
        }
    }
}

/// Prepare TokenSaver for the native ChatGPT/Codex backend.
///
/// Ordering is intentional:
/// 1. resolve/recover the exact TokenSaver endpoint,
/// 2. bind loopback,
/// 3. durably snapshot Codex config,
/// 4. point Codex at the already-bound endpoint.
///
/// No server task is spawned here. The runtime layer owns task supervision.
pub(crate) async fn prepare_native_chatgpt_connection(
    snapshot_path: impl AsRef<Path>,
    requested_port: u16,
    aging_policy: AgingPolicy,
) -> Result<PreparedCodexConnection, CodexConnectionError> {
    let config_path = codex_config_path().map_err(CodexConnectionError::CodexPath)?;
    prepare_native_chatgpt_connection_at(
        config_path,
        snapshot_path.as_ref().to_path_buf(),
        requested_port,
        aging_policy,
    )
    .await
}

pub(super) async fn prepare_native_chatgpt_connection_at(
    config_path: PathBuf,
    snapshot_path: PathBuf,
    requested_port: u16,
    aging_policy: AgingPolicy,
) -> Result<PreparedCodexConnection, CodexConnectionError> {
    let settings = if snapshot_path.exists() {
        let snapshot = load_config_snapshot(&snapshot_path).map_err(CodexConnectionError::Config)?;
        let endpoint = snapshot.installed_openai_base_url.clone();
        let (port, capability) = CallerCapability::from_loopback_base_url(&endpoint)
            .ok_or_else(|| CodexConnectionError::InvalidPersistedEndpoint(endpoint.clone()))?;
        TransportSettings::native_chatgpt_with_capability(port, capability, aging_policy)
    } else {
        TransportSettings::native_chatgpt(requested_port, aging_policy)
    };

    let (observation_tx, observation_rx) = mpsc::unbounded_channel();
    let server = BoundTransport::bind(settings.with_observer(observation_tx))
        .await
        .map_err(CodexConnectionError::Transport)?;
    let control = server.control();
    let snapshot = connect_with_snapshot(
        &config_path,
        &snapshot_path,
        &control.codex_base_url(),
    )
    .map_err(CodexConnectionError::Config)?;

    Ok(PreparedCodexConnection {
        server,
        control,
        record: CodexConnectionRecord {
            config_path,
            snapshot_path,
            snapshot,
        },
        observations: observation_rx,
    })
}

pub(crate) fn disconnect_native_codex(
    record: &CodexConnectionRecord,
) -> Result<CodexConfigSnapshot, CodexConnectionError> {
    disconnect_with_snapshot(&record.config_path, &record.snapshot_path)
        .map_err(CodexConnectionError::Config)
}
