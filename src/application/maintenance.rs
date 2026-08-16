use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::shared::paths::product_data_dir;

const SNAPSHOT_FILE: &str = "codex-config-snapshot.json";
const PREFERENCES_FILE: &str = "runtime-preferences.json";
const SAVINGS_FILE: &str = "savings.json";
const CONTROL_SOCKET_FILE: &str = "control.sock";
const OWNED_FILES: &[&str] = &[PREFERENCES_FILE, SAVINGS_FILE, CONTROL_SOCKET_FILE];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PurgeReport {
    pub(crate) removed_files: Vec<String>,
    pub(crate) preserved_entries: Vec<String>,
    pub(crate) removed_data_directory: bool,
}

#[derive(Debug)]
pub(crate) enum MaintenanceError {
    Io(io::Error),
    ActiveRestorationSnapshot,
}

impl fmt::Display for MaintenanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "TokenSaver maintenance I/O failed: {error}"),
            Self::ActiveRestorationSnapshot => write!(
                formatter,
                "TokenSaver restoration snapshot is still active; disconnect Codex safely before removing local state"
            ),
        }
    }
}

impl std::error::Error for MaintenanceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::ActiveRestorationSnapshot => None,
        }
    }
}

impl From<io::Error> for MaintenanceError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Remove only state files that TokenSaver owns.
///
/// This operation is deliberately non-recursive. An active restoration snapshot
/// blocks cleanup because deleting it could strand Codex on a dead loopback
/// endpoint with no proof of the original configuration. Unknown files are
/// preserved and reported rather than guessed to be disposable.
pub(crate) fn purge_owned_state() -> Result<PurgeReport, MaintenanceError> {
    let data_dir = product_data_dir()?;
    purge_owned_state_at(&data_dir)
}

fn purge_owned_state_at(data_dir: &Path) -> Result<PurgeReport, MaintenanceError> {
    if !data_dir.exists() {
        return Ok(PurgeReport {
            removed_data_directory: true,
            ..PurgeReport::default()
        });
    }

    if data_dir.join(SNAPSHOT_FILE).exists() {
        return Err(MaintenanceError::ActiveRestorationSnapshot);
    }

    let mut report = PurgeReport::default();

    for file_name in OWNED_FILES {
        remove_if_present(data_dir, file_name, &mut report)?;
        remove_known_atomic_temps(data_dir, file_name, &mut report)?;
    }

    let mut preserved = Vec::new();
    for entry in fs::read_dir(data_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        preserved.push(name);
    }
    preserved.sort();
    report.preserved_entries = preserved;

    if report.preserved_entries.is_empty() {
        match fs::remove_dir(data_dir) {
            Ok(()) => report.removed_data_directory = true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                report.removed_data_directory = true;
            }
            Err(error) => return Err(error.into()),
        }
    }

    Ok(report)
}

fn remove_if_present(
    data_dir: &Path,
    file_name: &str,
    report: &mut PurgeReport,
) -> Result<(), MaintenanceError> {
    let path = data_dir.join(file_name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            // Never follow or recursively remove directories through an expected
            // file name. Unknown shape is preserved for manual inspection.
            if !removable_owned_file_type(&metadata, file_name) {
                return Ok(());
            }
            fs::remove_file(&path)?;
            report.removed_files.push(file_name.to_owned());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn removable_owned_file_type(metadata: &fs::Metadata, file_name: &str) -> bool {
    if metadata.file_type().is_file() {
        return true;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        return file_name == CONTROL_SOCKET_FILE && metadata.file_type().is_socket();
    }

    #[cfg(not(unix))]
    {
        let _ = file_name;
        false
    }
}

fn remove_known_atomic_temps(
    data_dir: &Path,
    target_file_name: &str,
    report: &mut PurgeReport,
) -> Result<(), MaintenanceError> {
    let prefix = format!(".{target_file_name}.tokensaver-");
    for entry in fs::read_dir(data_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(&prefix) || !name.ends_with(".tmp") {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.file_type().is_file() {
            continue;
        }
        fs::remove_file(entry.path())?;
        report.removed_files.push(name);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{purge_owned_state_at, MaintenanceError};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn temp_root() -> std::path::PathBuf {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "tokensaver-maintenance-{}-{sequence}",
            std::process::id()
        ))
    }

    #[test]
    fn active_snapshot_blocks_all_cleanup() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join("codex-config-snapshot.json"), "snapshot").expect("snapshot");
        fs::write(root.join("savings.json"), "savings").expect("savings");

        let error = purge_owned_state_at(&root).expect_err("snapshot must block cleanup");
        assert!(matches!(error, MaintenanceError::ActiveRestorationSnapshot));
        assert!(root.join("savings.json").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn purge_preserves_unknown_entries_and_is_non_recursive() {
        let root = temp_root();
        fs::create_dir_all(root.join("keep-me")).expect("create root");
        fs::write(root.join("runtime-preferences.json"), "prefs").expect("prefs");
        fs::write(root.join("savings.json"), "savings").expect("savings");
        fs::write(root.join("unknown.txt"), "unknown").expect("unknown");

        let report = purge_owned_state_at(&root).expect("purge");
        assert!(!root.join("runtime-preferences.json").exists());
        assert!(!root.join("savings.json").exists());
        assert!(root.join("unknown.txt").exists());
        assert!(root.join("keep-me").exists());
        assert!(!report.removed_data_directory);
        assert!(report.preserved_entries.contains(&"unknown.txt".to_owned()));
        assert!(report.preserved_entries.contains(&"keep-me".to_owned()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn purge_removes_known_atomic_temp_files_only() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("create root");
        fs::write(
            root.join(".savings.json.tokensaver-11-2.tmp"),
            "temporary",
        )
        .expect("known temp");
        fs::write(root.join(".other.tokensaver-11-2.tmp"), "other").expect("other temp");

        let report = purge_owned_state_at(&root).expect("purge");
        assert!(!root.join(".savings.json.tokensaver-11-2.tmp").exists());
        assert!(root.join(".other.tokensaver-11-2.tmp").exists());
        assert!(!report.removed_data_directory);
        let _ = fs::remove_dir_all(root);
    }
}
