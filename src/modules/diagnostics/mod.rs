//! Diagnostics boundary.
//!
//! This module owns redacted health primitives used by `application::doctor`.
//! It never reads tool-result bodies, receipts, bearer credentials, account IDs,
//! or TokenSaver capability values.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const PINNED_CODEX_BASELINE: &str = "9ded177ce7c1c0bd2047f902936c177612ab3434";
// Populate only after the exact reported CLI version has passed the release
// validation suite against TokenSaver. An empty list intentionally means that
// no installed Codex build is yet release-certified.
const VALIDATED_CODEX_CLI_VERSIONS: &[&str] = &[];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticSeverity {
    Pass,
    Warning,
    Failure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticCheck {
    pub(crate) name: &'static str,
    pub(crate) severity: DiagnosticSeverity,
    pub(crate) detail: String,
}

impl DiagnosticCheck {
    pub(crate) fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            severity: DiagnosticSeverity::Pass,
            detail: detail.into(),
        }
    }

    pub(crate) fn warning(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            severity: DiagnosticSeverity::Warning,
            detail: detail.into(),
        }
    }

    pub(crate) fn failure(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            severity: DiagnosticSeverity::Failure,
            detail: detail.into(),
        }
    }
}

pub(crate) fn codex_cli_check() -> DiagnosticCheck {
    let Some(path) = find_codex_executable() else {
        return DiagnosticCheck::warning(
            "codex-cli",
            "Codex CLI was not found in PATH or common user install locations; desktop Codex may still be installed",
        );
    };

    match Command::new(&path).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if version.is_empty() {
                return DiagnosticCheck::warning(
                    "codex-cli",
                    "Codex version command succeeded but returned no version identity",
                );
            }
            if VALIDATED_CODEX_CLI_VERSIONS.contains(&version.as_str()) {
                DiagnosticCheck::pass(
                    "codex-cli",
                    format!("{version}; release-validated against TokenSaver protocol baseline"),
                )
            } else {
                DiagnosticCheck::warning(
                    "codex-cli",
                    format!(
                        "{version}; this exact Codex build has not yet passed TokenSaver release validation (protocol baseline {PINNED_CODEX_BASELINE})"
                    ),
                )
            }
        }
        Ok(output) => DiagnosticCheck::warning(
            "codex-cli",
            format!(
                "Codex executable found but --version exited with {}",
                output.status
            ),
        ),
        Err(error) => DiagnosticCheck::warning(
            "codex-cli",
            format!("Codex executable found but could not be queried: {error}"),
        ),
    }
}

pub(crate) fn readable_file_check(name: &'static str, path: &Path) -> DiagnosticCheck {
    match fs::File::open(path) {
        Ok(_) => DiagnosticCheck::pass(name, "readable"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            DiagnosticCheck::warning(name, "not present")
        }
        Err(error) => DiagnosticCheck::failure(name, format!("not readable: {error}")),
    }
}

pub(crate) fn owner_private_path_check(name: &'static str, path: &Path) -> DiagnosticCheck {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return DiagnosticCheck::warning(name, "not present");
        }
        Err(error) => {
            return DiagnosticCheck::failure(name, format!("metadata unavailable: {error}"));
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode() & 0o777;
        let forbidden = mode & 0o077;
        if forbidden == 0 {
            DiagnosticCheck::pass(name, format!("owner-private permissions {:03o}", mode))
        } else {
            DiagnosticCheck::failure(
                name,
                format!("permissions {:03o} allow group/other access", mode),
            )
        }
    }

    #[cfg(not(unix))]
    {
        let _ = metadata;
        DiagnosticCheck::warning(name, "permission-mode check is Unix-specific")
    }
}

pub(crate) async fn first_party_reachability_check(
    name: &'static str,
    url: &'static str,
) -> DiagnosticCheck {
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return DiagnosticCheck::failure(name, format!("HTTP client creation failed: {error}"));
        }
    };

    match client.head(url).send().await {
        Ok(response) => DiagnosticCheck::pass(
            name,
            format!("first-party host reachable (HTTP {})", response.status()),
        ),
        Err(error) => DiagnosticCheck::warning(name, format!("host probe failed: {error}")),
    }
}

fn find_codex_executable() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(env::split_paths(&path).map(|directory| directory.join("codex")));
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local/bin/codex"));
        candidates.push(home.join(".cargo/bin/codex"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/codex"));
    candidates.push(PathBuf::from("/usr/local/bin/codex"));

    candidates.into_iter().find(|candidate| candidate.is_file())
}
