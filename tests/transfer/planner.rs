use luno_desktop_lib::domain::identity::{MatchResult, ProviderIdentity, TrackMetadata};
use luno_desktop_lib::transfer::codec::{Manifest, Playlist, Source, Track};
use luno_desktop_lib::transfer::planner::{apply_import_plan, plan_import, MatchPlan};
use luno_desktop_lib::transfer::provider_index::{IndexedTrack, ProviderIndex};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn metadata(title: &str, duration_ms: u64, provider: Option<(&str, &str)>) -> TrackMetadata {
    TrackMetadata { title: title.into(), artist: "Artist".into(), duration_ms,
        provider: provider.map(|(provider, id)| ProviderIdentity { provider: provider.into(), id: id.into() }) }
}

fn indexed(path: &str, item: TrackMetadata) -> IndexedTrack {
    IndexedTrack { path: path.into(), metadata: item }
}

fn manifest(source: Option<Source>, ambiguity: bool) -> Manifest {
    Manifest { format: "luno.library.export".into(), manifest_version: 1, scope: "library".into(),
        created_at: "2026-08-13T00:00:00Z".into(), tracks: vec![Track {
            r#ref: "track:one".into(), title: "Song".into(), artist: "Artist".into(), album: "".into(),
            duration_ms: 10_000, source, ambiguity_confirmation_required: ambiguity,
        }], playlists: vec![Playlist { name: "Mix".into(), description: "".into(), track_refs: vec!["track:one".into()] }],
        unassigned_track_refs: vec![] }
}

#[test]
fn provider_id_is_preferred_even_when_duration_differs() {
    let wanted = metadata("Song", 10_000, Some(("youtube", "abc")));
    let index = ProviderIndex::new([indexed("local.mp3", metadata("Other", 1, Some(("youtube", "abc"))))]);
    assert_eq!(index.matching(&wanted), MatchResult::Match(PathBuf::from("local.mp3")));
}

#[test]
fn metadata_match_accepts_two_second_duration_tolerance() {
    let wanted = metadata("Song", 10_000, None);
    let index = ProviderIndex::new([indexed("local.mp3", metadata("Song", 12_000, None))]);
    assert!(matches!(index.matching(&wanted), MatchResult::Match(_)));
}

#[test]
fn duplicate_candidates_are_ambiguous() {
    let wanted = metadata("Song", 10_000, None);
    let index = ProviderIndex::new([indexed("a.mp3", metadata("Song", 10_000, None)), indexed("b.mp3", metadata("Song", 10_001, None))]);
    assert!(matches!(index.matching(&wanted), MatchResult::Ambiguous(paths) if paths.len() == 2));
}

#[test]
fn ambiguity_requires_a_per_record_approval() {
    let index = ProviderIndex::new([indexed("a.mp3", metadata("Song", 10_000, None)), indexed("b.mp3", metadata("Song", 10_000, None))]);
    let plan = plan_import(&manifest(None, true), &index, "/tmp/luno-transfer-test");
    assert!(matches!(plan.tracks[0].match_plan, MatchPlan::Ambiguous(_)));
    assert!(apply_import_plan(&plan, &HashMap::new()).copied.is_empty());
}

#[test]
fn copy_uses_collision_suffix_and_never_deletes() {
    let root = std::env::temp_dir().join(format!("luno-transfer-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("Mix")).unwrap();
    let source = root.join("source.mp3"); fs::write(&source, b"audio").unwrap();
    let existing = root.join("Mix/Artist - Song.mp3"); fs::write(&existing, b"old").unwrap();
    let index = ProviderIndex::new([indexed(source.to_str().unwrap(), metadata("Song", 10_000, None))]);
    let plan = plan_import(&manifest(None, true), &index, &root);
    let mut approvals = HashMap::new(); approvals.insert("track:one".into(), source.clone());
    let result = apply_import_plan(&plan, &approvals);
    assert_eq!(result.failures.len(), 0);
    assert!(root.join("Mix/Artist - Song (1).mp3").exists());
    assert_eq!(fs::read(&existing).unwrap(), b"old");
    assert!(source.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn copy_failure_is_reported_without_removing_source() {
    let source = PathBuf::from("/definitely/missing/source.mp3");
    let index = ProviderIndex::new([indexed(source.to_str().unwrap(), metadata("Song", 10_000, None))]);
    let root = std::env::temp_dir().join(format!("luno-transfer-failure-{}", std::process::id()));
    let plan = plan_import(&manifest(None, true), &index, &root);
    let mut approvals = HashMap::new(); approvals.insert("track:one".into(), source.clone());
    let result = apply_import_plan(&plan, &approvals);
    assert_eq!(result.copied.len(), 0);
    assert_eq!(result.failures.len(), 1);
    assert!(!source.exists());
    let _ = fs::remove_dir_all(root);
}
