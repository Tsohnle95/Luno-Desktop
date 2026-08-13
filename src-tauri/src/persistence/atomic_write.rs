//! Durable, same-directory file replacement.

use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const LOCK_SUFFIX: &str = ".luno-lock";
const BACKUP_SUFFIX: &str = ".luno-backup";
const META_SUFFIX: &str = ".luno-recovery.json";

#[derive(Debug, Clone)]
pub struct WriteReceipt {
    pub path: PathBuf,
    pub backup: PathBuf,
    pub metadata: PathBuf,
}

#[derive(Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RecoveryMetadata {
    pub target: String,
    pub temporary: String,
    pub backup: String,
    pub committed: bool,
    pub created_unix_ms: u128,
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", path.display(), suffix))
}

fn unique_temp(path: &Path) -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    PathBuf::from(format!("{}.luno-tmp-{}-{}", path.display(), std::process::id(), stamp))
}

struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) { let _ = fs::remove_file(&self.0); }
}

fn acquire_lock(path: &Path) -> io::Result<Lock> {
    let lock = sibling(path, LOCK_SUFFIX);
    for _ in 0..200 {
        match OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id())?;
                file.sync_all()?;
                return Ok(Lock(lock));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => thread::sleep(Duration::from_millis(5)),
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(io::ErrorKind::WouldBlock, "timed out waiting for persistence writer"))
}

fn preserve_permissions(file: &File, target: &Path) -> io::Result<()> {
    if let Ok(metadata) = fs::metadata(target) {
        #[cfg(unix)]
        file.set_permissions(fs::Permissions::from_mode(metadata.permissions().mode()))?;
    }
    Ok(())
}

fn write_file(path: &Path, bytes: &[u8], target: &Path) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    preserve_permissions(&file, target)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Write bytes durably and replace `target` atomically. Existing contents are copied
/// to a durable sibling backup before promotion, and all artifacts share the target directory.
pub fn write_atomic(target: impl AsRef<Path>, bytes: &[u8]) -> io::Result<WriteReceipt> {
    let target = target.as_ref();
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let _lock = acquire_lock(target)?;
    let temporary = unique_temp(target);
    let backup = sibling(target, BACKUP_SUFFIX);
    let metadata_path = sibling(target, META_SUFFIX);
    let result = (|| {
        write_file(&temporary, bytes, target)?;
        if target.exists() {
            copy_durable(target, &backup)?;
        }
        let metadata = RecoveryMetadata {
            target: target.display().to_string(), temporary: temporary.display().to_string(),
            backup: backup.display().to_string(), committed: false,
            created_unix_ms: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        };
        write_json_sibling(&metadata_path, &metadata, target)?;
        fs::rename(&temporary, target)?;
        sync_directory(parent)?;
        let committed = RecoveryMetadata { committed: true, ..metadata };
        write_json_sibling(&metadata_path, &committed, target)?;
        Ok(WriteReceipt { path: target.to_path_buf(), backup, metadata: metadata_path })
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result
}

pub fn atomic_write(target: impl AsRef<Path>, bytes: &[u8]) -> io::Result<WriteReceipt> { write_atomic(target, bytes) }

fn copy_durable(from: &Path, to: &Path) -> io::Result<()> {
    let temp = unique_temp(to);
    let result = (|| { fs::copy(from, &temp)?; let file = OpenOptions::new().read(true).open(&temp)?; file.sync_all()?; fs::rename(&temp, to)?; sync_directory(to.parent().unwrap_or_else(|| Path::new("."))) })();
    if result.is_err() { let _ = fs::remove_file(temp); }
    result
}

fn write_json_sibling<T: Serialize>(path: &Path, value: &T, target: &Path) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    write_file(path, &bytes, target).or_else(|e| if e.kind() == io::ErrorKind::AlreadyExists { fs::remove_file(path)?; write_file(path, &bytes, target) } else { Err(e) })
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)] { File::open(path)?.sync_all()?; }
    Ok(())
}

pub(crate) fn suffixes() -> (&'static str, &'static str, &'static str) { (LOCK_SUFFIX, BACKUP_SUFFIX, META_SUFFIX) }
