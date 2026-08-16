use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use toml_edit::{value, DocumentMut, Item};

use crate::shared::filesystem::atomic_write_private;

const OWNED_KEY: &str = "openai_base_url";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OriginalOpenAiBaseUrl {
    Absent,
    Value(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodexConfigSnapshot {
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
    let source = fs::read_to_string(path)?;
    let next = disconnect_config_text(&source, snapshot)?;
    atomic_write_private(path, &next)?;
    Ok(())
}

pub(crate) fn connection_state_file(
    path: &Path,
    snapshot: &CodexConfigSnapshot,
) -> Result<CodexConnectionState, CodexConfigError> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(CodexConnectionState::Drifted);
        }
        Err(error) => return Err(CodexConfigError::Io(error)),
    };
    connection_state_text(&source, snapshot)
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
        original_openai_base_url: original,
        installed_openai_base_url: loopback_base_url.to_owned(),
    };
    Ok((document.to_string(), snapshot))
}

pub(super) fn disconnect_config_text(
    source: &str,
    snapshot: &CodexConfigSnapshot,
) -> Result<String, CodexConfigError> {
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

fn validate_loopback_base_url(url: &str) -> Result<(), CodexConfigError> {
    let safe_prefix = url.starts_with("http://127.0.0.1:") || url.starts_with("http://[::1]:");
    let has_capability_path = url
        .split_once("//")
        .and_then(|(_, rest)| rest.split_once('/'))
        .is_some_and(|(_, path)| !path.is_empty() && !path.contains(char::is_whitespace));

    if safe_prefix && has_capability_path && !url.contains(['?', '#']) {
        Ok(())
    } else {
        Err(CodexConfigError::UnsafeLoopbackUrl(url.to_owned()))
    }
}
