use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, value};

use crate::shared::filesystem::atomic_write_private;

const OPENAI_BASE_URL_KEY: &str = "openai_base_url";
const CHATGPT_BASE_URL_KEY: &str = "chatgpt_base_url";
const REALTIME_CALL_BASE_URL_KEY: &str = "experimental_realtime_webrtc_call_base_url";
const REALTIME_WS_BASE_URL_KEY: &str = "experimental_realtime_ws_base_url";
const DEFAULT_CHATGPT_BASE_URL: &str = "https://chatgpt.com/backend-api";
const DEFAULT_REALTIME_WS_BASE_URL: &str = "https://api.openai.com/v1";
const SNAPSHOT_SCHEMA_VERSION: u32 = 2;
const CAPABILITY_HEX_LENGTH: usize = 64;

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
    /// `Some` means TokenSaver added this key because the user had no value.
    /// `None` means a pre-existing user value was left untouched.
    pub(crate) installed_realtime_call_base_url: Option<String>,
    /// `Some` means TokenSaver added this key because the user had no value.
    /// `None` means a pre-existing user value was left untouched.
    pub(crate) installed_realtime_ws_base_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodexConnectionState {
    Connected,
    NotConnected,
    Drifted,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum CodexConfigError {
    InvalidToml(String),
    UnsupportedOpenAiBaseUrlType,
    UnsupportedChatGptBaseUrlType,
    UnsafeLoopbackUrl(String),
    SnapshotFormat(String),
    UnsupportedSnapshotVersion(u32),
    ActiveSnapshotDifferentEndpoint {
        installed: String,
        requested: String,
    },
    SnapshotDrift,
    Drift {
        key: &'static str,
        expected: Option<String>,
        actual: Option<String>,
    },
    Io(io::Error),
}

impl fmt::Display for CodexConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToml(_) => write!(formatter, "Codex config TOML is invalid"),
            Self::UnsupportedOpenAiBaseUrlType => {
                write!(
                    formatter,
                    "Codex openai_base_url must be a string when present"
                )
            }
            Self::UnsupportedChatGptBaseUrlType => {
                write!(
                    formatter,
                    "Codex chatgpt_base_url must be a string when present"
                )
            }
            Self::UnsafeLoopbackUrl(_) => write!(
                formatter,
                "TokenSaver base URL is not a valid managed loopback /v1 URL"
            ),
            Self::SnapshotFormat(_) => {
                write!(formatter, "TokenSaver config snapshot is invalid")
            }
            Self::UnsupportedSnapshotVersion(version) => {
                write!(
                    formatter,
                    "unsupported TokenSaver config snapshot version: {version}"
                )
            }
            Self::ActiveSnapshotDifferentEndpoint { .. } => write!(
                formatter,
                "TokenSaver already has an active managed endpoint; cleanly disconnect before replacing it"
            ),
            Self::SnapshotDrift => write!(
                formatter,
                "TokenSaver snapshot exists but Codex configuration has drifted; refusing automatic overwrite"
            ),
            Self::Drift { key, .. } => write!(
                formatter,
                "Codex {key} changed after TokenSaver connected; refusing to overwrite the changed value"
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

#[allow(dead_code)]
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
    let original_openai_base_url = read_original_openai_base_url(&document)?;
    let native_realtime_call_base_url = native_realtime_call_base_url(&document)?;

    document[OPENAI_BASE_URL_KEY] = value(loopback_base_url);

    let installed_realtime_call_base_url = install_if_absent(
        &mut document,
        REALTIME_CALL_BASE_URL_KEY,
        &native_realtime_call_base_url,
    );
    let installed_realtime_ws_base_url = install_if_absent(
        &mut document,
        REALTIME_WS_BASE_URL_KEY,
        DEFAULT_REALTIME_WS_BASE_URL,
    );

    let snapshot = CodexConfigSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        original_openai_base_url,
        installed_openai_base_url: loopback_base_url.to_owned(),
        installed_realtime_call_base_url,
        installed_realtime_ws_base_url,
    };
    Ok((document.to_string(), snapshot))
}

pub(super) fn disconnect_config_text(
    source: &str,
    snapshot: &CodexConfigSnapshot,
) -> Result<String, CodexConfigError> {
    validate_snapshot(snapshot)?;
    let mut document = parse_document(source)?;
    validate_owned_values(&document, snapshot, true)?;

    match &snapshot.original_openai_base_url {
        OriginalOpenAiBaseUrl::Absent => {
            document.as_table_mut().remove(OPENAI_BASE_URL_KEY);
        }
        OriginalOpenAiBaseUrl::Value(original) => {
            document[OPENAI_BASE_URL_KEY] = value(original.as_str());
        }
    }

    if snapshot.installed_realtime_call_base_url.is_some() {
        document.as_table_mut().remove(REALTIME_CALL_BASE_URL_KEY);
    }
    if snapshot.installed_realtime_ws_base_url.is_some() {
        document.as_table_mut().remove(REALTIME_WS_BASE_URL_KEY);
    }

    Ok(document.to_string())
}

pub(super) fn connection_state_text(
    source: &str,
    snapshot: &CodexConfigSnapshot,
) -> Result<CodexConnectionState, CodexConfigError> {
    validate_snapshot(snapshot)?;
    let document = parse_document(source)?;

    if validate_owned_values(&document, snapshot, true).is_ok() {
        return Ok(CodexConnectionState::Connected);
    }

    let original_openai_matches = match &snapshot.original_openai_base_url {
        OriginalOpenAiBaseUrl::Absent => {
            read_optional_string(&document, OPENAI_BASE_URL_KEY)?.is_none()
        }
        OriginalOpenAiBaseUrl::Value(original) => {
            read_optional_string(&document, OPENAI_BASE_URL_KEY)?.as_deref()
                == Some(original.as_str())
        }
    };
    let installed_realtime_absent = snapshot
        .installed_realtime_call_base_url
        .as_ref()
        .is_none_or(|_| {
            read_optional_string(&document, REALTIME_CALL_BASE_URL_KEY)
                .ok()
                .flatten()
                .is_none()
        })
        && snapshot
            .installed_realtime_ws_base_url
            .as_ref()
            .is_none_or(|_| {
                read_optional_string(&document, REALTIME_WS_BASE_URL_KEY)
                    .ok()
                    .flatten()
                    .is_none()
            });

    Ok(if original_openai_matches && installed_realtime_absent {
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

fn read_original_openai_base_url(
    document: &DocumentMut,
) -> Result<OriginalOpenAiBaseUrl, CodexConfigError> {
    match document.get(OPENAI_BASE_URL_KEY) {
        None | Some(Item::None) => Ok(OriginalOpenAiBaseUrl::Absent),
        Some(item) => item
            .as_str()
            .map(|value| OriginalOpenAiBaseUrl::Value(value.to_owned()))
            .ok_or(CodexConfigError::UnsupportedOpenAiBaseUrlType),
    }
}

fn read_optional_string(
    document: &DocumentMut,
    key: &'static str,
) -> Result<Option<String>, CodexConfigError> {
    match document.get(key) {
        None | Some(Item::None) => Ok(None),
        Some(item) => item
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| {
                if key == CHATGPT_BASE_URL_KEY {
                    CodexConfigError::UnsupportedChatGptBaseUrlType
                } else {
                    CodexConfigError::Drift {
                        key,
                        expected: None,
                        actual: None,
                    }
                }
            }),
    }
}

fn native_realtime_call_base_url(document: &DocumentMut) -> Result<String, CodexConfigError> {
    let base = read_optional_string(document, CHATGPT_BASE_URL_KEY)?
        .unwrap_or_else(|| DEFAULT_CHATGPT_BASE_URL.to_owned());
    let trimmed = base.trim_end_matches('/');
    Ok(if trimmed.ends_with("/codex") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/codex")
    })
}

fn install_if_absent(
    document: &mut DocumentMut,
    key: &'static str,
    installed: &str,
) -> Option<String> {
    if document.get(key).is_some() {
        return None;
    }
    document[key] = value(installed);
    Some(installed.to_owned())
}

fn validate_owned_values(
    document: &DocumentMut,
    snapshot: &CodexConfigSnapshot,
    expect_installed: bool,
) -> Result<(), CodexConfigError> {
    let expected_openai = if expect_installed {
        Some(snapshot.installed_openai_base_url.clone())
    } else {
        match &snapshot.original_openai_base_url {
            OriginalOpenAiBaseUrl::Absent => None,
            OriginalOpenAiBaseUrl::Value(value) => Some(value.clone()),
        }
    };
    validate_key(document, OPENAI_BASE_URL_KEY, expected_openai)?;

    if let Some(expected) = &snapshot.installed_realtime_call_base_url {
        validate_key(
            document,
            REALTIME_CALL_BASE_URL_KEY,
            if expect_installed {
                Some(expected.clone())
            } else {
                None
            },
        )?;
    }
    if let Some(expected) = &snapshot.installed_realtime_ws_base_url {
        validate_key(
            document,
            REALTIME_WS_BASE_URL_KEY,
            if expect_installed {
                Some(expected.clone())
            } else {
                None
            },
        )?;
    }
    Ok(())
}

fn validate_key(
    document: &DocumentMut,
    key: &'static str,
    expected: Option<String>,
) -> Result<(), CodexConfigError> {
    let actual = match document.get(key) {
        None | Some(Item::None) => None,
        Some(item) => item.as_str().map(str::to_owned),
    };
    if actual == expected {
        Ok(())
    } else {
        Err(CodexConfigError::Drift {
            key,
            expected,
            actual,
        })
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
    let Some(rest) = url.strip_prefix("http://127.0.0.1:") else {
        return Err(CodexConfigError::UnsafeLoopbackUrl(url.to_owned()));
    };
    if url.contains('?') || url.contains('#') || url.chars().any(char::is_whitespace) {
        return Err(CodexConfigError::UnsafeLoopbackUrl(url.to_owned()));
    }
    let Some((port, path)) = rest.split_once('/') else {
        return Err(CodexConfigError::UnsafeLoopbackUrl(url.to_owned()));
    };
    let Ok(port) = port.parse::<u16>() else {
        return Err(CodexConfigError::UnsafeLoopbackUrl(url.to_owned()));
    };
    let mut segments = path.split('/');
    let secret = segments.next().unwrap_or_default();
    let api_prefix = segments.next().unwrap_or_default();
    let valid = port != 0
        && secret.len() == CAPABILITY_HEX_LENGTH
        && secret.bytes().all(|byte| byte.is_ascii_hexdigit())
        && api_prefix == "v1"
        && segments.next().is_none();
    if valid {
        Ok(())
    } else {
        Err(CodexConfigError::UnsafeLoopbackUrl(url.to_owned()))
    }
}
