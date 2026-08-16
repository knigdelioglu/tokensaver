use super::control::{send_control_request, ControlError, ControlRequest, ControlResponse};
use crate::shared::paths::control_socket_path;

/// Product-edge client for the single live TokenSaver runtime. CLI code uses
/// this application service instead of resolving or opening the control socket
/// directly.
pub(crate) async fn send_runtime_request(
    request: &ControlRequest,
) -> Result<ControlResponse, ControlError> {
    let socket_path = control_socket_path().map_err(ControlError::Io)?;
    send_control_request(&socket_path, request).await
}
