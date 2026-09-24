//! Where the app keeps its files.

use std::path::PathBuf;

/// Data directory of the app.
///
/// On Android the path comes from Kotlin (`context.filesDir`) at registration. Elsewhere
/// there is none: the desktop build is frozen by the project owner's decision.
#[cfg(target_os = "android")]
pub(crate) fn storage_dir() -> Result<PathBuf, String> {
    apeiron_platform::storage_dir()
        .map(|p| p.to_path_buf())
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
pub(crate) fn storage_dir() -> Result<PathBuf, String> {
    Err("no data directory on this platform: the build is frozen".to_string())
}
