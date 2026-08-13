use luno_desktop_lib::preview::assets::*;
use std::fs;
use std::io::Write;
use std::sync::atomic::AtomicBool;

fn dir() -> std::path::PathBuf { let p = std::env::temp_dir().join(format!("luno-assets-{}", std::process::id())); let _ = fs::remove_dir_all(&p); fs::create_dir_all(&p).unwrap(); p }

#[test]
fn startup_and_session_cleanup_preserve_current_file() {
    let root = dir(); let old = root.join("old.mp3"); let current = root.join("current.mp3"); fs::write(&old, b"old").unwrap(); fs::write(&current, b"current").unwrap();
    let store = PreviewAssetStore::at(&root).unwrap(); assert!(!old.exists());
    fs::write(&current, b"current").unwrap(); store.cleanup_session(Some(&current)).unwrap(); assert!(current.exists());
}

#[test]
fn bounded_download_publishes_only_complete_asset() {
    let root = dir(); let mut store = PreviewAssetStore::at(&root).unwrap(); let generation = store.begin_generation(); let cancel = AtomicBool::new(false);
    let path = store.download("artist/title", generation, &cancel, |w| { w.write_all(b"ID3 preview").map_err(|e| e.to_string()) }).unwrap();
    assert!(path.starts_with(&root)); assert_eq!(fs::read(path).unwrap(), b"ID3 preview"); assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
}

#[test]
fn cancellation_timeout_and_traversal_never_leave_partial_or_ready() {
    let root = dir(); let mut store = PreviewAssetStore::at(&root).unwrap(); let generation = store.begin_generation(); let cancel = AtomicBool::new(true);
    assert!(matches!(store.download("x", generation, &cancel, |_| Ok(())), Err(AssetError::Cancelled)));
    let escaped = store.ready_path("../escape").unwrap(); assert!(escaped.starts_with(&root)); // separators are sanitized, not traversed
    let cancel = AtomicBool::new(false); assert!(matches!(store.resolve(generation + 1, &cancel, || Ok::<_, String>(())), Err(AssetError::StaleGeneration)));
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
}
