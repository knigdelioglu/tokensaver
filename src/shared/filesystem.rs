use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Atomically replace a user-owned text file while keeping temporary bytes in
/// the same directory. Same-directory rename avoids exposing a partially
/// written Codex configuration.
pub(crate) fn atomic_write_private(path: &Path, contents: &str) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "target path has no parent directory")
    })?;
    fs::create_dir_all(parent)?;

    let permissions = fs::metadata(path).ok().map(|metadata| metadata.permissions());
    let temporary = temporary_path(path);

    let write_result = (|| -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        let mut file = options.open(&temporary)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }

        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        drop(file);

        if let Some(permissions) = permissions {
            fs::set_permissions(&temporary, permissions)?;
        }

        fs::rename(&temporary, path)?;

        #[cfg(unix)]
        {
            if let Ok(directory) = OpenOptions::new().read(true).open(parent) {
                let _ = directory.sync_all();
            }
        }

        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }

    write_result
}

fn temporary_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    let process_id = std::process::id();
    path.with_file_name(format!(".{file_name}.tokensaver-{process_id}.tmp"))
}
