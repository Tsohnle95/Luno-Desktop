use luno_desktop_lib::persistence::atomic_write::write_atomic;
use luno_desktop_lib::persistence::recovery::{cleanup, read_metadata, recover, RecoveryResult};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("luno-persistence-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn replaces_atomically_and_preserves_mode() {
    let dir = temp_dir();
    let path = dir.join("settings.json");
    fs::write(&path, b"old").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let receipt = write_atomic(&path, b"new").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"new");
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
    assert!(receipt.backup.exists());
    assert_eq!(read_metadata(&path).unwrap().unwrap().committed, true);
    cleanup(&path).unwrap();
    assert!(!receipt.backup.exists());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn missing_target_recovers_backup_and_cleans_partial_files() {
    let dir = temp_dir();
    let path = dir.join("state.json");
    fs::write(format!("{}.luno-backup", path.display()), b"safe").unwrap();
    fs::write(format!("{}.luno-tmp-123", path.display()), b"partial").unwrap();
    fs::write(format!("{}.luno-recovery.json", path.display()), b"not-json").unwrap();
    assert_eq!(recover(&path).unwrap(), RecoveryResult::RestoredBackup);
    assert_eq!(fs::read(&path).unwrap(), b"safe");
    assert!(read_metadata(&path).unwrap().is_none());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn malformed_metadata_is_ignored_and_cleanup_is_safe() {
    let dir = temp_dir();
    let path = dir.join("state.json");
    fs::write(format!("{}.luno-recovery.json", path.display()), b"{").unwrap();
    assert!(read_metadata(&path).unwrap().is_none());
    assert_eq!(recover(&path).unwrap(), RecoveryResult::RemovedCorruptArtifacts);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn concurrent_writers_leave_a_complete_value() {
    let dir = temp_dir();
    let path = Arc::new(dir.join("state.json"));
    let mut handles = Vec::new();
    for value in [b"one".as_slice(), b"two".as_slice(), b"three".as_slice()] {
        let path = Arc::clone(&path);
        let value = value.to_vec();
        handles.push(thread::spawn(move || write_atomic(&*path, &value).unwrap()));
    }
    for handle in handles { handle.join().unwrap(); }
    let value = fs::read(&*path).unwrap();
    assert!(value == b"one" || value == b"two" || value == b"three");
    let _ = fs::remove_dir_all(dir);
}
