use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::shared::filesystem::atomic_write_private;

const PREFERENCES_SCHEMA_VERSION: u32 = 2;
const LEGACY_SCHEMA_VERSION: u32 = 1;

// Persistence defaults are intentionally duplicated at this boundary rather
// than importing the aging domain. `application::settings` owns the authored
// contract test that keeps these persisted defaults aligned with AgingPolicy.
const DEFAULT_MIN_BYTES: usize = 32 * 1024;
const DEFAULT_FRONTIER: usize = 4;
const DEFAULT_PREVIEW_CODE_UNITS: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RuntimePreferences {
    schema_version: u32,
    pub(crate) saving_enabled: bool,
    /// User intent, distinct from the temporary Codex config snapshot. Safe app
    /// shutdown restores Codex configuration but preserves this preference so a
    /// later launch/start-at-login can reconnect automatically.
    #[serde(default)]
    pub(crate) connect_on_launch: bool,
    #[serde(default = "default_min_bytes")]
    pub(crate) min_bytes: usize,
    #[serde(default = "default_frontier")]
    pub(crate) frontier: usize,
    #[serde(default = "default_preview_code_units")]
    pub(crate) preview_code_units: usize,
}

impl Default for RuntimePreferences {
    fn default() -> Self {
        Self {
            schema_version: PREFERENCES_SCHEMA_VERSION,
            // TokenSaver rewrites historical context when enabled. A fresh
            // install therefore starts conservative/off until the operator
            // explicitly opts in; an existing persisted preference is never
            // re-defaulted by this release.
            saving_enabled: false,
            connect_on_launch: false,
            min_bytes: DEFAULT_MIN_BYTES,
            frontier: DEFAULT_FRONTIER,
            preview_code_units: DEFAULT_PREVIEW_CODE_UNITS,
        }
    }
}

#[derive(Debug)]
pub(crate) enum RuntimePreferencesError {
    Io(io::Error),
    InvalidJson(String),
    UnsupportedSchema(u32),
    InvalidValue(&'static str),
}

impl fmt::Display for RuntimePreferencesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "runtime preferences I/O failed: {error}"),
            Self::InvalidJson(error) => {
                write!(formatter, "runtime preferences JSON is invalid: {error}")
            }
            Self::UnsupportedSchema(version) => {
                write!(
                    formatter,
                    "unsupported runtime preferences schema version: {version}"
                )
            }
            Self::InvalidValue(name) => write!(formatter, "invalid runtime preference: {name}"),
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
        let mut preferences = match fs::read_to_string(&path) {
            Ok(source) => {
                let preferences = serde_json::from_str::<RuntimePreferences>(&source)
                    .map_err(|error| RuntimePreferencesError::InvalidJson(error.to_string()))?;
                match preferences.schema_version {
                    PREFERENCES_SCHEMA_VERSION | LEGACY_SCHEMA_VERSION => preferences,
                    version => return Err(RuntimePreferencesError::UnsupportedSchema(version)),
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => RuntimePreferences::default(),
            Err(error) => return Err(RuntimePreferencesError::Io(error)),
        };
        validate(&preferences)?;
        preferences.schema_version = PREFERENCES_SCHEMA_VERSION;
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

    pub(crate) fn set_connect_on_launch(
        &mut self,
        enabled: bool,
    ) -> Result<(), RuntimePreferencesError> {
        self.preferences.connect_on_launch = enabled;
        self.save()
    }

    pub(crate) fn set_min_bytes(
        &mut self,
        min_bytes: usize,
    ) -> Result<(), RuntimePreferencesError> {
        if min_bytes == 0 {
            return Err(RuntimePreferencesError::InvalidValue(
                "min_bytes must be greater than zero",
            ));
        }
        self.preferences.min_bytes = min_bytes;
        self.save()
    }

    pub(crate) fn set_frontier(&mut self, frontier: usize) -> Result<(), RuntimePreferencesError> {
        if frontier > 256 {
            return Err(RuntimePreferencesError::InvalidValue(
                "frontier must be <= 256",
            ));
        }
        self.preferences.frontier = frontier;
        self.save()
    }

    pub(crate) fn set_preview_code_units(
        &mut self,
        preview_code_units: usize,
    ) -> Result<(), RuntimePreferencesError> {
        if !(64..=16_384).contains(&preview_code_units) {
            return Err(RuntimePreferencesError::InvalidValue(
                "preview_code_units must be between 64 and 16384",
            ));
        }
        self.preferences.preview_code_units = preview_code_units;
        self.save()
    }

    fn save(&self) -> Result<(), RuntimePreferencesError> {
        validate(&self.preferences)?;
        let serialized = serde_json::to_string_pretty(&self.preferences)
            .map_err(|error| RuntimePreferencesError::InvalidJson(error.to_string()))?;
        atomic_write_private(&self.path, &serialized)?;
        Ok(())
    }
}

fn validate(preferences: &RuntimePreferences) -> Result<(), RuntimePreferencesError> {
    if preferences.min_bytes == 0 {
        return Err(RuntimePreferencesError::InvalidValue(
            "min_bytes must be greater than zero",
        ));
    }
    if preferences.frontier > 256 {
        return Err(RuntimePreferencesError::InvalidValue(
            "frontier must be <= 256",
        ));
    }
    if !(64..=16_384).contains(&preferences.preview_code_units) {
        return Err(RuntimePreferencesError::InvalidValue(
            "preview_code_units must be between 64 and 16384",
        ));
    }
    Ok(())
}

const fn default_min_bytes() -> usize {
    DEFAULT_MIN_BYTES
}

const fn default_frontier() -> usize {
    DEFAULT_FRONTIER
}

const fn default_preview_code_units() -> usize {
    DEFAULT_PREVIEW_CODE_UNITS
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{
        DEFAULT_FRONTIER, DEFAULT_MIN_BYTES, DEFAULT_PREVIEW_CODE_UNITS, RuntimePreferencesError,
        RuntimePreferencesStore,
    };

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn temp_path(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tokensaver-runtime-preferences-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let path = root.join(name);
        (root, path)
    }

    #[test]
    fn fresh_install_defaults_saving_off() {
        let (root, path) = temp_path("runtime-preferences.json");
        let store = RuntimePreferencesStore::open(&path).expect("open fresh defaults");
        let preferences = store.preferences();
        assert!(!preferences.saving_enabled);
        assert!(!preferences.connect_on_launch);
        assert_eq!(preferences.min_bytes, DEFAULT_MIN_BYTES);
        assert_eq!(preferences.frontier, DEFAULT_FRONTIER);
        assert_eq!(preferences.preview_code_units, DEFAULT_PREVIEW_CODE_UNITS);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_v1_preserves_explicit_saving_choice_and_receives_policy_defaults() {
        let (root, path) = temp_path("runtime-preferences.json");
        fs::write(
            &path,
            r#"{
  "schema_version": 1,
  "saving_enabled": true,
  "connect_on_launch": true
}"#,
        )
        .expect("write legacy preferences");

        let store = RuntimePreferencesStore::open(&path).expect("open legacy preferences");
        let preferences = store.preferences();
        assert!(preferences.saving_enabled);
        assert!(preferences.connect_on_launch);
        assert_eq!(preferences.min_bytes, DEFAULT_MIN_BYTES);
        assert_eq!(preferences.frontier, DEFAULT_FRONTIER);
        assert_eq!(preferences.preview_code_units, DEFAULT_PREVIEW_CODE_UNITS);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_policy_values_are_rejected_before_persisting() {
        let (root, path) = temp_path("runtime-preferences.json");
        let mut store = RuntimePreferencesStore::open(&path).expect("open defaults");

        assert!(matches!(
            store.set_min_bytes(0),
            Err(RuntimePreferencesError::InvalidValue(_))
        ));
        assert!(matches!(
            store.set_frontier(257),
            Err(RuntimePreferencesError::InvalidValue(_))
        ));
        assert!(matches!(
            store.set_preview_code_units(63),
            Err(RuntimePreferencesError::InvalidValue(_))
        ));

        let _ = fs::remove_dir_all(root);
    }
}
