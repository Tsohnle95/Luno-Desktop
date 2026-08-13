use luno_desktop_lib::download::markers::*;
use luno_desktop_lib::download::sync::*;
use luno_desktop_lib::runtime::task_registry::CancellationToken;
use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf { let p = std::env::temp_dir().join(format!("luno-sync-{}", std::process::id())); let _ = fs::remove_dir_all(&p); fs::create_dir_all(&p).unwrap(); p }

#[test]
fn parses_playlist_urls_and_titles() { assert_eq!(playlist_url("https://youtube.com/watch?v=x&list=abc"), Some("https://www.youtube.com/playlist?list=abc".into())); assert_eq!(parse_title("Artist - Song"), ("Artist".into(), "Song".into())); }

#[test]
fn sibling_playlist_is_whole_root_deduped() {
    let root = root(); let sibling = root.join("other"); let destination = root.join("target"); fs::create_dir_all(&sibling).unwrap(); fs::create_dir_all(&destination).unwrap(); fs::write(sibling.join("Artist - Song.mp3"), b"audio").unwrap();
    let entries = vec![PlaylistEntry { id: Some("v1".into()), title: "Artist - Song".into(), url: None }, PlaylistEntry { id: Some("v2".into()), title: "Artist - New".into(), url: None }];
    assert_eq!(plan(&root, &destination, &entries).len(), 1); let _ = fs::remove_dir_all(root);
}

#[test]
fn markers_read_legacy_and_only_follow_valid_output() {
    let root = root(); fs::write(root.join(LEGACY_SUCCESS), "old title old-id\nmalformed\n").unwrap(); let mut markers = Markers::load(&root); assert!(markers.successful.contains("old-id"));
    let item = SyncItem { entry: PlaylistEntry { id: Some("new".into()), title: "A - B".into(), url: None }, destination: root.join("B.mp3") };
    let result = sync_sequential(&root, &[item.clone()], &CancellationToken::new(), |_, _| Ok(())); assert!(matches!(result.outcomes[0].1, ItemOutcome::Failed(_))); assert!(!markers.successful.contains("new"));
    let result = sync_sequential(&root, &[item], &CancellationToken::new(), |item, _| { fs::write(&item.destination, b"audio").unwrap(); Ok(()) }); assert!(matches!(result.outcomes[0].1, ItemOutcome::Downloaded)); assert!(Markers::load(&root).successful.contains("new")); let _ = fs::remove_dir_all(root);
}

#[test]
fn permanent_failures_are_retained_and_cancel_is_partial() {
    let root = root(); let items = (0..2).map(|i| SyncItem { entry: PlaylistEntry { id: Some(format!("v{i}")), title: format!("A - S{i}"), url: None }, destination: root.join(format!("S{i}.mp3")) }).collect::<Vec<_>>(); let token = CancellationToken::new();
    let result = sync_sequential(&root, &items, &token, |item, token| { if item.entry.id.as_deref() == Some("v0") { token.cancel(); } Err("video unavailable".into()) }); assert!(result.cancelled); assert!(Markers::load(&root).failed.contains("v0")); assert_eq!(result.outcomes.len(), 1); let _ = fs::remove_dir_all(root);
}
