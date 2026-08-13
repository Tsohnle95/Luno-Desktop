use luno_desktop_lib::library::favorites::{normalize_path, Favorites};
use luno_desktop_lib::library::reference_repair::PathReferences;
use std::collections::BTreeMap;

#[test]
fn favorites_are_normalized_projected_and_pruned() {
    let mut favorites = Favorites::from_paths(["/music/../music/a.mp3", "/music/missing.mp3"]);
    assert!(favorites.contains("/music/a.mp3"));
    let projected = favorites.project(["/music/a.mp3"]);
    assert_eq!(projected.into_iter().map(|p| normalize_path(p)).collect::<Vec<_>>(), vec![normalize_path("/music/a.mp3")]);
    assert_eq!(favorites.paths().count(), 1);
}

#[test]
fn rename_repairs_all_path_references_including_artwork() {
    let old = "/music/old.mp3";
    let new = "/music/new.mp3";
    let mut refs = PathReferences { queue_playlist: vec![old.into()], queue: vec![old.into()], original_queue: vec![old.into()], recent: vec![old.into()], favorites: Favorites::from_paths([old]), artwork: BTreeMap::from([(old.into(), "cover".into())]) };
    let report = refs.repair(old, Some(new));
    assert_eq!(report.changed, 6);
    assert_eq!(refs.queue_playlist, vec![normalize_path(new)]);
    assert_eq!(refs.queue, vec![normalize_path(new)]);
    assert!(refs.favorites.contains(new));
    assert!(refs.artwork.contains_key(&normalize_path(new)));
}

#[test]
fn delete_and_stale_pruning_remove_everything_without_persisting_history() {
    let old = "/music/old.mp3";
    let mut refs = PathReferences { queue_playlist: vec![old.into()], queue: vec![old.into()], original_queue: vec![old.into()], recent: vec![old.into()], favorites: Favorites::from_paths([old]), artwork: BTreeMap::from([(old.into(), "cover".into())]) };
    let report = refs.repair(old, Option::<&str>::None);
    assert_eq!(report.removed, 4);
    assert!(refs.queue_playlist.is_empty() && refs.queue.is_empty() && refs.original_queue.is_empty() && refs.recent.is_empty());
    assert_eq!(refs.favorites.paths().count(), 0);
    assert!(refs.artwork.is_empty());
}

#[test]
fn failed_transaction_restores_all_references() {
    let old = "/music/old.mp3";
    let mut refs = PathReferences { queue_playlist: vec![old.into()], queue: vec![], original_queue: vec![], recent: vec![], favorites: Favorites::default(), artwork: BTreeMap::new() };
    let result: Result<(), &str> = refs.transaction(|state| { state.repair(old, Some("/music/new.mp3")); Err("write failed") });
    assert_eq!(result, Err("write failed"));
    assert_eq!(refs.queue_playlist, vec![normalize_path(old)]);
}
