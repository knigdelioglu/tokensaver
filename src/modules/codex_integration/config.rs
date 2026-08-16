use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use toml_edit::{value, DocumentMut, Item};

use crate::shared::filesystem::atomic_write_private;

const OWNED_KEY: &str = "openai_base_url";
const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "value")]
pub(crate) enum OriginalOpenAiBaseUrl {
    Absent,
    Value(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CodexConfigSnapshot {
    schema_version: u32,
    pub(crate) original_openai_base_url: OriginalOpenAiBaseUrl,
    pub(crate) installed_openai_base_url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodexConnectionState {
    Connected,
    NotConnected,
    Drifted,
}

#[derive(Debug)]
pub(crate) enum CodexConfigError {
    InvalidToml(String),
    UnsupportedOpenAiBaseUrlType,
    UnsafeLoopbackUrl(String),
    SnapshotFormat(String),
    UnsupportedSnapshotVersion(u32),
    ActiveSnapshotDifferentEndpoint {
        installed: String,
        requested: String,
    },
    SnapshotDrift,
    Drift {
        expected: String,
        actual: Option<String>,
    },
    Io(io::Error),
}

impl fmt::Display for CodexConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToml(error) => write!(formatter, "invalid Codex config TOML: {error}"),
            Self::UnsupportedOpenAiBaseUrlType => {
                write!(formatter, "Codex openai_base_url must be a string when present")
            }
            Self::UnsafeLoopbackUrl(url) => {
                write!(formatter, "TokenSaver base URL is not loopback-only: {url}")
            }
            Self::SnapshotFormat(error) => write!(formatter, "invalid TokenSaver config snapshot: {error}"),
            Self::UnsupportedSnapshotVersion(version) => {
                write!(formatter, "unsupported TokenSaver config snapshot version: {version}")
            }
            Self::ActiveSnapshotDifferentEndpoint { installed, requested } => write!(
                formatter,
                "TokenSaver is already connected at {installed:?}; refusing to replace it with {requested:?} without a clean disconnect"
            ),
            Self::SnapshotDrift => write!(
                formatter,
                "TokenSaver snapshot exists but Codex configuration has drifted; refusing automatic overwrite"
            ),
            Self::Drift { expected, actual } => write!(
                formatter,
                "Codex openai_base_url changed after TokenSaver connected; expected {expected:?}, found {actual:?}"
            ),
            Self::Io(error) => write!(formatter, "Codex config I/O failed: {error}"),
        }
    }
}

impl std::error::Error for CodexConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CodexConfigError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Crash-safe connection transaction.
///
/// The restoration snapshot is durably written before the Codex config is
/// changed. An existing active snapshot is never silently replaced.
pub(crate) fn connect_with_snapshot(
    config_path: &Path,
    snapshot_path: &Path,
    loopback_base_url: &str,
) -> Result<CodexConfigSnapshot, CodexConfigError> {
    validate_loopback_base_url(loopback_base_url)?;

    if snapshot_path.exists() {
        let existing = load_config_snapshot(snapshot_path)?;
        match connection_state_file(config_path, &existing)? {
            CodexConnectionState::Connected => {
                if existing.installed_openai_base_url == loopback_base_url {
                    return Ok(existing);
                }
                return Err(CodexConfigError::ActiveSnapshotDifferentEndpoint {
                    installed: existing.installed_openai_base_url,
                    requested: loopback_base_url.to_owned(),
                });
            }
            CodexConnectionState::Drifted => return Err(CodexConfigError::SnapshotDrift),
            CodexConnectionState::NotConnected => {
                fs::remove_file(snapshot_path)?;
            }
        }
    }

    let source = match fs::read_to_string(config_path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(CodexConfigError::Io(error)),
    };
    let (next, snapshot) = connect_config_text(&source, loopback_base_url)?;

    save_config_snapshot(snapshot_path, &snapshot)?;
    if let Err(error) = atomic_write_private(config_path, &next) {
        let _ = fs::remove_file(snapshot_path);
        return Err(CodexConfigError::Io(error));
    }

    Ok(snapshot)
}

pub(crate) fn disconnect_with_snapshot(
    config_path: &Path,
    snapshot_path: &Path,
) -> Result<CodexConfigSnapshot, CodexConfigError> {
    let snapshot = load_config_snapshot(snapshot_path)?;
    disconnect_config_file(config_path, &snapshot)?;
    match fs::remove_file(snapshot_path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(CodexConfigError::Io(error)),
    }
    Ok(snapshot)
}

pub(crate) fn connection_state_with_snapshot(
    config_path: &Path,
    snapshot_path: &Path,
) -> Result<CodexConnectionState, CodexConfigError> {
    if !snapshot_path.exists() {
        return Ok(CodexConnectionState::NotConnected);
    }
    let snapshot = load_config_snapshot(snapshot_path)?;
    connection_state_file(config_path, &snapshot)
}

pub(crate) fn connect_config_file(
    path: &Path,
    loopback_base_url: &str,
) -> Result<CodexConfigSnapshot, CodexConfigError> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(CodexConfigError::Io(error)),
    };
    let (next, snapshot) = connect_config_text(&source, loopback_base_url)?;
    atomic_write_private(path, &next)?;
    Ok(snapshot)
}

pub(crate) fn disconnect_config_file(
    path: &Path,
    snapshot: &CodexConfigSnapshot,
) -> Result<(), CodexConfigError> {
    validate_snapshot(snapshot)?;
    let source = fs::read_to_string(path)?;
    let next = disconnect_config_text(&source, snapshot)?;
    atomic_write_private(path, &next)?;
    Ok(())
}

pub(crate) fn connection_state_file(
    path: &Path,
    snapshot: &CodexConfigSnapshot,
) -> Result<CodexConnectionState, CodexConfigError> {
    validate_snapshot(snapshot)?;
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(CodexConnectionState::Drifted);
        }
        Err(error) => return Err(CodexConfigError::Io(error)),
    };
    connection_state_text(&source, snapshot)
}

pub(crate) fn save_config_snapshot(
    path: &Path,
    snapshot: &CodexConfigSnapshot,
) -> Result<(), CodexConfigError> {
    validate_snapshot(snapshot)?;
    let serialized = serde_json::to_string_pretty(snapshot)
        .map_err(|error| CodexConfigError::SnapshotFormat(error.to_string()))?;
    atomic_write_private(path, &serialized)?;
    Ok(())
}

pub(crate) fn load_config_snapshot(path: &Path) -> Result<CodexConfigSnapshot, CodexConfigError> {
    let source = fs::read_to_string(path)?;
    let snapshot = serde_json::from_str::<CodexConfigSnapshot>(&source)
        .map_err(|error| CodexConfigError::SnapshotFormat(error.to_string()))?;
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

pub(super) fn connect_config_text(
    source: &str,
    loopback_base_url: &str,
) -> Result<(String, CodexConfigSnapshot), CodexConfigError> {
    validate_loopback_base_url(loopback_base_url)?;
    let mut document = parse_document(source)?;
    let original = read_original_value(&document)?;

    document[OWNED_KEY] = value(loopback_base_url);
    let snapshot = CodexConfigSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        original_openai_base_url: original,
        installed_openai_base_url: loopback_base_url.to_owned(),
    };
    Ok((document.to_string(), snapshot))
}

pub(super) fn disconnect_config_text(
    source: &str,
    snapshot: &CodexConfigSnapshot,
) -> Result<String, CodexConfigError> {
    validate_snapshot(snapshot)?;
    let mut document = parse_document(source)?;
    let current = read_current_string(&document)?;

    if current.as_deref() != Some(snapshot.installed_openai_base_url.as_str()) {
        return Err(CodexConfigError::Drift {
            expected: snapshot.installed_openai_base_url.clone(),
            actual: current,
        });
    }

    match &snapshot.original_openai_base_url {
        OriginalOpenAiBaseUrl::Absent => {
            document.as_table_mut().remove(OWNED_KEY);
        }
        OriginalOpenAiBaseUrl::Value(original) => {
            document[OWNED_KEY] = value(original.as_str());
        }
    }

    Ok(document.to_string())
}

pub(super) fn connection_state_text(
    source: &str,
    snapshot: &CodexConfigSnapshot,
) -> Result<CodexConnectionState, CodexConfigError> {
    validate_snapshot(snapshot)?;
    let document = parse_document(source)?;
    let current = read_current_string(&document)?;
    if current.as_deref() == Some(snapshot.installed_openai_base_url.as_str()) {
        return Ok(CodexConnectionState::Connected);
    }

    let original_matches = match &snapshot.original_openai_base_url {
        OriginalOpenAiBaseUrl::Absent => current.is_none(),
        OriginalOpenAiBaseUrl::Value(original) => current.as_deref() == Some(original.as_str()),
    };

    Ok(if original_matches {
        CodexConnectionState::NotConnected
    } else {
        CodexConnectionState::Drifted
    })
}

fn parse_document(source: &str) -> Result<DocumentMut, CodexConfigError> {
    if source.trim().is_empty() {
        return Ok(DocumentMut::new());
    }
    source
        .parse::<DocumentMut>()
        .map_err(|error| CodexConfigError::InvalidToml(error.to_string()))
}

fn read_original_value(document: &DocumentMut) -> Result<OriginalOpenAiBaseUrl, CodexConfigError> {
    match document.get(OWNED_KEY) {
        None | Some(Item::None) => Ok(OriginalOpenAiBaseUrl::Absent),
        Some(item) => item
            .as_str()
            .map(|value| OriginalOpenAiBaseUrl::Value(value.to_owned()))
            .ok_or(CodexConfigError::UnsupportedOpenAiBaseUrlType),
    }
}

fn read_current_string(document: &DocumentMut) -> Result<Option<String>, CodexConfigError> {
    match read_original_value(document)? {
        OriginalOpenAiBaseUrl::Absent => Ok(None),
        OriginalOpenAiBaseUrl::Value(value) => Ok(Some(value)),
    }
}

fn validate_snapshot(snapshot: &CodexConfigSnapshot) -> Result<(), CodexConfigError> {
    if snapshot.schema_version == SNAPSHOT_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(CodexConfigError::UnsupportedSnapshotVersion(
            snapshot.schema_version,
        ))
    }
}

fn validate_loopback_base_url(url: &str) -> Result<(), CodexConfigError> {
    let safe_prefix = url.starts_with("http://127.0.0.1:") || url.starts_with("http://[::1]:");
    let has_capability_path = url
        .split_once("//")
        .and_then(|(_, rest)| rest.split_once('/'))
        .is_some_and(|(_, path)| !path.is_empty() && !path.chars().any(char::is_whitespace));
    let has_forbidden_suffix = url.contains('?') || url.contains('#');

    if safe_prefix && has_capability_path && !has_forbidden_suffix {
        Ok(())
    } else {
        Err(CodexConfigError::UnsafeLoopbackUrl(url.to_owned()))
    }
}
