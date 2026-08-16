use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::desktop_runtime::{
    AgingPolicyView, DesktopCodexState, DesktopRuntimeController, DesktopRuntimeSnapshot,
    DesktopServiceState, SavingsView,
};

const MAX_CONTROL_MESSAGE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "command", rename_all = "snake_case")]
pub(crate) enum ControlRequest {
    Status,
    Connect,
    Disconnect,
    Saving { enabled: bool },
    Stats,
    ConfigShow,
    ConfigSet { key: String, value: usize },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ControlSavings {
    pub(crate) bytes_saved: u64,
    pub(crate) estimated_tokens_saved: u64,
    pub(crate) tool_results_compacted: u64,
    pub(crate) aged_requests: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ControlPolicy {
    pub(crate) min_bytes: usize,
    pub(crate) frontier: usize,
    pub(crate) preview_code_units: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ControlSnapshot {
    pub(crate) service: String,
    pub(crate) codex: String,
    pub(crate) saving_enabled: bool,
    pub(crate) connect_on_launch: bool,
    pub(crate) active_requests: usize,
    pub(crate) policy: ControlPolicy,
    pub(crate) session: ControlSavings,
    pub(crate) today: ControlSavings,
    pub(crate) all_time: ControlSavings,
    pub(crate) last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ControlResponse {
    pub(crate) ok: bool,
    pub(crate) message: Option<String>,
    pub(crate) snapshot: Option<ControlSnapshot>,
}

impl ControlResponse {
    fn success(snapshot: ControlSnapshot, message: Option<String>) -> Self {
        Self {
            ok: true,
            message,
            snapshot: Some(snapshot),
        }
    }

    fn failure(message: impl Into<String>, snapshot: Option<ControlSnapshot>) -> Self {
        Self {
            ok: false,
            message: Some(message.into()),
            snapshot,
        }
    }
}

#[derive(Debug)]
pub(crate) enum ControlError {
    UnsupportedPlatform,
    RuntimeAlreadyActive,
    Io(io::Error),
    Protocol(String),
}

impl fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                write!(formatter, "TokenSaver control socket is currently macOS/Unix only")
            }
            Self::RuntimeAlreadyActive => {
                write!(formatter, "a TokenSaver control runtime is already active")
            }
            Self::Io(error) => write!(formatter, "control channel I/O failed: {error}"),
            Self::Protocol(error) => write!(formatter, "control protocol failed: {error}"),
        }
    }
}

impl std::error::Error for ControlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ControlError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(unix)]
pub(crate) async fn serve_control_socket(
    socket_path: PathBuf,
    controller: DesktopRuntimeController,
) -> Result<(), ControlError> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{UnixListener, UnixStream};

    let parent = socket_path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "control socket path has no parent")
    })?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;

    if socket_path.exists() {
        match UnixStream::connect(&socket_path).await {
            Ok(_) => return Err(ControlError::RuntimeAlreadyActive),
            Err(_) => {
                let _ = fs::remove_file(&socket_path);
            }
        }
    }

    let listener = UnixListener::bind(&socket_path)?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;

    loop {
        let (stream, _) = listener.accept().await?;
        let controller = controller.clone();
        tokio::spawn(async move {
            let (reader, mut writer) = stream.into_split();
            let mut limited = BufReader::new(reader).take((MAX_CONTROL_MESSAGE_BYTES + 1) as u64);
            let mut line = String::new();
            let response = match limited.read_line(&mut line).await {
                Ok(0) => ControlResponse::failure("empty control request", None),
                Ok(_) if line.len() > MAX_CONTROL_MESSAGE_BYTES => {
                    ControlResponse::failure("control request exceeds size limit", None)
                }
                Ok(_) => match serde_json::from_str::<ControlRequest>(line.trim_end()) {
                    Ok(request) => handle_request(&controller, request).await,
                    Err(error) => ControlResponse::failure(
                        format!("invalid control request: {error}"),
                        None,
                    ),
                },
                Err(error) => {
                    ControlResponse::failure(format!("control read failed: {error}"), None)
                }
            };

            if let Ok(mut encoded) = serde_json::to_vec(&response) {
                encoded.push(b'\n');
                let _ = writer.write_all(&encoded).await;
                let _ = writer.shutdown().await;
            }
        });
    }
}

#[cfg(not(unix))]
pub(crate) async fn serve_control_socket(
    _socket_path: PathBuf,
    _controller: DesktopRuntimeController,
) -> Result<(), ControlError> {
    Err(ControlError::UnsupportedPlatform)
}

#[cfg(unix)]
pub(crate) async fn send_control_request(
    socket_path: &Path,
    request: &ControlRequest,
) -> Result<ControlResponse, ControlError> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;

    let stream = UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut encoded = serde_json::to_vec(request)
        .map_err(|error| ControlError::Protocol(error.to_string()))?;
    encoded.push(b'\n');
    writer.write_all(&encoded).await?;
    writer.shutdown().await?;

    let mut limited = BufReader::new(reader).take((MAX_CONTROL_MESSAGE_BYTES + 1) as u64);
    let mut line = String::new();
    limited.read_line(&mut line).await?;
    if line.is_empty() {
        return Err(ControlError::Protocol("runtime returned no response".to_owned()));
    }
    if line.len() > MAX_CONTROL_MESSAGE_BYTES {
        return Err(ControlError::Protocol(
            "runtime response exceeds size limit".to_owned(),
        ));
    }
    serde_json::from_str(line.trim_end())
        .map_err(|error| ControlError::Protocol(error.to_string()))
}

#[cfg(not(unix))]
pub(crate) async fn send_control_request(
    _socket_path: &Path,
    _request: &ControlRequest,
) -> Result<ControlResponse, ControlError> {
    Err(ControlError::UnsupportedPlatform)
}

async fn handle_request(
    controller: &DesktopRuntimeController,
    request: ControlRequest,
) -> ControlResponse {
    let result = match request {
        ControlRequest::Status | ControlRequest::Stats | ControlRequest::ConfigShow => Ok(None),
        ControlRequest::Connect => controller
            .connect()
            .await
            .map(|_| Some("Codex connected".to_owned())),
        ControlRequest::Disconnect => controller
            .disconnect()
            .await
            .map(|_| Some("Codex disconnected".to_owned())),
        ControlRequest::Saving { enabled } => controller.set_saving_enabled(enabled).await.map(|_| {
            Some(format!(
                "token saving {}",
                if enabled { "enabled" } else { "disabled" }
            ))
        }),
        ControlRequest::ConfigSet { key, value } => match key.as_str() {
            "min-bytes" => controller
                .set_min_bytes(value)
                .await
                .map(|_| Some(format!("min-bytes set to {value}"))),
            "frontier" => controller
                .set_frontier(value)
                .await
                .map(|_| Some(format!("frontier set to {value}"))),
            "preview-code-units" => controller
                .set_preview_code_units(value)
                .await
                .map(|_| Some(format!("preview-code-units set to {value}"))),
            _ => Err(super::desktop_runtime::DesktopRuntimeError::Preferences(
                crate::modules::runtime::RuntimePreferencesError::InvalidValue(
                    "unknown aging policy key",
                ),
            )),
        },
    };

    match result {
        Ok(message) => {
            ControlResponse::success(snapshot_to_control(controller.snapshot().await), message)
        }
        Err(error) => ControlResponse::failure(
            error.to_string(),
            Some(snapshot_to_control(controller.snapshot().await)),
        ),
    }
}

fn snapshot_to_control(snapshot: DesktopRuntimeSnapshot) -> ControlSnapshot {
    ControlSnapshot {
        service: service_text(snapshot.service).to_owned(),
        codex: codex_text(snapshot.codex).to_owned(),
        saving_enabled: snapshot.saving_enabled,
        connect_on_launch: snapshot.connect_on_launch,
        active_requests: snapshot.active_requests,
        policy: policy_to_control(snapshot.policy),
        session: savings_to_control(snapshot.session),
        today: savings_to_control(snapshot.today),
        all_time: savings_to_control(snapshot.all_time),
        last_error: snapshot.last_error,
    }
}

fn policy_to_control(policy: AgingPolicyView) -> ControlPolicy {
    ControlPolicy {
        min_bytes: policy.min_bytes,
        frontier: policy.frontier,
        preview_code_units: policy.preview_code_units,
    }
}

fn savings_to_control(savings: SavingsView) -> ControlSavings {
    ControlSavings {
        bytes_saved: savings.bytes_saved,
        estimated_tokens_saved: savings.estimated_tokens_saved,
        tool_results_compacted: savings.tool_results_compacted,
        aged_requests: savings.aged_requests,
    }
}

fn service_text(state: DesktopServiceState) -> &'static str {
    match state {
        DesktopServiceState::Starting => "starting",
        DesktopServiceState::Running => "running",
        DesktopServiceState::Error => "error",
    }
}

fn codex_text(state: DesktopCodexState) -> &'static str {
    match state {
        DesktopCodexState::Disconnected => "disconnected",
        DesktopCodexState::Connecting => "connecting",
        DesktopCodexState::Connected => "connected",
        DesktopCodexState::Drifted => "drifted",
        DesktopCodexState::Error => "error",
    }
}
