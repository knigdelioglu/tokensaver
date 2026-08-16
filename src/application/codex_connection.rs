use std::fmt;
use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::modules::aging::AgingPolicy;
use crate::modules::codex_integration::{
    CodexConfigError, CodexConfigSnapshot, CodexPathError, codex_config_path,
    connect_with_snapshot, disconnect_with_snapshot, load_config_snapshot,
};
use crate::modules::transport::{
    BoundTransport, CallerCapability, TransportControl, TransportError, TransportObservation,
    TransportSettings,
};
use crate::shared::security::redact_local_secrets;

const OBSERVATION_CHANNEL_CAPACITY: usize = 1024;

#[derive(Clone, Debug)]
pub(crate) struct CodexConnectionRecord {
    pub(crate) config_path: PathBuf,
    pub(crate) snapshot_path: PathBuf,
    #[allow(dead_code)]
    pub(crate) snapshot: CodexConfigSnapshot,
}

pub(crate) struct PreparedCodexConnection {
    pub(crate) server: BoundTransport,
    pub(crate) control: TransportControl,
    pub(crate) record: CodexConnectionRecord,
    pub(crate) observations: mpsc::Receiver<TransportObservation>,
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
            Self::Config(error) => write!(
                formatter,
                "failed to update Codex config: {}",
                redact_local_secrets(&error.to_string())
            ),
            Self::Transport(error) => {
                write!(formatter, "failed to prepare TokenSaver transport: {error}")
            }
            Self::InvalidPersistedEndpoint(value) => write!(
                formatter,
                "stored TokenSaver endpoint is invalid and cannot be reused safely: {}",
                redact_local_secrets(value)
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

/// Prepare TokenSaver for the built-in OpenAI provider used by Codex.
///
/// The transport preserves Codex's auth-mode distinction at request time:
/// account-scoped requests go to the ChatGPT Codex backend; API-key-style
/// requests go to the OpenAI API backend.
///
/// Ordering is intentional:
/// 1. resolve/recover the exact TokenSaver endpoint,
/// 2. bind loopback,
/// 3. durably snapshot Codex config,
/// 4. point Codex at the already-bound endpoint.
///
/// No server task is spawned here. The runtime layer owns task supervision.
pub(crate) async fn prepare_native_codex_connection(
    snapshot_path: impl AsRef<Path>,
    requested_port: u16,
    aging_policy: AgingPolicy,
) -> Result<PreparedCodexConnection, CodexConnectionError> {
    let config_path = codex_config_path().map_err(CodexConnectionError::CodexPath)?;
    prepare_native_codex_connection_at(
        config_path,
        snapshot_path.as_ref().to_path_buf(),
        requested_port,
        aging_policy,
    )
    .await
}

pub(super) async fn prepare_native_codex_connection_at(
    config_path: PathBuf,
    snapshot_path: PathBuf,
    requested_port: u16,
    aging_policy: AgingPolicy,
) -> Result<PreparedCodexConnection, CodexConnectionError> {
    let settings = if snapshot_path.exists() {
        let snapshot =
            load_config_snapshot(&snapshot_path).map_err(CodexConnectionError::Config)?;
        let endpoint = snapshot.installed_openai_base_url.clone();
        let (port, capability) = CallerCapability::from_loopback_base_url(&endpoint)
            .ok_or_else(|| CodexConnectionError::InvalidPersistedEndpoint(endpoint.clone()))?;
        TransportSettings::native_codex_with_capability(port, capability, aging_policy)
    } else {
        TransportSettings::native_codex(requested_port, aging_policy)
    };

    let (observation_tx, observation_rx) = mpsc::channel(OBSERVATION_CHANNEL_CAPACITY);
    let server = BoundTransport::bind(settings.with_observer(observation_tx))
        .await
        .map_err(CodexConnectionError::Transport)?;
    let control = server.control();
    let snapshot = connect_with_snapshot(&config_path, &snapshot_path, &control.codex_base_url())
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{
        CodexConnectionError, disconnect_native_codex, prepare_native_codex_connection_at,
    };
    use crate::modules::aging::AgingPolicy;

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn temp_paths() -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tokensaver-phase3-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = root.join("config.toml");
        let snapshot = root.join("codex-config-snapshot.json");
        (root, config, snapshot)
    }

    #[test]
    fn persisted_endpoint_error_redacts_capability() {
        let capability = "a".repeat(64);
        let endpoint = format!("http://127.0.0.1:43117/{capability}/v1");
        let message = CodexConnectionError::InvalidPersistedEndpoint(endpoint).to_string();
        assert!(!message.contains(&capability));
        assert!(message.contains("[REDACTED]"));
    }

    #[tokio::test]
    async fn prepare_then_disconnect_restores_unrelated_codex_config() {
        let (root, config, snapshot_path) = temp_paths();
        let original = "model = \"gpt-test\"\n[mcp_servers.demo]\ncommand = \"demo\"\n";
        fs::write(&config, original).expect("write config");

        let prepared = prepare_native_codex_connection_at(
            config.clone(),
            snapshot_path.clone(),
            0,
            AgingPolicy::default(),
        )
        .await
        .expect("prepare connection");
        let endpoint = prepared.control.codex_base_url();
        let connected = fs::read_to_string(&config).expect("connected config");
        assert!(connected.contains(&format!("openai_base_url = \"{endpoint}\"")));
        assert!(snapshot_path.exists());

        let record = prepared.record.clone();
        drop(prepared);
        disconnect_native_codex(&record).expect("disconnect");
        let restored = fs::read_to_string(&config).expect("restored config");
        assert!(!restored.contains("openai_base_url"));
        assert!(restored.contains("model = \"gpt-test\""));
        assert!(restored.contains("[mcp_servers.demo]"));
        assert!(!snapshot_path.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn restart_reuses_persisted_port_and_capability() {
        let (root, config, snapshot_path) = temp_paths();
        fs::write(&config, "model = \"gpt-test\"\n").expect("write config");

        let first = prepare_native_codex_connection_at(
            config.clone(),
            snapshot_path.clone(),
            0,
            AgingPolicy::default(),
        )
        .await
        .expect("first prepare");
        let first_endpoint = first.control.codex_base_url();
        drop(first);

        let second = prepare_native_codex_connection_at(
            config.clone(),
            snapshot_path.clone(),
            0,
            AgingPolicy::default(),
        )
        .await
        .expect("restart prepare");
        assert_eq!(second.control.codex_base_url(), first_endpoint);

        let record = second.record.clone();
        drop(second);
        disconnect_native_codex(&record).expect("disconnect");
        let _ = fs::remove_dir_all(root);
    }
}
