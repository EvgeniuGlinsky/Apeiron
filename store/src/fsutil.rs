//! Writing small files so that a sudden power-off leaves either the old or the new one.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::StorageError;

/// Name of the temporary file for `path`: the full name with `.tmp` appended.
///
/// Appended rather than substituted for the extension, so that `vault.bin` and a future
/// `vault.bak` never share `vault.tmp`.
pub(crate) fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

/// Writes `bytes` to `path` atomically and flushes it.
///
/// The order is not negotiable: temporary file → flush to disk → rename → flush the
/// directory. Writing over the file in place could leave it half-written, and for the
/// wrapper that means losing every conversation.
pub(crate) fn write_atomically(dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let tmp = tmp_path(path);
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;

    // The rename lands in the directory's metadata, and that has to be flushed too. On
    // Windows a directory cannot be opened as a file, but this build does not run there.
    #[cfg(unix)]
    {
        let dir_handle = fs::File::open(dir)?;
        dir_handle.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = dir;

    Ok(())
}

/// Removes `path` and its temporary file, if present.
pub(crate) fn remove_with_tmp(path: &Path) -> Result<(), StorageError> {
    for p in [path.to_path_buf(), tmp_path(path)] {
        if p.exists() {
            fs::remove_file(&p)?;
        }
    }
    Ok(())
}
