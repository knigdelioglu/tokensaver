use std::path::Path;

use crate::modules::codex_integration::{
    CodexConnectionState, codex_config_path, connection_state_with_snapshot, load_config_snapshot,
};
use crate::modules::diagnostics::{
    DiagnosticCheck, DiagnosticSeverity, codex_cli_check, first_party_reachability_check,
    owner_private_path_check, readable_file_check,
};
use crate::shared::paths::{control_socket_path, product_data_dir};
use crate::shared::security::redact_local_secrets;

use super::control::{ControlRequest, send_control_request};

const SNAPSHOT_FILE: &str = "codex-config-snapshot.json";
const SAVINGS_FILE: &str = "savings.json";
const PREFERENCES_FILE: &str = "runtime-preferences.json";
const CHATGPT_PROBE: &str = "https://chatgpt.com/backend-api/codex/models";
const OPENAI_PROBE: &str = "https://api.openai.com/v1/models";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DoctorSeverity {
    Pass,
    Warning,
    Failure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DoctorCheck {
    pub(crate) name: &'static str,
    pub(crate) severity: DoctorSeverity,
    pub(crate) detail: String,
}

#[derive(Clone, Debug)]
pub(crate) struct DoctorReport {
    pub(crate) checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    pub(crate) fn has_failures(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.severity == DoctorSeverity::Failure)
    }
}

pub(crate) async fn run_doctor() -> DoctorReport {
    let mut checks = Vec::new();
    checks.push(codex_cli_check());

    let data_dir = match product_data_dir() {
        Ok(path) => {
            checks.push(owner_private_path_check("tokensaver-data-dir", &path));
            Some(path)
        }
        Err(error) => {
            checks.push(DiagnosticCheck::failure(
                "tokensaver-data-dir",
                format!("cannot resolve application data directory: {error}"),
            ));
            None
        }
    };

    if let Some(data_dir) = &data_dir {
        checks.push(owner_private_path_check(
            "runtime-preferences",
            &data_dir.join(PREFERENCES_FILE),
        ));
        checks.push(owner_private_path_check(
            "savings-store",
            &data_dir.join(SAVINGS_FILE),
        ));
        checks.push(snapshot_check(data_dir));
    }

    match codex_config_path() {
        Ok(path) => checks.push(readable_file_check("codex-config", &path)),
        Err(error) => checks.push(DiagnosticCheck::failure(
            "codex-config",
            format!("cannot resolve Codex configuration: {error}"),
        )),
    }

    checks.push(runtime_control_check().await);
    checks.push(first_party_reachability_check("chatgpt-upstream", CHATGPT_PROBE).await);
    checks.push(first_party_reachability_check("openai-upstream", OPENAI_PROBE).await);

    DoctorReport {
        checks: checks.into_iter().map(map_check).collect(),
    }
}

fn snapshot_check(data_dir: &Path) -> DiagnosticCheck {
    let snapshot_path = data_dir.join(SNAPSHOT_FILE);
    if !snapshot_path.exists() {
        return DiagnosticCheck::pass(
            "codex-restoration-snapshot",
            "no active restoration snapshot",
        );
    }

    let private = owner_private_path_check("codex-restoration-snapshot", &snapshot_path);
    if private.severity == DiagnosticSeverity::Failure {
        return private;
    }

    if let Err(error) = load_config_snapshot(&snapshot_path) {
        return DiagnosticCheck::failure(
            "codex-restoration-snapshot",
            format!("snapshot cannot be validated: {error}"),
        );
    }
    let config_path = match codex_config_path() {
        Ok(path) => path,
        Err(error) => {
            return DiagnosticCheck::failure(
                "codex-restoration-snapshot",
                format!("snapshot exists but Codex config cannot be resolved: {error}"),
            );
        }
    };

    match connection_state_with_snapshot(&config_path, &snapshot_path) {
        Ok(CodexConnectionState::Connected) => DiagnosticCheck::pass(
            "codex-restoration-snapshot",
            "snapshot is valid and matches the TokenSaver-owned Codex configuration",
        ),
        Ok(CodexConnectionState::NotConnected) => DiagnosticCheck::warning(
            "codex-restoration-snapshot",
            "snapshot exists but Codex currently appears restored/disconnected",
        ),
        Ok(CodexConnectionState::Drifted) => DiagnosticCheck::failure(
            "codex-restoration-snapshot",
            "snapshot exists and TokenSaver-owned Codex configuration has drifted",
        ),
        Err(error) => DiagnosticCheck::failure(
            "codex-restoration-snapshot",
            format!("snapshot/config coherence check failed: {error}"),
        ),
    }
}

async fn runtime_control_check() -> DiagnosticCheck {
    let socket_path = match control_socket_path() {
        Ok(path) => path,
        Err(error) => {
            return DiagnosticCheck::failure(
                "runtime-control",
                format!("cannot resolve control socket path: {error}"),
            );
        }
    };

    match send_control_request(&socket_path, &ControlRequest::Status).await {
        Ok(response) if response.ok => match response.snapshot {
            Some(snapshot) if snapshot.dropped_telemetry_observations > 0 => {
                DiagnosticCheck::warning(
                    "runtime-control",
                    format!(
                        "runtime reachable; service={}, codex={}, active_requests={}; {} content-free telemetry observation(s) were dropped because the bounded queue was saturated",
                        snapshot.service,
                        snapshot.codex,
                        snapshot.active_requests,
                        snapshot.dropped_telemetry_observations
                    ),
                )
            }
            Some(snapshot) => DiagnosticCheck::pass(
                "runtime-control",
                format!(
                    "runtime reachable; service={}, codex={}, active_requests={}; telemetry queue has no recorded drops",
                    snapshot.service, snapshot.codex, snapshot.active_requests
                ),
            ),
            None => DiagnosticCheck::pass("runtime-control", "runtime reachable"),
        },
        Ok(response) => DiagnosticCheck::warning(
            "runtime-control",
            response
                .message
                .unwrap_or_else(|| "runtime returned an unsuccessful status".to_owned()),
        ),
        Err(_) => DiagnosticCheck::warning(
            "runtime-control",
            "menu-bar runtime is not reachable; start TokenSaver for connect/saving commands",
        ),
    }
}

fn map_check(check: DiagnosticCheck) -> DoctorCheck {
    DoctorCheck {
        name: check.name,
        severity: match check.severity {
            DiagnosticSeverity::Pass => DoctorSeverity::Pass,
            DiagnosticSeverity::Warning => DoctorSeverity::Warning,
            DiagnosticSeverity::Failure => DoctorSeverity::Failure,
        },
        detail: redact_local_secrets(&check.detail),
    }
}
