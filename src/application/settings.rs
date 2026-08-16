use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use crate::modules::runtime::{RuntimePreferencesError, RuntimePreferencesStore};
use crate::shared::paths::product_data_dir;

const PREFERENCES_FILE: &str = "runtime-preferences.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingsSnapshot {
    pub(crate) saving_enabled: bool,
    pub(crate) connect_on_launch: bool,
    pub(crate) min_bytes: usize,
    pub(crate) frontier: usize,
    pub(crate) preview_code_units: usize,
}

#[derive(Debug)]
pub(crate) enum SettingsError {
    Io(io::Error),
    Preferences(RuntimePreferencesError),
    UnknownKey(String),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "settings I/O failed: {error}"),
            Self::Preferences(error) => write!(formatter, "settings failed: {error}"),
            Self::UnknownKey(key) => write!(formatter, "unknown TokenSaver setting: {key}"),
        }
    }
}

impl std::error::Error for SettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Preferences(error) => Some(error),
            Self::UnknownKey(_) => None,
        }
    }
}

impl From<io::Error> for SettingsError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<RuntimePreferencesError> for SettingsError {
    fn from(error: RuntimePreferencesError) -> Self {
        Self::Preferences(error)
    }
}

pub(crate) fn load_product_settings() -> Result<SettingsSnapshot, SettingsError> {
    let data_dir = product_data_dir()?;
    load_settings(&data_dir)
}

pub(crate) fn set_product_numeric_setting(
    key: &str,
    value: usize,
) -> Result<SettingsSnapshot, SettingsError> {
    let data_dir = product_data_dir()?;
    set_numeric_setting_offline(&data_dir, key, value)
}

fn load_settings(data_dir: &Path) -> Result<SettingsSnapshot, SettingsError> {
    fs::create_dir_all(data_dir)?;
    let store = RuntimePreferencesStore::open(data_dir.join(PREFERENCES_FILE))?;
    Ok(snapshot(store.preferences()))
}

fn set_numeric_setting_offline(
    data_dir: &Path,
    key: &str,
    value: usize,
) -> Result<SettingsSnapshot, SettingsError> {
    fs::create_dir_all(data_dir)?;
    let mut store = RuntimePreferencesStore::open(data_dir.join(PREFERENCES_FILE))?;
    match key {
        "min-bytes" => store.set_min_bytes(value)?,
        "frontier" => store.set_frontier(value)?,
        "preview-code-units" => store.set_preview_code_units(value)?,
        _ => return Err(SettingsError::UnknownKey(key.to_owned())),
    }
    Ok(snapshot(store.preferences()))
}

fn snapshot(preferences: crate::modules::runtime::RuntimePreferences) -> SettingsSnapshot {
    SettingsSnapshot {
        saving_enabled: preferences.saving_enabled,
        connect_on_launch: preferences.connect_on_launch,
        min_bytes: preferences.min_bytes,
        frontier: preferences.frontier,
        preview_code_units: preferences.preview_code_units,
    }
}
