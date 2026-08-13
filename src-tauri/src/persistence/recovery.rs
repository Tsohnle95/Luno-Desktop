//! Recovery and cleanup for interrupted atomic writes.

use super::atomic_write::{suffixes, RecoveryMetadata};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum RecoveryResult { NothingToDo, RestoredBackup, RemovedCorruptArtifacts }

/// Restore a durable backup only when the target is absent. Existing target data is
/// never overwritten by recovery; callers can validate it before invoking this function.
pub fn recover(target: impl AsRef<Path>) -> io::Result<RecoveryResult> {
    let target = target.as_ref();
    if target.exists() { cleanup(target)?; return Ok(RecoveryResult::NothingToDo); }
    let backup = PathBuf::from(format!("{}{}", target.display(), suffixes().1));
    if backup.exists() {
        fs::rename(&backup, target)?;
        if let Some(parent) = target.parent() { sync_directory(parent)?; }
        cleanup(target)?;
        return Ok(RecoveryResult::RestoredBackup);
    }
    let removed = cleanup(target)?;
    Ok(if removed { RecoveryResult::RemovedCorruptArtifacts } else { RecoveryResult::NothingToDo })
}

/// Remove stale lock, metadata, backup, and temporary siblings after a successful read.
/// This deliberately does not remove the current target.
pub fn cleanup(target: impl AsRef<Path>) -> io::Result<bool> {
    let target = target.as_ref();
    let mut removed = false;
    for path in [
        PathBuf::from(format!("{}{}", target.display(), suffixes().0)),
        PathBuf::from(format!("{}{}", target.display(), suffixes().1)),
        PathBuf::from(format!("{}{}", target.display(), suffixes().2)),
    ] { if fs::remove_file(path).is_ok() { removed = true; } }
    if let Some(parent) = target.parent() {
        let prefix = format!("{}.luno-tmp-", target.display());
        for entry in fs::read_dir(parent)? {
            let path = entry?.path();
            if path.display().to_string().starts_with(&prefix) && fs::remove_file(path).is_ok() { removed = true; }
        }
    }
    Ok(removed)
}

/// Parse recovery metadata without treating malformed metadata as an application error.
pub fn read_metadata(target: impl AsRef<Path>) -> io::Result<Option<RecoveryMetadata>> {
    let path = PathBuf::from(format!("{}{}", target.as_ref().display(), suffixes().2));
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn sync_directory(path: &Path) -> io::Result<()> { fs::File::open(path)?.sync_all() }
