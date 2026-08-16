use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CODEX_HOME_ENV: &str = "CODEX_HOME";
const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug)]
pub(crate) enum CodexPathError {
    HomeDirectoryUnavailable,
    CodexHomeMissing(PathBuf),
    CodexHomeNotDirectory(PathBuf),
    Io(io::Error),
}

impl fmt::Display for CodexPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HomeDirectoryUnavailable => write!(formatter, "home directory is unavailable"),
            Self::CodexHomeMissing(path) => {
                write!(formatter, "CODEX_HOME does not exist: {}", path.display())
            }
            Self::CodexHomeNotDirectory(path) => {
                write!(formatter, "CODEX_HOME is not a directory: {}", path.display())
            }
            Self::Io(error) => write!(formatter, "failed to resolve Codex home: {error}"),
        }
    }
}

impl std::error::Error for CodexPathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CodexPathError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub(crate) fn codex_config_path() -> Result<PathBuf, CodexPathError> {
    let codex_home = env::var_os(CODEX_HOME_ENV).filter(|value| !value.is_empty());
    resolve_codex_config_path(codex_home, dirs::home_dir())
}

pub(super) fn resolve_codex_config_path(
    codex_home: Option<OsString>,
    user_home: Option<PathBuf>,
) -> Result<PathBuf, CodexPathError> {
    let home = match codex_home {
        Some(value) => canonical_codex_home(Path::new(&value))?,
        None => user_home
            .ok_or(CodexPathError::HomeDirectoryUnavailable)?
            .join(".codex"),
    };

    Ok(home.join(CONFIG_FILE_NAME))
}

fn canonical_codex_home(path: &Path) -> Result<PathBuf, CodexPathError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(CodexPathError::CodexHomeMissing(path.to_path_buf()));
        }
        Err(error) => return Err(CodexPathError::Io(error)),
    };

    if !metadata.is_dir() {
        return Err(CodexPathError::CodexHomeNotDirectory(path.to_path_buf()));
    }

    Ok(path.canonicalize()?)
}
