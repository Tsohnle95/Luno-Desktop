use luno_desktop_lib::library::index::{LibraryIndex, RefreshRequest};
use luno_desktop_lib::library::mutations::{MutationError, PlaylistService};
use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    let path = std::env::temp_dir().join(format!("luno-playlists-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn create_sanitizes_and_requests_refresh_but_rejects_collision() {
    let root = root(); let service = PlaylistService::new(&root); let mut index = LibraryIndex::default();
    let result = service.create(" Rock:/* ", &mut index).unwrap();
    assert_eq!(result.value.file_name().unwrap(), "Rock");
    assert!(matches!(result.refresh, RefreshRequest::Started { .. }));
    assert_eq!(service.create("Rock", &mut index), Err(MutationError::Collision("Rock".into())));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn protected_roots_and_invalid_urls_are_never_mutated() {
    let root = root(); let service = PlaylistService::new(&root); let mut index = LibraryIndex::default();
    assert_eq!(service.create("Unsorted", &mut index), Err(MutationError::Protected("Unsorted".into())));
    service.create("Mix", &mut index).unwrap();
    assert_eq!(service.set_url("Mix", Some("file:///tmp/x"), &mut index), Err(MutationError::InvalidUrl));
    assert!(service.set_url("Mix", Some("https://example.test/list"), &mut index).is_ok());
    assert!(root.join(".luno_playlist_urls.json").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dump_preserves_destination_collisions_and_reports_partial_errors() {
    let root = root(); let service = PlaylistService::new(&root); let mut index = LibraryIndex::default();
    service.create("A", &mut index).unwrap(); service.create("B", &mut index).unwrap();
    fs::write(root.join("A/song.mp3"), b"audio").unwrap(); fs::write(root.join("B/song.mp3"), b"old").unwrap();
    let result = service.dump("A", "B", &mut index).unwrap();
    assert_eq!(result.value.copied, 0); assert_eq!(result.value.failed, 1); assert!(result.errors.iter().any(|e| matches!(e, MutationError::PartialCopy(1))));
    assert_eq!(fs::read(root.join("B/song.mp3")).unwrap(), b"old");
    let _ = fs::remove_dir_all(root);
}
