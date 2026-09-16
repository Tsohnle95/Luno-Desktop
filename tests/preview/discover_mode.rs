use luno_desktop_lib::contracts::TrackId;
use luno_desktop_lib::preview::discover_mode::*;
use luno_desktop_lib::preview::save::*;
use std::fs;

fn id(s: &str) -> TrackId { TrackId::new(s).unwrap() }

#[test]
fn discover_adopts_ready_assets_and_restores_captured_queue() {
    let recs = (0..7).map(|i| id(&format!("r{i}"))).collect();
    let mut d = DiscoverMode::new(1, vec![id("seed"), id("next")], 0, recs).unwrap();
    for i in 0..5 { d.ready_asset(1, ReadyAsset { id: id(&format!("r{i}")), path: format!("r{i}.mp3").into() }); }
    assert!(matches!(d.enable(), DiscoverAction::QueueChanged { .. }));
    assert_eq!(d.queued_assets().len(), 5);
    assert!(matches!(d.disable(2), DiscoverAction::Restore { index: 0, .. }));
    assert!(!d.is_active());
}

#[test]
fn discover_projects_pending_recommendations_so_transport_walks_them() {
    // Regression: enable only adopted ALREADY-STAGED assets, so until a
    // download finished the projection equalled the physical queue and
    // Next/Previous walked the library instead of the temporary list.
    let recs = vec![id("r0"), id("r1"), id("r2")];
    let mut d = DiscoverMode::new(1, vec![id("seed"), id("next")], 0, recs).unwrap();
    assert!(matches!(d.enable(), DiscoverAction::QueueChanged { .. }));
    assert_eq!(d.queued_assets().len(), 3, "pending window is projected immediately");
    assert_eq!(
        d.projected_queue(),
        vec![id("seed"), id("r0"), id("r1"), id("r2"), id("next")]
    );
}

#[test]
fn stale_ready_asset_cannot_be_adopted() {
    let mut d = DiscoverMode::new(1, vec![id("seed")], 0, vec![id("r")]).unwrap();
    assert_eq!(d.ready_asset(2, ReadyAsset { id: id("r"), path: "r.mp3".into() }), DiscoverAction::Noop);
    assert!(d.path_for(&id("r")).is_none());
}

#[test]
fn save_requires_playlist_and_does_not_redownload_or_overwrite() {
    let root = std::env::temp_dir().join(format!("luno-u38-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root); fs::create_dir_all(&root).unwrap();
    let staged = root.join("preview.mp3"); fs::write(&staged, b"ID3preview").unwrap();
    assert!(matches!(promote_to_playlist(&staged, "song.mp3", None), Err(SaveError::MissingDestination)));
    let dest = PlaylistDestination { playlist_id: "playlist-a".into(), directory: root.join("playlist") };
    let first = promote_to_playlist(&staged, "song.mp3", Some(&dest)).unwrap();
    let second = promote_to_playlist(&staged, "song.mp3", Some(&dest)).unwrap();
    assert!(!first.already_saved && second.already_saved);
    assert_eq!(fs::read(&staged).unwrap(), b"ID3preview");
    let _ = fs::remove_dir_all(root);
}
