use std::fs;
use std::io;
use std::path::PathBuf;

pub(crate) const APP_IDENTIFIER: &str = "com.knigdelioglu.tokensaver";
pub(crate) const CONTROL_SOCKET_FILE: &str = "control.sock";

/// Resolve the same per-user application-data directory used by the macOS
/// desktop runtime and the CLI. Keeping this in one low-level helper prevents
/// the two product edges from inventing separate state roots.
pub(crate) fn product_data_dir() -> io::Result<PathBuf> {
    let root = dirs::data_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "unable to resolve the user application-data directory",
        )
    })?;
    Ok(root.join(APP_IDENTIFIER))
}

/// Create the TokenSaver state root and ensure it is owner-private on Unix.
/// Individual sensitive files are still written through `atomic_write_private`.
pub(crate) fn ensure_product_data_dir() -> io::Result<PathBuf> {
    let path = product_data_dir()?;
    fs::create_dir_all(&path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    }

    Ok(path)
}

pub(crate) fn control_socket_path() -> io::Result<PathBuf> {
    Ok(product_data_dir()?.join(CONTROL_SOCKET_FILE))
}
