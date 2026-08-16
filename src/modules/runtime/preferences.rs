use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::shared::filesystem::atomic_write_private;

const PREFERENCES_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RuntimePreferences {
    schema_version: u32,
    pub(crate) saving_enabled: bool,
}

impl Default for RuntimePreferences {
    fn default() -> Self {
        Self {
            schema_version: PREFERENCES_SCHEMA_VERSION,
            saving_enabled: true,
        }
    }
}

#[derive(Debug)]
pub(crate) enum RuntimePreferencesError {
    Io(io::Error),
    InvalidJson(String),
    UnsupportedSchema(u32),
}

impl fmt::Display for RuntimePreferencesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "runtime preferences I/O failed: {error}"),
            Self::InvalidJson(error) => write!(formatter, "runtime preferences JSON is invalid: {error}"),
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported runtime preferences schema version: {version}")
            }
        }
    }
}

impl std::error::Error for RuntimePreferencesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for RuntimePreferencesError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub(crate) struct RuntimePreferencesStore {
    path: PathBuf,
    preferences: RuntimePreferences,
}

impl RuntimePreferencesStore {
    pub(crate) fn open(path: impl Into<PathBuf>) -> Result<Self, RuntimePreferencesError> {
        let path = path.into();
        let preferences = match fs::read_to_string(&path) {
            Ok(source) => {
                let preferences = serde_json::from_str::<RuntimePreferences>(&source)
                    .map_err(|error| RuntimePreferencesError::InvalidJson(error.to_string()))?;
                if preferences.schema_version != PREFERENCES_SCHEMA_VERSION {
                    return Err(RuntimePreferencesError::UnsupportedSchema(
                        preferences.schema_version,
                    ));
                }
                preferences
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => RuntimePreferences::default(),
            Err(error) => return Err(RuntimePreferencesError::Io(error)),
        };
        Ok(Self { path, preferences })
    }

    pub(crate) fn preferences(&self) -> RuntimePreferences {
        self.preferences
    }

    pub(crate) fn set_saving_enabled(
        &mut self,
        enabled: bool,
    ) -> Result<(), RuntimePreferencesError> {
        self.preferences.saving_enabled = enabled;
        self.save()
    }

    fn save(&self) -> Result<(), RuntimePreferencesError> {
        let serialized = serde_json::to_string_pretty(&self.preferences)
            .map_err(|error| RuntimePreferencesError::InvalidJson(error.to_string()))?;
        atomic_write_private(&self.path, &serialized)?;
        Ok(())
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}
