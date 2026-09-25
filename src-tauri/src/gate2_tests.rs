use crate::contracts::events::Command;
use crate::contracts::events::CommandEnvelope;
use crate::contracts::events::{
    Event, RootStatus as ContractRootStatus, ShutdownReason, ShutdownState,
};
use crate::contracts::types::{
    DiscoverSnapshot, Generation, PlaybackSnapshot, PlaylistId, SaveState, SaveStateKind,
    TrackId,
};
use crate::download::markers::Markers;
use crate::download::promotion::stage_output;
use crate::download::sync::{plan, sync_sequential, ItemOutcome, PlaylistEntry, SyncItem};
use crate::library::duplicates::{find_duplicates, load_allowed, save_allowed};
use crate::library::favorites::Favorites;
use crate::library::index::{LibraryIndex, RefreshRequest};
use crate::library::mutations::{MutationError, PlaylistService};
use crate::library::reference_repair::PathReferences;
use crate::library::scanner::{scan_library, Entry, FileSystem, RootStatus};
use crate::persistence::atomic_write::write_atomic;
use crate::persistence::legacy::read_config;
use crate::persistence::recovery::{recover, RecoveryResult};
use crate::runtime::task_registry::SpawnError;
use crate::state::AppState;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn fixture_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("luno-gate2-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

struct FixtureFs {
    entries: HashMap<PathBuf, Result<Vec<Entry>, String>>,
    modified: HashMap<PathBuf, SystemTime>,
}
impl FileSystem for FixtureFs {
    fn read_dir(&self, path: &Path) -> Result<Vec<Entry>, String> {
        self.entries
            .get(path)
            .cloned()
            .unwrap_or_else(|| Err("not found".into()))
    }
    fn modified(&self, path: &Path) -> Result<SystemTime, String> {
        self.modified
            .get(path)
            .copied()
            .ok_or_else(|| "not found".into())
    }
}

#[test]
fn scan_roots_order_and_status_match_corpus() {
    let root = PathBuf::from("fixture://scan-order");
    let newer = root.join("track-b.flac");
    let older = root.join("artist-a - track-a.mp3");
    let playlist = root.join("playlist-a");
    let fs = FixtureFs {
        entries: HashMap::from([
            (
                root.clone(),
                Ok(vec![
                    Entry {
                        path: playlist.clone(),
                        name: "playlist-a".into(),
                        is_dir: true,
                    },
                    Entry {
                        path: newer.clone(),
                        name: "track-b.flac".into(),
                        is_dir: false,
                    },
                    Entry {
                        path: older.clone(),
                        name: "artist-a - track-a.mp3".into(),
                        is_dir: false,
                    },
                    Entry {
                        path: root.join("notes.txt"),
                        name: "notes.txt".into(),
                        is_dir: false,
                    },
                ]),
            ),
            (
                playlist.clone(),
                Ok(vec![Entry {
                    path: playlist.join("track-c.mp3"),
                    name: "track-c.mp3".into(),
                    is_dir: false,
                }]),
            ),
            (PathBuf::from("fixture://empty-root"), Ok(vec![])),
        ]),
        modified: HashMap::from([
            (newer.clone(), UNIX_EPOCH + Duration::from_secs(2)),
            (older.clone(), UNIX_EPOCH + Duration::from_secs(1)),
        ]),
    };
    let outcome = scan_library(
        &[
            root.clone(),
            PathBuf::from("fixture://empty-root"),
            PathBuf::from("fixture://missing-root"),
        ],
        &fs,
    );
    let empty = scan_library(&[PathBuf::from("fixture://empty-root")], &fs);
    assert!(empty.playlists.is_empty());
    assert_eq!(outcome.playlists["Unsorted"], vec![newer, older]);
    assert_eq!(outcome.playlists["playlist-a"].len(), 1);
    assert_eq!(
        outcome.roots,
        vec![
            (root, RootStatus::Scanned),
            (PathBuf::from("fixture://empty-root"), RootStatus::Scanned),
            (PathBuf::from("fixture://missing-root"), RootStatus::Missing)
        ]
    );
    assert!(!outcome.playlists["Unsorted"]
        .iter()
        .any(|path| path.extension() == Some("txt".as_ref())));
}

#[test]
fn real_missing_root_is_classified_as_missing() {
    let root = fixture_dir("missing-root").join("does-not-exist");
    let outcome = scan_library(&[root.clone()], &crate::library::scanner::OsFileSystem);
    assert_eq!(outcome.roots, vec![(root, RootStatus::Missing)]);
    assert!(outcome.playlists.is_empty());
}

#[cfg(unix)]
#[test]
fn target_scan_publishes_fixture_root_statuses_and_snapshot_on_event_boundary() {
    struct PermissionGuard(PathBuf);
    impl Drop for PermissionGuard {
        fn drop(&mut self) {
            if let Ok(metadata) = fs::metadata(&self.0) {
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o700);
                let _ = fs::set_permissions(&self.0, permissions);
            }
        }
    }

    let dir = fixture_dir("target-scan-boundary");
    let empty = dir.join("empty");
    let denied = dir.join("denied");
    fs::create_dir_all(&empty).unwrap();
    fs::create_dir_all(&denied).unwrap();
    let _permission_guard = PermissionGuard(denied.clone());
    let mut permissions = fs::metadata(&denied).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    permissions.set_mode(0o000);
    fs::set_permissions(&denied, permissions).unwrap();
    let missing = dir.join("missing");
    let events = Arc::new(Mutex::new(Vec::new()));
    let received = events.clone();
    let state = AppState::new(2);
    state.set_scan_roots(vec![empty.clone(), missing.clone(), denied.clone()]);
    state.set_event_sink(Arc::new(move |event| {
        received.lock().unwrap_or_else(|e| e.into_inner()).push(event);
        Ok(())
    }));
    let generation = state.scan_async().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !events
        .lock()
        .unwrap()
        .iter()
        .any(|event| matches!(event.event, Event::LibrarySnapshot { .. }))
    {
        assert!(
            std::time::Instant::now() < deadline,
            "scan result was not delivered"
        );
        std::thread::yield_now();
    }
    let events = events.lock().unwrap_or_else(|e| e.into_inner());
    let statuses: Vec<_> = events
        .iter()
        .filter_map(|event| match event.event {
            Event::LibraryRootStatus { root_index, status } => Some((root_index, status)),
            _ => None,
        })
        .collect();
    assert_eq!(
        statuses[0..2],
        [
            (0, ContractRootStatus::Scanned),
            (1, ContractRootStatus::Missing),
        ]
    );
    if statuses.get(2) != Some(&(2, ContractRootStatus::Inaccessible)) {
        drop(events);
        drop(_permission_guard);
        fs::remove_dir_all(dir).unwrap();
        return;
    }
    let snapshot = events.iter().find_map(|event| match &event.event {
        Event::LibrarySnapshot { snapshot } => Some(snapshot),
        _ => None,
    });
    assert_eq!(
        snapshot.map(|snapshot| snapshot.generation),
        Some(generation)
    );
    assert!(snapshot.is_some_and(|snapshot| snapshot.playlists.is_empty()));
    assert_eq!(
        state.root_statuses(),
        vec![
            ContractRootStatus::Scanned,
            ContractRootStatus::Missing,
            ContractRootStatus::Inaccessible,
        ]
    );
    drop(events);
    drop(_permission_guard);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn scan_generation_coalesces_and_rejects_stale_results() {
    let mut index = LibraryIndex::default();
    assert_eq!(
        index.request_refresh(),
        RefreshRequest::Started { generation: 1 }
    );
    assert_eq!(
        index.request_refresh(),
        RefreshRequest::Coalesced { generation: 1 }
    );
    let stale = crate::library::scanner::ScanOutcome {
        playlists: Default::default(),
        roots: vec![],
        errors: vec![],
    };
    assert!(!index.finish_scan(2, stale.clone()));
    assert!(index.finish_scan(1, stale));
    assert!(index.take_pending());
}

#[test]
fn favorites_and_rename_repair_prune_stale_paths() {
    let root = PathBuf::from("/synthetic/library");
    let old = root.join("artist-a - track-a.mp3");
    let new = root.join("artist-b - track-b.mp3");
    let mut favorites = Favorites::from_paths([old.clone(), root.join("missing.mp3")]);
    assert_eq!(favorites.project([old.clone()]), vec![old.clone()]);
    let mut refs = PathReferences {
        queue_playlist: vec![old.to_string_lossy().into()],
        queue: vec![old.to_string_lossy().into()],
        original_queue: vec![],
        recent: vec![old.to_string_lossy().into()],
        favorites,
        artwork: Default::default(),
    };
    let report = refs.repair(&old, Some(&new));
    assert!(report.changed >= 4);
    assert!(refs.favorites.contains(&new));
    refs.prune_stale([new]);
    assert_eq!(refs.queue.len(), 1);
}

#[test]
fn markers_accept_all_legacy_forms_but_not_corrupt_ids_and_retry_failures() {
    let dir = fixture_dir("markers");
    fs::write(dir.join(".downloaded_vids"), "success-id\n/suspicious\n").unwrap();
    fs::write(dir.join(".downloaded.txt"), "legacy title legacy-id\n").unwrap();
    fs::write(
        dir.join(".sync_failed_vids"),
        "failed-id\tvideo unavailable\n/bad\treason\n",
    )
    .unwrap();
    let mut markers = Markers::load(&dir);
    assert!(markers.successful.contains("success-id"));
    assert!(markers.successful.contains("legacy-id"));
    assert!(!markers.successful.contains("/suspicious"));
    assert!(markers.failed.contains("failed-id"));
    assert!(!markers.failed.contains("/bad"));
    markers.record_success(&dir, "retry-id").unwrap();
    markers
        .record_failure(&dir, "permanent-id", "blocked\nsource")
        .unwrap();
    let reread = Markers::load(&dir);
    assert!(reread.successful.contains("retry-id"));
    assert!(reread.failed.contains("permanent-id"));
    let invalid_dir = dir.join("invalid-writes");
    fs::create_dir(&invalid_dir).unwrap();
    let successful_before = markers.successful.clone();
    let failed_before = markers.failed.clone();
    markers.record_success(&invalid_dir, "/invalid").unwrap();
    markers
        .record_failure(&invalid_dir, "invalid id", "blocked")
        .unwrap();
    assert_eq!(markers.successful, successful_before);
    assert_eq!(markers.failed, failed_before);
    assert!(!invalid_dir.join(".downloaded_vids").exists());
    assert!(!invalid_dir.join(".sync_failed_vids").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_sync_journey_covers_whole_root_dedupe_markers_validation_retry_and_cancel() {
    let dir = fixture_dir("sync-journey");
    let root = dir.join("library");
    let destination = root.join("playlist-a");
    fs::create_dir_all(root.join("other-playlist")).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(
        root.join("other-playlist").join("Artist - Existing.mp3"),
        b"ID3synthetic",
    )
    .unwrap();
    fs::write(
        destination.join(".downloaded_vids"),
        "current-id\n/bad-id\n",
    )
    .unwrap();
    fs::write(
        destination.join(".downloaded.txt"),
        "legacy title legacy-id\n",
    )
    .unwrap();
    fs::write(
        destination.join(".sync_failed_vids"),
        "previous-permanent\tvideo unavailable\n/bad-failure\trejected\n",
    )
    .unwrap();

    let entries = vec![
        PlaylistEntry {
            id: Some("existing-id".into()),
            title: "Artist - Existing".into(),
            channel: None,
            duration_secs: None,
            url: None,
        },
        PlaylistEntry {
            id: Some("duplicate-id".into()),
            title: "Artist - Existing".into(),
            channel: None,
            duration_secs: None,
            url: None,
        },
        PlaylistEntry {
            id: Some("new-id".into()),
            title: "Artist - New".into(),
            channel: None,
            duration_secs: None,
            url: Some("local:synthetic".into()),
        },
    ];
    let planned = plan(&root, &destination, &entries);
    assert_eq!(planned.len(), 1, "REF-URL-MARKERS whole-root dedupe");
    assert_eq!(planned[0].entry.id.as_deref(), Some("new-id"));

    let item = |id: &str, title: &str| SyncItem {
        entry: PlaylistEntry {
            id: Some(id.into()),
            title: title.into(),
            channel: None,
            duration_secs: None,
            url: Some("local:synthetic".into()),
        },
        source_override: None,
        destination: destination.join(format!("{id}.mp3")),
    };
    let items = vec![
        item("current-id", "Current"),
        item("legacy-id", "Legacy"),
        item("valid-id", "Valid"),
        item("invalid-id", "Invalid"),
        item("missing-id", "Missing"),
        item("permanent-id", "Permanent"),
        item("transient-id", "Transient"),
        item("cancel-id", "Cancel"),
    ];
    let attempts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let attempts_for_download = attempts.clone();
    let token = crate::runtime::task_registry::CancellationToken::new();
    let result = sync_sequential(&destination, &items, &token, move |item, token| {
        let id = item.entry.id.as_deref().unwrap();
        let mut attempts = attempts_for_download.lock().unwrap_or_else(|e| e.into_inner());
        *attempts.entry(id.into()).or_default() += 1;
        match id {
            "valid-id" | "transient-id" => {
                fs::write(&item.destination, b"ID3validated").unwrap();
                if id == "transient-id" && attempts[id] == 1 {
                    fs::remove_file(&item.destination).unwrap();
                    Err("temporary local failure".into())
                } else {
                    Ok(())
                }
            }
            "invalid-id" => {
                fs::write(&item.destination, b"not audio").unwrap();
                Ok(())
            }
            "missing-id" => Ok(()),
            "permanent-id" => Err("video unavailable from local fixture".into()),
            "cancel-id" => {
                token.cancel();
                Ok(())
            }
            _ => Ok(()),
        }
    });
    assert_eq!(
        result.outcomes[0],
        ("current-id".into(), ItemOutcome::Matched)
    );
    assert_eq!(
        result.outcomes[1],
        ("legacy-id".into(), ItemOutcome::Matched)
    );
    assert_eq!(
        result.outcomes[2],
        ("valid-id".into(), ItemOutcome::Downloaded)
    );
    assert!(matches!(result.outcomes[3].1, ItemOutcome::Failed(_)));
    assert!(matches!(result.outcomes[4].1, ItemOutcome::Failed(_)));
    assert!(matches!(
        result.outcomes[5].1,
        ItemOutcome::Unavailable(_)
    ));
    assert!(matches!(result.outcomes[6].1, ItemOutcome::Failed(_)));
    assert!(result.cancelled);
    let success_markers = fs::read_to_string(destination.join(".downloaded_vids")).unwrap();
    let failure_markers = fs::read_to_string(destination.join(".sync_failed_vids")).unwrap();
    assert!(!success_markers.contains("invalid-id"));
    assert!(!success_markers.contains("missing-id"));
    assert!(success_markers.contains("valid-id"));
    assert!(failure_markers.contains("permanent-id"));
    assert!(!success_markers.contains("cancel-id"));
    let permanent_retry = sync_sequential(
        &destination,
        &[item("permanent-id", "Permanent")],
        &crate::runtime::task_registry::CancellationToken::new(),
        |item, _token| {
            fs::write(&item.destination, b"ID3retried-audio").unwrap();
            Ok(())
        },
    );
    assert_eq!(permanent_retry.outcomes, vec![("permanent-id".into(), ItemOutcome::Downloaded)]);

    let retry_token = crate::runtime::task_registry::CancellationToken::new();
    let retry = sync_sequential(
        &destination,
        &[item("transient-id", "Transient")],
        &retry_token,
        |item, _| {
            fs::write(&item.destination, b"ID3retry").unwrap();
            Ok(())
        },
    );
    assert_eq!(
        retry.outcomes,
        vec![("transient-id".into(), ItemOutcome::Downloaded)]
    );
    assert_eq!(*attempts.lock().unwrap_or_else(|e| e.into_inner()).get("transient-id").unwrap(), 1);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn target_scan_spawn_failure_is_bounded_and_shutdown_cancels_owned_work() {
    let state = AppState::new(1);
    let started = Arc::new(Barrier::new(2));
    let started_worker = started.clone();
    let _handle = state
        .submit(
            state.generation("gate2-blocking", crate::contracts::types::Generation(1)),
            move |token| {
                started_worker.wait();
                while !token.is_cancelled() {
                    std::thread::yield_now();
                }
            },
        )
        .unwrap();
    started.wait();
    assert_eq!(state.scan_async(), Err(SpawnError::AtCapacity));
    let report = state.shutdown(std::time::Duration::from_secs(1));
    assert!(!report.timed_out);
    assert!(!report.failed);
}

#[test]
fn target_shutdown_event_terminal_order_is_deterministic_after_cancellation() {
    let state = AppState::new(1);
    let started = Arc::new(Barrier::new(2));
    let started_worker = started.clone();
    let _handle = state
        .submit(
            state.generation("gate2-cancellable", crate::contracts::types::Generation(1)),
            move |token| {
                started_worker.wait();
                while !token.is_cancelled() {
                    std::thread::yield_now();
                }
            },
        )
        .unwrap();
    started.wait();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let sink_observed = observed.clone();
    state.set_event_sink(Arc::new(move |envelope| {
        if let Event::Shutdown { envelope } = envelope.event {
            sink_observed.lock().unwrap_or_else(|e| e.into_inner()).push(envelope.state);
        }
        Ok(())
    }));
    let report = state.shutdown_events(ShutdownReason::UserRequested, Duration::from_secs(1));
    assert!(!report.timed_out);
    assert_eq!(
        *observed.lock().unwrap_or_else(|e| e.into_inner()),
        vec![
            ShutdownState::Requested,
            ShutdownState::Draining,
            ShutdownState::Complete
        ]
    );
}

#[test]
fn atomic_write_recovers_previous_value_even_with_corrupt_metadata() {
    let dir = fixture_dir("atomic");
    let target = dir.join("settings.json");
    write_atomic(&target, br#"{"value":1}"#).unwrap();
    write_atomic(&target, br#"{"value":2}"#).unwrap();
    fs::remove_file(&target).unwrap();
    fs::write(
        format!("{}.luno-recovery.json", target.display()),
        b"not-json",
    )
    .unwrap();
    assert_eq!(recover(&target).unwrap(), RecoveryResult::RestoredBackup);
    assert_eq!(fs::read(&target).unwrap(), br#"{"value":1}"#);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn corrupt_config_is_read_only_and_missing_fields_are_safe() {
    let dir = fixture_dir("config");
    let config = dir.join(".luno_config.json");
    fs::write(&config, br#"{"music_root":"fixture://missing-root","volume":2,"favorite_songs":["fixture://track-a"],"unknown":true}"#).unwrap();
    let original = fs::read(&config).unwrap();
    let parsed = read_config(&config).unwrap();
    assert_eq!(parsed.music_root.as_deref(), Some("fixture://missing-root"));
    assert_eq!(parsed.volume, Some(2.0));
    assert_eq!(fs::read(&config).unwrap(), original);
    fs::write(&config, b"not-json").unwrap();
    assert!(read_config(&config).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn playlist_crud_and_allowed_duplicate_persistence_keep_safeguards() {
    let dir = fixture_dir("playlists");
    let service = PlaylistService::new(&dir);
    let mut index = LibraryIndex::default();
    let created = service.create("playlist<>a", &mut index).unwrap();
    assert_eq!(
        created.value.file_name().unwrap().to_string_lossy(),
        "playlista"
    );
    assert!(matches!(
        service.create("playlist<>a", &mut index),
        Err(MutationError::Collision(_))
    ));
    assert!(matches!(
        service.delete("All Music", &mut index),
        Err(MutationError::Protected(_))
    ));
    assert!(matches!(
        service.set_url("playlista", Some("file://not-allowed"), &mut index),
        Err(MutationError::InvalidUrl)
    ));
    service
        .set_url(
            "playlista",
            Some("https://example.test/playlist?list=fixture"),
            &mut index,
        )
        .unwrap();
    let source = dir.join("source-track.mp3");
    fs::write(&source, b"audio fixture").unwrap();
    let copied = service.add_files(&[source.clone()], "playlista", &mut index).unwrap();
    assert_eq!(copied.value.copied, 1);
    assert_eq!(fs::read(dir.join("playlista/source-track.mp3")).unwrap(), b"audio fixture");
    let duplicate = service.add_files(&[source], "playlista", &mut index).unwrap();
    assert_eq!(duplicate.value.copied, 0, "adding an existing song must not overwrite it");
    let keys = std::collections::BTreeSet::from(["artistatracka".to_owned()]);
    let library = std::collections::BTreeMap::from([(
        "playlist-a".to_owned(),
        vec![
            PathBuf::from("artist-a - track-a.mp3"),
            PathBuf::from("ARTIST-A - TRACK-A.mp3"),
        ],
    )]);
    assert_eq!(
        find_duplicates(&library, &std::collections::BTreeSet::new()).len(),
        1
    );
    assert!(find_duplicates(&library, &keys).is_empty());
    save_allowed(&dir, &keys).unwrap();
    assert_eq!(load_allowed(&dir).unwrap(), keys);
    fs::write(dir.join(".luno_allowed_dups.json"), b"not-json").unwrap();
    assert!(load_allowed(&dir).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn playlist_url_command_persists_canonical_youtube_url_into_snapshots() {
    let root = fixture_dir("playlist-url-command");
    let playlist = root.join("Road trip");
    fs::create_dir_all(&playlist).unwrap();
    let track = playlist.join("Artist - Track.mp3");
    fs::write(&track, b"ID3fixture").unwrap();
    let state = AppState::new(4);
    state.set_scan_roots(vec![root.clone()]);
    state.replace_library(
        crate::contracts::types::Generation(1),
        BTreeMap::from([("Road trip".into(), vec![track.clone()])]),
    );
    let result = state.dispatch(command("set-playlist-url", Command::SetPlaylistUrl {
        playlist_id: crate::contracts::types::PlaylistId("playlist:Road trip".into()),
        url: Some("https://music.youtube.com/playlist?list=PL_fixture-1&feature=share".into()),
    })).unwrap();
    assert_eq!(result["url"], "https://www.youtube.com/playlist?list=PL_fixture-1");
    let snapshot = state.replace_library(
        crate::contracts::types::Generation(2),
        BTreeMap::from([("Road trip".into(), vec![track])]),
    );
    assert_eq!(snapshot.playlists[0].url.as_deref(), Some("https://www.youtube.com/playlist?list=PL_fixture-1"));
    assert!(state.dispatch(command("reject-playlist-url", Command::SetPlaylistUrl {
        playlist_id: crate::contracts::types::PlaylistId("playlist:Road trip".into()),
        url: Some("https://youtube.com.evil.test/playlist?list=PL_fixture-1".into()),
    })).is_err());
    fs::remove_dir_all(root).unwrap();
}

fn command(request_id: &str, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        version: 1,
        request_id: request_id.into(),
        command,
    }
}

#[test]
fn discover_without_key_returns_unavailable_error() {
    let state = AppState::new(2);
    let result = state.dispatch(command(
        "discover-no-key",
        Command::Discover { artist: "Artist".into(), title: "Track".into(), limit: 5 },
    ));
    let error = result.expect_err("discovery without a key must fail truthfully");
    assert_eq!(error.code, crate::contracts::types::ErrorCode::Unavailable);
    assert!(error.message.contains("Last.fm"), "message: {}", error.message);
    assert!(!error.retryable);
}

#[test]
fn discover_envelope_rejects_empty_seed_fields() {
    let invalid = serde_json::from_value::<CommandEnvelope>(serde_json::json!({
        "version": 1,
        "request_id": "discover-invalid",
        "command": { "type": "discover", "payload": { "artist": "   ", "title": "Track", "limit": 5 } }
    }));
    assert!(invalid.is_err(), "empty artist must be rejected by the envelope");
    let parsed = serde_json::from_value::<CommandEnvelope>(serde_json::json!({
        "version": 1,
        "request_id": "discover-limit",
        "command": { "type": "discover", "payload": { "artist": "Artist", "title": "Track", "limit": 21 } }
    })).expect("limit is not contract-restricted at parse time");
    match parsed.command {
        Command::Discover { limit, .. } => assert_eq!(limit, 21, "clamping happens in dispatch"),
        _ => panic!("unexpected command"),
    }
}

struct FixtureToolchain { root: PathBuf }
impl Drop for FixtureToolchain {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); }
}

fn fixture_preview_toolchain() -> (FixtureToolchain, PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "luno-gate2-preview-toolchain-lifecycle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let helper = root.join("helper");
    // The ID3 prefix keeps the fake payload past media-signature validation.
    std::fs::write(
        &helper,
        "#!/bin/sh\ncount=\"$0.calls\"\nn=0\nif [ -f \"$count\" ]; then n=$(cat \"$count\"); fi\necho $((n + 1)) > \"$count\"\nprintf 'ID3synthetic preview audio source bytes' > \"$1\"\n",
    )
    .unwrap();
    let ffmpeg = root.join("ffmpeg");
    // `-y -i INPUT -ar ... -f adts -` puts the input at positional argument 3
    // and streams the output to stdout (`-` is the final argument); saves
    // remux with `-f mp4 OUTPUT` where the final argument is the file.
    std::fs::write(&ffmpeg, "#!/bin/sh\nfor last do :; done\nif [ \"$last\" = \"-\" ]; then cat \"$3\"; else cat \"$3\" > \"$last\"; fi\n").unwrap();
    for script in [&helper, &ffmpeg] {
        let mut permissions = std::fs::metadata(script).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(script, permissions).unwrap();
    }
    (FixtureToolchain { root }, helper, ffmpeg)
}

fn preview_seed(artist: &str, title: &str) -> crate::contracts::types::PreviewSeed {
    crate::contracts::types::PreviewSeed { artist: artist.into(), title: title.into() }
}

fn isolated_preview_state(label: &str, task_limit: usize) -> AppState {
    let root = std::env::temp_dir().join(format!("luno-gate2-preview-store-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    AppState::with_preview_store(
        crate::preview::assets::PreviewAssetStore::at(&root).expect("isolated preview store"),
        task_limit,
    )
}

fn preview_snapshots(state: &AppState) -> Vec<crate::contracts::types::PreviewSnapshot> {
    state.events().iter().filter_map(|envelope| match &envelope.event {
        Event::PreviewSnapshot { snapshot } => Some(snapshot.clone()),
        _ => None,
    }).collect()
}

#[test]
fn preview_prefetch_does_not_leave_an_active_session_when_driver_cannot_be_admitted() {
    let state = isolated_preview_state("driver-capacity", 1);
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    state.submit(state.generation("capacity-holder", Generation(1)), move |token| {
        let _ = started_tx.send(());
        while !token.is_cancelled() && release_rx.try_recv().is_err() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }).expect("occupy the only task slot");
    started_rx.recv_timeout(Duration::from_secs(1)).expect("capacity holder started");
    let seed = TrackId("library-seed".into());
    state.set_playing(seed.clone(), vec![seed]);

    let error = state.dispatch(command("prefetch-driver-capacity", Command::PreviewPrefetch {
        seeds: vec![preview_seed("Artist", "Track")],
    })).expect_err("a full task registry must be retryable, not look like an active preview");
    assert_eq!(error.code, crate::contracts::types::ErrorCode::Busy);
    assert!(error.retryable);
    let snapshot = preview_snapshots(&state).into_iter().last().expect("terminal snapshot is published");
    assert!(!snapshot.active, "no active preview may exist without a completion driver: {snapshot:?}");

    let _ = release_tx.send(());
    let report = state.shutdown(Duration::from_secs(1));
    assert!(!report.timed_out, "capacity holder must drain: {report:?}");
    state.cleanup_previews();
}

#[test]
fn preview_prefetch_retries_item_admission_after_task_capacity_frees() {
    let (_toolchain, helper, ffmpeg) = fixture_preview_toolchain();
    let state = isolated_preview_state("item-capacity-retry", 2);
    state.set_preview_toolchain(Some(helper), Some(ffmpeg));
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    state.submit(state.generation("temporary-capacity-holder", Generation(1)), move |token| {
        let _ = started_tx.send(());
        while !token.is_cancelled() && release_rx.try_recv().is_err() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }).expect("occupy the remaining task slot");
    started_rx.recv_timeout(Duration::from_secs(1)).expect("capacity holder started");
    let seed = TrackId("library-seed".into());
    state.set_playing(seed.clone(), vec![seed]);
    let seeds: Vec<_> = (0..5).map(|n| preview_seed(&format!("Artist{n}"), &format!("Track{n}"))).collect();

    let value = state.dispatch(command("prefetch-item-capacity", Command::PreviewPrefetch { seeds }))
        .expect("the session driver has one available slot");
    let initial: crate::contracts::types::PreviewSnapshot = serde_json::from_value(value).unwrap();
    assert!(initial.active);
    assert!(initial.prepared.is_empty());
    assert!(initial.unavailable.is_empty(), "capacity pressure must not become a media failure");

    let _ = release_tx.send(());
    wait_until(10, "five recommendation previews after capacity release", || {
        preview_snapshots(&state).iter().any(|snapshot| snapshot.active && snapshot.prepared.len() == 5)
    }, || format!("{:#?}", preview_snapshots(&state)));
    let report = state.shutdown(Duration::from_secs(1));
    assert!(!report.timed_out, "preview workers must drain: {report:?}");
    state.cleanup_previews();
}

#[test]
fn preview_tap_requires_an_active_playback_queue() {
    let state = AppState::new(2);
    let result = state.dispatch(command(
        "preview-empty-queue",
        Command::PreviewTap {
            seeds: vec![preview_seed("Artist", "Track")],
            key: "Artist-Track".into(),
        },
    ));
    let error = result.expect_err("tapping without a playback queue must fail truthfully");
    assert_eq!(error.code, crate::contracts::types::ErrorCode::Unavailable);
    assert!(error.message.contains("playback queue"), "message: {}", error.message);
}

#[test]
fn preview_tap_rejects_keys_outside_the_seeded_recommendations() {
    let state = AppState::new(2);
    let error = state
        .dispatch(command(
            "preview-foreign-key",
            Command::PreviewTap {
                seeds: vec![preview_seed("Artist", "Track")],
                key: "Other-Other".into(),
            },
        ))
        .expect_err("foreign keys must be rejected before any session work");
    assert_eq!(error.code, crate::contracts::types::ErrorCode::InvalidRequest);
}

#[test]
fn preview_session_lifecycle_prepares_window_and_reports_snapshots() {
    let (_toolchain, helper, ffmpeg) = fixture_preview_toolchain();
    let state = isolated_preview_state("lifecycle", 16);
    state.set_preview_toolchain(Some(helper), Some(ffmpeg));
    state
        .dispatch(command("preview-queue", Command::AddToQueue {
            track_ids: vec![TrackId("fixture://queue-a".into()), TrackId("fixture://queue-b".into()), TrackId("fixture://queue-c".into())],
        }))
        .expect("queue seeding must succeed");
    let seeds = vec![
        preview_seed("Artist0", "Track0"),
        preview_seed("Artist1", "Track1"),
        preview_seed("Artist2", "Track2"),
        preview_seed("Artist3", "Track3"),
        preview_seed("Artist4", "Track4"),
        preview_seed("Artist5", "Track5"),
    ];
    let value = state
        .dispatch(command("preview-tap", Command::PreviewTap {
            seeds: seeds.clone(),
            key: "Artist0-Track0".into(),
        }))
        .expect("tap must start a session");
    let snapshot: crate::contracts::types::PreviewSnapshot = serde_json::from_value(value).unwrap();
    assert!(snapshot.active, "session must start active, got: {snapshot:?}\nevts: {:#?}", preview_snapshots(&state));
    assert_eq!(snapshot.window, vec!["Artist0-Track0", "Artist1-Track1", "Artist2-Track2", "Artist3-Track3", "Artist4-Track4", "Artist5-Track5"]);
    assert_eq!(snapshot.current, None, "the tapped item becomes current once prepared");

    // The fixture toolchain publishes the tapped asset; wait for the driver
    // to apply it (this also exercises the real worker pipeline end to end).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let ready = state.events().iter().any(|envelope| match &envelope.event {
            Event::PreviewSnapshot { snapshot } => snapshot.prepared.iter().any(|key| key == "Artist0-Track0") && snapshot.active && snapshot.current.as_deref() == Some("Artist0-Track0"),
            _ => false,
        });
        if ready { break; }
        assert!(std::time::Instant::now() < deadline, "prepared asset never landed: {:#?}", preview_snapshots(&state));
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(state.events().iter().any(|envelope| match &envelope.event {
        Event::PreviewSnapshot { snapshot } => snapshot.prepared.contains(&"Artist0-Track0".to_string()) && snapshot.active,
        _ => false,
    }), "prepared state must publish through snapshots: {:#?}", preview_snapshots(&state));

    let value = state.dispatch(command("preview-next", Command::PreviewNext))
        .expect("next must advance the session");
    let snapshot: crate::contracts::types::PreviewSnapshot = serde_json::from_value(value).unwrap();
    assert_eq!(snapshot.current, Some("Artist1-Track1".into()));
    assert!(snapshot.active);

    let value = state.dispatch(command("preview-previous", Command::PreviewPrevious))
        .expect("previous must step back");
    let snapshot: crate::contracts::types::PreviewSnapshot = serde_json::from_value(value).unwrap();
    assert_eq!(snapshot.current, Some("Artist0-Track0".into()));

    let value = state.dispatch(command("preview-cancel", Command::PreviewCancel))
        .expect("cancel must close the session");
    let snapshot: crate::contracts::types::PreviewSnapshot = serde_json::from_value(value).unwrap();
    assert!(snapshot.cancelled, "cancel must be explicit in the snapshot");
    assert!(!snapshot.active, "session must end on cancel");

    let error = state.dispatch(command("preview-next-after-cancel", Command::PreviewNext)).expect_err("stepping after cancel must be NotFound");
    assert_eq!(error.code, crate::contracts::types::ErrorCode::NotFound);

    state.cleanup_previews();
}

#[test]
fn preview_tap_without_toolchain_ends_the_session_truthfully() {
    let state = isolated_preview_state("notool", 16);
    state
        .dispatch(command("preview-queue-2", Command::AddToQueue {
            track_ids: vec![TrackId("fixture://queue-x".into())],
        }))
        .expect("queue seeding must succeed");
    let value = state
        .dispatch(command("preview-tap-no-toolchain", Command::PreviewTap {
            seeds: vec![preview_seed("Artist", "Track")],
            key: "Artist-Track".into(),
        }))
        .expect("tap must start a session even without a toolchain");
    let snapshot: crate::contracts::types::PreviewSnapshot = serde_json::from_value(value).unwrap();
    assert!(snapshot.active);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let ended = preview_snapshots(&state).iter().any(|snapshot| !snapshot.active);
        if ended { break; }
        assert!(std::time::Instant::now() < deadline, "missing toolchain must end the session: {:#?}", preview_snapshots(&state));
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let final_snapshot = preview_snapshots(&state).into_iter().last().expect("snapshots exist");
    assert!(!final_snapshot.active);
    assert!(final_snapshot.unavailable.iter().any(|key| key == "Artist-Track"), "tapped key must be reported unavailable: {final_snapshot:?}");
    state.cleanup_previews();
}

#[test]
fn preview_prefetch_marks_every_first_window_item_unavailable_when_media_cannot_start() {
    let state = isolated_preview_state("unavailable-window", 16);
    let seed = TrackId("library-seed".into());
    state.set_playing(seed.clone(), vec![seed]);
    let seeds: Vec<_> = (0..5).map(|n| preview_seed(&format!("Artist{n}"), &format!("Track{n}"))).collect();
    state.dispatch(command("prefetch-unavailable-window", Command::PreviewPrefetch { seeds }))
        .expect("prefetch reports session before media preparation");

    wait_until(5, "unavailable recommendation window", || {
        preview_snapshots(&state).iter().any(|snapshot| snapshot.active && snapshot.unavailable.len() == 5)
    }, || format!("{:#?}", preview_snapshots(&state)));
    let snapshot = preview_snapshots(&state).into_iter().last().expect("final preview snapshot");
    assert!(snapshot.active, "a prefetch-only session remains available for explicit retry");
    assert_eq!(snapshot.unavailable, (0..5).map(|n| format!("Artist{n}-Track{n}")).collect::<Vec<_>>());
    let terminal_snapshot_count = preview_snapshots(&state).len();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(preview_snapshots(&state).len(), terminal_snapshot_count,
        "terminal failures must not be retried automatically and starve later rows");

    let report = state.shutdown(Duration::from_secs(1));
    assert!(!report.timed_out, "preview driver must drain: {report:?}");
    state.cleanup_previews();
}

fn discover_snapshots(state: &AppState) -> Vec<DiscoverSnapshot> {
    state.events().iter().filter_map(|envelope| match &envelope.event {
        Event::DiscoverSnapshot { snapshot } => Some(snapshot.clone()),
        _ => None,
    }).collect()
}

fn playback_snapshots(state: &AppState) -> Vec<PlaybackSnapshot> {
    state.events().iter().filter_map(|envelope| match &envelope.event {
        Event::PlaybackSnapshot { snapshot } => Some(snapshot.clone()),
        _ => None,
    }).collect()
}

fn save_states(state: &AppState) -> Vec<SaveState> {
    state.events().iter().filter_map(|envelope| match &envelope.event {
        Event::SaveState { state } => Some(state.clone()),
        _ => None,
    }).collect()
}

fn preview_store_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("luno-gate2-preview-store-{label}-{}", std::process::id()))
}

fn wait_until(deadline_seconds: u64, label: &str, mut ready: impl FnMut() -> bool, detail: impl Fn() -> String) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(deadline_seconds);
    loop {
        if ready() { return; }
        assert!(
            std::time::Instant::now() < deadline,
            "{label} never became ready: {}",
            detail()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn discover_mode_enables_adopts_advances_and_restores() {
    let (_toolchain, helper, ffmpeg) = fixture_preview_toolchain();
    let state = isolated_preview_state("discover-lifecycle", 16);
    state.set_preview_toolchain(Some(helper), Some(ffmpeg));
    let seed = TrackId("seed".into());
    let next = TrackId("next".into());
    state.set_playing(seed.clone(), vec![seed.clone(), next.clone()]);

    let recs = ["Artist0-Track0", "Artist1-Track1", "Artist2-Track2", "Artist3-Track3", "Artist4-Track4"];
    let seeds: Vec<_> = recs.iter().enumerate().map(|(n, _)| preview_seed(&format!("Artist{n}"), &format!("Track{n}"))).collect();
    let value = state
        .dispatch(command("discover-enable", Command::DiscoverEnable { seeds: seeds.clone() }))
        .expect("enable with a playing track must start the session");
    let snapshot: DiscoverSnapshot = serde_json::from_value(value).unwrap();
    assert!(snapshot.enabled, "session must start enabled");
    assert_eq!(snapshot.window, recs);
    assert_eq!(snapshot.ready, 0, "nothing is staged at enable");

    // Enabling while already running is idempotent: the running session wins.
    let value = state
        .dispatch(command("discover-enable-again", Command::DiscoverEnable { seeds: seeds.clone() }))
        .expect("enabling an active session reports the running one");
    let snapshot: DiscoverSnapshot = serde_json::from_value(value).unwrap();
    assert!(snapshot.enabled);

    // The fixture pipeline stages assets; the driver adopts them into the
    // queue. Waiting for the full window guarantees no worker is still in
    // flight, so the advance below is deterministic.
    wait_until(5, "adoption", || {
        discover_snapshots(&state).iter().any(|snapshot| snapshot.enabled && snapshot.ready == 5 && snapshot.queued >= 1)
    }, || format!("{:#?}", discover_snapshots(&state)));
    let latest_discover = discover_snapshots(&state).into_iter().last().expect("discover snapshot");
    assert_eq!(latest_discover.prepared.len(), 5, "every ready recommendation must remain playable after handoff");

    let queue = playback_snapshots(&state).into_iter().last().expect("queue snapshot").queue;
    assert_eq!(queue.first(), Some(&seed), "the seed leads the projection");
    assert_eq!(queue.last(), Some(&next), "the seed successor trails the block");
    assert!(
        queue.iter().any(|id| id.0 == "Artist0-Track0"),
        "the adopted recommendation must sit in the queue: {queue:?}"
    );

    // A queued recommendation starting rolls the window and keeps the session.
    state.set_playing(TrackId("Artist0-Track0".into()), queue.clone());
    let latest = discover_snapshots(&state).into_iter().last().expect("snapshot exists");
    assert!(latest.enabled, "advancing into a recommendation must not end the session: {latest:?}");

    // The explicit advance command mirrors the automatic hook.
    let value = state
        .dispatch(command("discover-advance", Command::DiscoverAdvance))
        .expect("advance must report the session");
    let snapshot: DiscoverSnapshot = serde_json::from_value(value).unwrap();
    assert!(snapshot.enabled);

    // Disable restores the captured physical queue.
    let value = state
        .dispatch(command("discover-disable", Command::DiscoverDisable))
        .expect("disable must end the session");
    let snapshot: DiscoverSnapshot = serde_json::from_value(value).unwrap();
    assert!(!snapshot.enabled, "disable must turn the session off");
    let queue = playback_snapshots(&state).into_iter().last().expect("queue snapshot").queue;
    assert_eq!(queue, vec![seed, next], "the captured queue must be restored");

    wait_until(5, "disable", || {
        discover_snapshots(&state).iter().any(|snapshot| !snapshot.enabled)
    }, || format!("{:#?}", discover_snapshots(&state)));
    for key in recs {
        assert!(
            preview_store_root("discover-lifecycle").join(format!("{key}.aac")).is_file(),
            "disabling discovery must retain the active seed's prepared assets"
        );
    }
    state.cleanup_previews();
}

#[test]
fn discover_handoff_adopts_ready_assets_without_downloading_again() {
    let (_toolchain, helper, ffmpeg) = fixture_preview_toolchain();
    let state = isolated_preview_state("discover-handoff-ready", 16);
    state.set_preview_toolchain(Some(helper.clone()), Some(ffmpeg));
    let seed = TrackId("seed".into());
    state.set_playing(seed.clone(), vec![seed.clone(), TrackId("next".into())]);
    let seeds: Vec<_> = (0..5)
        .map(|n| preview_seed(&format!("Artist{n}"), &format!("Track{n}")))
        .collect();
    state
        .dispatch(command("handoff-ready-enable", Command::DiscoverEnable { seeds: seeds.clone() }))
        .expect("discover mode must start");
    wait_until(5, "discover ready assets", || {
        discover_snapshots(&state).iter().any(|snapshot| snapshot.enabled && snapshot.prepared.len() == 5)
    }, || format!("{:#?}", discover_snapshots(&state)));
    let calls_path = helper.with_extension("calls");
    let calls_before = fs::read_to_string(&calls_path).expect("fixture helper call count").trim().parse::<u32>().unwrap();

    state
        .dispatch(command(
            "handoff-ready-tap",
            Command::PreviewTap {
                seeds,
                key: "Artist0-Track0".into(),
            },
        ))
        .expect("ready recommendation tap must hand off");
    wait_until(5, "focused adoption", || {
        preview_snapshots(&state).iter().any(|snapshot| snapshot.active && snapshot.prepared.iter().any(|key| key == "Artist0-Track0"))
    }, || format!("{:#?}", preview_snapshots(&state)));
    let calls_after = fs::read_to_string(&calls_path).expect("fixture helper call count").trim().parse::<u32>().unwrap();
    assert_eq!(calls_after, calls_before, "handoff must adopt the existing staged asset");
    state.cleanup_previews();
}

#[cfg(target_os = "macos")]
#[test]
fn focused_previous_preserves_cached_asset_for_replay() {
    let (_toolchain, helper, ffmpeg) = fixture_preview_toolchain();
    let state = isolated_preview_state("focused-previous-cache", 16);
    state.set_preview_toolchain(Some(helper.clone()), Some(ffmpeg));
    let previous = TrackId("previous".into());
    let seed = TrackId("seed".into());
    state.set_playing(seed.clone(), vec![previous, seed]);
    let seeds = vec![preview_seed("Artist0", "Track0")];
    state.dispatch(command("prefetch", Command::PreviewPrefetch { seeds: seeds.clone() })).expect("prefetch starts");
    wait_until(5, "prefetch ready", || {
        preview_snapshots(&state).iter().any(|snapshot| snapshot.prepared.contains(&"Artist0-Track0".to_string()))
    }, || format!("{:#?}", preview_snapshots(&state)));
    let calls_path = helper.with_extension("calls");
    let calls_before = fs::read_to_string(&calls_path).unwrap().trim().parse::<u32>().unwrap();

    let first: crate::contracts::types::PreviewSnapshot = serde_json::from_value(
        state.dispatch(command("tap-ready", Command::PreviewTap { seeds: seeds.clone(), key: "Artist0-Track0".into() })).expect("ready tap"),
    ).unwrap();
    assert!(first.prepared.contains(&"Artist0-Track0".to_string()), "ready tap must adopt before responding: {first:?}");

    // The fixture physical ids have no library paths, so playback start may
    // reject after the ownership transition. The cache invariant is the point:
    // Previous must not delete the already published recommendation file.
    let _ = state.dispatch(command("physical-previous", Command::Previous));
    let second: crate::contracts::types::PreviewSnapshot = serde_json::from_value(
        state.dispatch(command("tap-ready-again", Command::PreviewTap { seeds, key: "Artist0-Track0".into() })).expect("cached replay tap"),
    ).unwrap();
    assert!(second.prepared.contains(&"Artist0-Track0".to_string()), "cached replay must be immediately ready: {second:?}");
    let calls_after = fs::read_to_string(&calls_path).unwrap().trim().parse::<u32>().unwrap();
    assert_eq!(calls_after, calls_before, "Previous followed by replay must not invoke the helper again");
    state.cleanup_previews();
}

#[test]
fn discover_mode_next_walks_recommendations_not_the_physical_library() {
    let (_toolchain, helper, ffmpeg) = fixture_preview_toolchain();
    let state = isolated_preview_state("discover-next", 16);
    state.set_preview_toolchain(Some(helper), Some(ffmpeg));
    let seed = TrackId("seed".into());
    let next = TrackId("next".into());
    state.set_playing(seed.clone(), vec![seed.clone(), next.clone()]);
    let seeds = vec![preview_seed("Artist0", "Track0"), preview_seed("Artist1", "Track1")];
    state
        .dispatch(command("next-enable", Command::DiscoverEnable { seeds: seeds.clone() }))
        .expect("enable must start the session");

    // The projection exposes the recommendation block immediately, before any
    // asset is staged, so the very next skip targets a recommendation.
    let playback = playback_snapshots(&state).into_iter().last().expect("queue snapshot");
    assert_eq!(playback.queue.first(), Some(&seed));
    assert_eq!(playback.queue.get(1), Some(&TrackId("Artist0-Track0".into())));

    let value = state.dispatch(command("next", Command::Next)).expect("next must dispatch");
    let playback: PlaybackSnapshot = serde_json::from_value(value).expect("next returns a snapshot");
    let index = playback.current_index.expect("current index") as usize;
    assert_eq!(
        playback.queue[index],
        TrackId("Artist0-Track0".into()),
        "Next must select the recommendation, not the library successor: {playback:?}"
    );
    state.cleanup_previews();
}

#[cfg(target_os = "macos")]
#[test]
fn discover_window_growth_preserves_current_and_supports_both_directions() {
    let (_toolchain, helper, ffmpeg) = fixture_preview_toolchain();
    let state = isolated_preview_state("discover-index-preserved", 16);
    state.set_preview_toolchain(Some(helper), Some(ffmpeg));
    let seed = TrackId("seed".into());
    let next = TrackId("next".into());
    state.set_playing(seed.clone(), vec![seed.clone(), next]);
    let seeds: Vec<_> = (0..7).map(|n| preview_seed(&format!("Artist{n}"), &format!("Track{n}"))).collect();
    state.dispatch(command("enable-index", Command::DiscoverEnable { seeds })).expect("enable");
    let rec0 = TrackId("Artist0-Track0".into());
    state.dispatch(command("select-index", Command::DiscoverSelect { key: rec0.clone() })).expect("discover selection");
    wait_until(5, "selected recommendation start and window growth", || {
        playback_snapshots(&state).last().is_some_and(|snapshot| {
            snapshot.current_index.is_some_and(|index| snapshot.queue.get(index as usize) == Some(&rec0))
                && snapshot.queue.iter().any(|id| id.0 == "Artist5-Track5")
        })
    }, || format!("{:#?}", playback_snapshots(&state)));
    let expanded = playback_snapshots(&state).into_iter().last().unwrap();
    assert_eq!(expanded.queue[expanded.current_index.unwrap() as usize], rec0, "rolling-window growth must retain the current recommendation");
    assert!(expanded.queue.iter().any(|id| id.0 == "Artist5-Track5"), "advance should append the next rolling item");

    let next_snapshot: PlaybackSnapshot = serde_json::from_value(state.dispatch(command("discover-next-live", Command::Next)).expect("next works")).unwrap();
    assert_eq!(next_snapshot.queue[next_snapshot.current_index.unwrap() as usize].0, "Artist1-Track1");
    let previous_snapshot: PlaybackSnapshot = serde_json::from_value(state.dispatch(command("discover-previous-live", Command::Previous)).expect("previous works")).unwrap();
    assert_eq!(previous_snapshot.queue[previous_snapshot.current_index.unwrap() as usize].0, "Artist0-Track0");
    let _ = state.dispatch(command("disable-index", Command::DiscoverDisable));
    state.cleanup_previews();
}

#[test]
fn discover_failure_is_terminal_and_pipeline_continues() {
    let (toolchain, helper, ffmpeg) = fixture_preview_toolchain();
    fs::write(&helper, "#!/bin/sh\ncount=\"$0.calls\"\nn=0\nif [ -f \"$count\" ]; then n=$(cat \"$count\"); fi\nn=$((n + 1))\necho \"$n\" > \"$count\"\nif [ \"$n\" -eq 1 ]; then echo 'provider rejected first candidate' >&2; exit 7; fi\nprintf 'ID3synthetic preview audio source bytes' > \"$1\"\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&helper, permissions).unwrap();
    let state = isolated_preview_state("discover-failure-continues", 16);
    state.set_preview_toolchain(Some(helper), Some(ffmpeg));
    let seed = TrackId("seed".into());
    state.set_playing(seed.clone(), vec![seed]);
    state.dispatch(command("enable-failure", Command::DiscoverEnable {
        seeds: vec![preview_seed("Artist0", "Track0"), preview_seed("Artist1", "Track1")],
    })).expect("enable");
    wait_until(5, "failure then continuation", || {
        discover_snapshots(&state).iter().any(|snapshot| {
            snapshot.unavailable.contains(&"Artist0-Track0".to_string())
                && snapshot.prepared.contains(&"Artist1-Track1".to_string())
        })
    }, || format!("{:#?}", discover_snapshots(&state)));
    let latest = discover_snapshots(&state).into_iter().last().unwrap();
    assert!(!latest.enabled || !latest.window.is_empty());
    drop(toolchain);
    state.cleanup_previews();
}

#[test]
fn discover_mode_handoff_ends_without_restoring_when_the_queue_is_replaced() {
    let state = isolated_preview_state("discover-handoff", 16);
    let staged = preview_store_root("discover-handoff").join("Artist0-Track0.aac");
    fs::write(&staged, b"ID3already prepared recommendation").unwrap();
    let seed = TrackId("seed".into());
    state.set_playing(seed.clone(), vec![seed.clone(), TrackId("next".into())]);
    state
        .dispatch(command("handoff-enable", Command::DiscoverEnable {
            seeds: vec![preview_seed("Artist0", "Track0")],
        }))
        .expect("enable must start the session");
    assert!(
        discover_snapshots(&state)
            .iter()
            .any(|snapshot| snapshot.prepared.contains(&"Artist0-Track0".to_string())),
        "enable must adopt the prepared asset"
    );

    // The user replaces the queue wholesale mid-session.
    let user_track = TrackId("user-track".into());
    state.set_playing(user_track.clone(), vec![user_track.clone()]);

    wait_until(5, "handoff", || {
        discover_snapshots(&state).iter().any(|snapshot| !snapshot.enabled)
    }, || format!("{:#?}", discover_snapshots(&state)));

    let queue = playback_snapshots(&state).into_iter().last().expect("queue snapshot").queue;
    assert_eq!(queue, vec![user_track], "a user-replaced queue must be left alone");

    // Ending queue ownership keeps the active seed's prepared asset available
    // for a later tap or Save.
    wait_until(5, "retained staged asset", || {
        preview_store_root("discover-handoff").join("Artist0-Track0.aac").is_file()
    }, || format!("staged file missing after handoff: {:#?}", std::fs::read_dir(preview_store_root("discover-handoff")).map(|entries| entries.collect::<Vec<_>>())));
    state.cleanup_previews();
}

#[test]
fn discover_save_promotes_staged_once_with_dedupe_and_rejects_duplicates() {
    let (_toolchain, helper, ffmpeg) = fixture_preview_toolchain();
    let state = isolated_preview_state("save-dedupe", 16);
    state.set_preview_toolchain(Some(helper), Some(ffmpeg));
    let lib = fixture_dir("save-library");
    let playlist_dir = lib.join("playlist-a");
    fs::create_dir_all(&playlist_dir).unwrap();
    fs::write(playlist_dir.join("song-a.mp3"), b"fixture song").unwrap();
    state.replace_library(
        Generation(7),
        BTreeMap::from([("playlist-a".to_string(), vec![playlist_dir.join("song-a.mp3")])]),
    );
    state.set_playing(TrackId("seed".into()), vec![TrackId("seed".into())]);
    state
        .dispatch(command("save-tap", Command::PreviewTap {
            seeds: vec![preview_seed("Artist0", "Track0")],
            key: "Artist0-Track0".into(),
        }))
        .expect("tap must start a session");
    wait_until(5, "staged", || {
        preview_snapshots(&state).iter().any(|snapshot| snapshot.prepared.iter().any(|key| key == "Artist0-Track0"))
    }, || format!("{:#?}", preview_snapshots(&state)));

    let playlist = PlaylistId("playlist:playlist-a".into());
    let key = TrackId("Artist0-Track0".into());
    let value = state
        .dispatch(command("save-1", Command::Save { key: key.clone(), playlist_id: playlist.clone() }))
        .expect("staged save must start");
    assert_eq!(value["state"], "saving");

    // A duplicate request while the first is in flight is rejected.
    let error = state
        .dispatch(command("save-2", Command::Save { key: key.clone(), playlist_id: playlist.clone() }))
        .expect_err("in-flight duplicate save must be rejected");
    assert_eq!(error.code, crate::contracts::types::ErrorCode::InvalidRequest);
    assert!(error.message.contains("in progress"), "message: {}", error.message);

    // The promotion completes into the playlist folder as a scannable M4A.
    wait_until(5, "saved", || {
        save_states(&state).iter().any(|save| save.state == SaveStateKind::Saved && save.key == key)
    }, || format!("{:#?}", save_states(&state)));
    let saved = save_states(&state).into_iter().find(|save| save.state == SaveStateKind::Saved && save.key == key).expect("saved receipt");
    let target = playlist_dir.join("Artist0 - Track0.m4a");
    assert_eq!(saved.path.as_deref(), target.to_str(), "saved path mismatch: {saved:?}");
    assert!(target.is_file(), "the saved file must exist at {target:?}");
    assert!(
        preview_store_root("save-dedupe").join("Artist0-Track0.aac").is_file(),
        "the staged preview must be retained for the temporary queue"
    );
    assert!(!saved.already_saved, "first promotion is a fresh save");

    // A later save of the same recommendation dedupes onto the same path.
    let value = state
        .dispatch(command("save-3", Command::Save { key: key.clone(), playlist_id: playlist.clone() }))
        .expect("dedupe save must start");
    assert_eq!(value["state"], "saving");
    wait_until(5, "dedupe", || {
        save_states(&state).iter().filter(|save| save.key == key && save.state == SaveStateKind::Saved && save.already_saved).count() >= 1
    }, || format!("{:#?}", save_states(&state)));
    let repeat = save_states(&state).into_iter().find(|save| save.key == key && save.state == SaveStateKind::Saved && save.already_saved).expect("dedupe receipt");
    assert_eq!(repeat.path.as_deref(), saved.path.as_deref());

    // The in-flight guard releases after completion: a fresh save is accepted again.
    let _ = state
        .dispatch(command("save-4", Command::Save { key: key.clone(), playlist_id: playlist.clone() }))
        .expect("in-flight guard must release after the first promotion completes");
    state.cleanup_previews();
}

#[test]
fn discover_save_requires_explicit_playlist_and_queues_downloads_for_unstaged() {
    let (_toolchain, helper, ffmpeg) = fixture_preview_toolchain();
    let state = isolated_preview_state("save-rules", 16);
    state.set_preview_toolchain(Some(helper.clone()), Some(ffmpeg));
    let lib = fixture_dir("save-rules-library");
    let playlist_dir = lib.join("playlist-a");
    fs::create_dir_all(&playlist_dir).unwrap();
    fs::write(playlist_dir.join("song-a.mp3"), b"fixture song").unwrap();
    state.set_scan_roots(vec![lib.clone()]);
    state.replace_library(
        Generation(7),
        BTreeMap::from([
            ("playlist-a".to_string(), vec![playlist_dir.join("song-a.mp3")]),
            ("empty-a".to_string(), Vec::new()),
        ]),
    );
    state.set_playing(TrackId("seed".into()), vec![TrackId("seed".into())]);

    let key = TrackId("Artist0-Track0".into());
    let destination = PlaylistId("playlist:playlist-a".into());

    // Unknown playlists are rejected before any work.
    let error = state
        .dispatch(command("save-unknown", Command::Save { key: key.clone(), playlist_id: PlaylistId("playlist:missing".into()) }))
        .expect_err("unknown playlists must be rejected");
    assert_eq!(error.code, crate::contracts::types::ErrorCode::NotFound);

    // A playlist without a directory cannot receive saves.
    let error = state
        .dispatch(command("save-nodir", Command::Save { key: key.clone(), playlist_id: PlaylistId("playlist:empty-a".into()) }))
        .expect_err("playlists without a directory must be rejected");
    assert!(error.message.contains("no directory"), "message: {}", error.message);

    // Unstaged keys with no downloader configured fail truthfully.
    let error = state
        .dispatch(command("save-unstaged", Command::Save { key: key.clone(), playlist_id: destination.clone() }))
        .expect_err("unstaged saves require a downloader");
    assert_eq!(error.code, crate::contracts::types::ErrorCode::Unavailable);
    assert!(error.message.contains("no staged audio"), "message: {}", error.message);

    // With the downloader configured, an unstaged save queues a durable job.
    state.set_bundle_helper(Some(helper));
    let value = state
        .dispatch(command("save-queued", Command::Save { key: key.clone(), playlist_id: destination.clone() }))
        .expect("unstaged save with a downloader must queue");
    assert_eq!(value["state"], "queued");
    wait_until(5, "queued", || {
        save_states(&state).iter().any(|save| save.state == SaveStateKind::Queued && save.key == key)
    }, || format!("{:#?}", save_states(&state)));
    let queued = save_states(&state).into_iter().find(|save| save.state == SaveStateKind::Queued && save.key == key).expect("queued receipt");
    assert!(queued.job_id.is_some(), "queued saves carry the download job: {queued:?}");
    wait_until(5, "downloaded", || {
        // No seed is known for an unstaged save without a Discover session,
        // so the filename falls back to the recommendation key.
        playlist_dir.join("Artist0-Track0.m4a").is_file()
    }, || format!("download never landed in {playlist_dir:?}: {:#?}", std::fs::read_dir(&playlist_dir).map(|entries| entries.collect::<Vec<_>>())));
    wait_until(5, "saved terminal state", || {
        save_states(&state).iter().any(|save| save.state == SaveStateKind::Saved && save.key == key)
    }, || format!("save never left queued state: {:#?}", save_states(&state)));

    // The picker supports "Create new playlist" followed immediately by
    // saving the recommendation. This must not race the asynchronous scan of
    // the new empty folder.
    state.dispatch(command("create-for-save", Command::CreatePlaylist { name: "fresh-rec-list".into() })).expect("create recommendation playlist");
    let fresh_key = TrackId("Artist1-Track1".into());
    let fresh_playlist = PlaylistId("playlist:fresh-rec-list".into());
    state.dispatch(command("save-created", Command::Save { key: fresh_key.clone(), playlist_id: fresh_playlist })).expect("save into just-created playlist");
    wait_until(5, "created playlist save", || {
        save_states(&state).iter().any(|save| save.state == SaveStateKind::Saved && save.key == fresh_key)
    }, || format!("create-and-save never completed: {:#?}", save_states(&state)));
    state.cleanup_previews();
}

#[test]
fn target_command_boundary_loads_config_registers_root_and_syncs_markers() {
    let home = fixture_dir("command-config");
    let root = home.join("Music");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        home.join(".luno_config.json"),
        format!(r#"{{"music_root":"{}","favorite_songs":["fixture://track-a"],"volume":0.5,"unknown":true}}"#, root.display()).as_bytes(),
    )
    .unwrap();
    fs::write(root.join(".downloaded_vids"), "success-id\n").unwrap();
    fs::write(root.join(".sync_failed_vids"), "permanent-id\tblocked\n").unwrap();
    let state = AppState::new(2);

    let config = state
        .dispatch(command(
            "config",
            Command::LoadConfig {
                home: home.display().to_string(),
            },
        ))
        .unwrap();
    assert_eq!(config["music_root"], root.display().to_string());
    assert_eq!(state.scan_roots(), vec![root.clone()]);
    let markers = state
        .dispatch(command(
            "markers",
            Command::SyncMarkers {
                folder: root.display().to_string(),
            },
        ))
        .unwrap();
    assert_eq!(markers["successful"], 1);
    assert_eq!(markers["permanent_failures"], 1);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn target_command_boundary_clears_previous_root_for_valid_legacy_config_without_root() {
    let home = fixture_dir("command-config-no-root");
    let previous_root = home.join("previous-music");
    fs::create_dir_all(&previous_root).unwrap();
    fs::write(&home.join(".luno_config.json"), br#"{"favorite_songs":[]}"#).unwrap();
    let state = AppState::new(1);
    state.set_scan_roots(vec![previous_root]);

    state
        .dispatch(command(
            "config-without-root",
            Command::LoadConfig {
                home: home.display().to_string(),
            },
        ))
        .unwrap();

    assert!(state.scan_roots().is_empty());
    fs::remove_dir_all(home).unwrap();
}

#[cfg(unix)]
#[test]
fn target_command_boundary_dispatches_root_statuses_without_home_fallback() {
    use std::os::unix::fs::PermissionsExt;

    let dir = fixture_dir("command-scan-statuses");
    let empty = dir.join("empty");
    let denied = dir.join("denied");
    fs::create_dir_all(&empty).unwrap();
    fs::create_dir_all(&denied).unwrap();
    let mut permissions = fs::metadata(&denied).unwrap().permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&denied, permissions).unwrap();
    let missing = dir.join("missing");
    let restore = denied.clone();
    let state = AppState::new(2);
    let (sender, receiver) = std::sync::mpsc::channel();
    state.set_event_sink(Arc::new(move |event| {
        sender.send(event).unwrap();
        Ok(())
    }));
    fs::write(
        dir.join(".luno_config.json"),
        format!(r#"{{"music_root":"{}"}}"#, empty.display()),
    )
    .unwrap();
    state
        .dispatch(command(
            "load-config",
            Command::LoadConfig {
                home: dir.display().to_string(),
            },
        ))
        .unwrap();
    state
        .dispatch(command("scan-empty", Command::ScanLibrary))
        .unwrap();
    let empty_status = loop {
        if let Event::LibraryRootStatus { status, .. } = receiver.recv_timeout(Duration::from_secs(2)).unwrap().event {
            break status;
        }
    };
    assert_eq!(empty_status, ContractRootStatus::Scanned);
    state
        .dispatch(command("register", Command::RegisterRoot {
            root: missing.display().to_string(),
        }))
        .unwrap();
    state
        .dispatch(command("scan-missing", Command::ScanLibrary))
        .unwrap();
    let missing_status = loop {
        if let Event::LibraryRootStatus { status, .. } = receiver.recv_timeout(Duration::from_secs(2)).unwrap().event {
            break status;
        }
    };
    assert_eq!(missing_status, ContractRootStatus::Missing);
    state
        .dispatch(command("register-denied", Command::RegisterRoot {
            root: denied.display().to_string(),
        }))
        .unwrap();
    state
        .dispatch(command("scan-denied", Command::ScanLibrary))
        .unwrap();
    let denied_status = loop {
        if let Event::LibraryRootStatus { status, .. } = receiver.recv_timeout(Duration::from_secs(2)).unwrap().event {
            break status;
        }
    };
    if denied_status != ContractRootStatus::Inaccessible {
        assert_eq!(denied_status, ContractRootStatus::Scanned);
    }
    let mut permissions = fs::metadata(&restore).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&restore, permissions).unwrap();
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn target_config_and_marker_failures_are_explicit() {
    let dir = fixture_dir("explicit-persistence-failures");
    let config = dir.join(".luno_config.json");
    let state = AppState::new(1);
    assert_eq!(
        state
            .dispatch(command("missing-config", Command::LoadConfig {
                home: dir.display().to_string(),
            }))
            .unwrap_err()
            .code,
        crate::contracts::types::ErrorCode::NotFound
    );
    fs::write(&config, br#"{"music_root":"   "}"#).unwrap();
    assert_eq!(
        state
            .dispatch(command("blank-config", Command::LoadConfig {
                home: dir.display().to_string(),
            }))
            .unwrap_err()
            .code,
        crate::contracts::types::ErrorCode::InvalidRequest
    );
    assert!(state.scan_roots().is_empty());
    fs::write(&config, b"not-json").unwrap();
    assert_eq!(
        state
            .dispatch(command("corrupt-config", Command::LoadConfig {
                home: dir.display().to_string(),
            }))
            .unwrap_err()
            .code,
        crate::contracts::types::ErrorCode::InvalidRequest
    );
    assert_eq!(
        state
            .dispatch(command("missing-markers", Command::SyncMarkers {
                folder: dir.join("missing-markers").display().to_string(),
            }))
            .unwrap_err()
            .code,
        crate::contracts::types::ErrorCode::InvalidRequest
    );
    fs::write(dir.join(crate::download::markers::CURRENT_SUCCESS), "bad id\n").unwrap();
    assert_eq!(
        state
            .dispatch(command("corrupt-markers", Command::SyncMarkers {
                folder: dir.display().to_string(),
            }))
            .unwrap_err()
            .code,
        crate::contracts::types::ErrorCode::InvalidRequest
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn target_command_boundary_toggles_and_prunes_favorites_and_keeps_recent_session_only() {
    let state = AppState::new(2);
    let live = PathBuf::from("/synthetic/live.mp3");
    let missing = PathBuf::from("/synthetic/missing.mp3");
    state.replace_library(
        crate::contracts::types::Generation(1),
        std::collections::BTreeMap::from([(String::from("Unsorted"), vec![live.clone()])]),
    );
    let track_id = state.library().tracks[0].id.clone();
    state.set_favorite_path(missing.clone());
    let toggled = state
        .dispatch(command(
            "favorite",
            Command::ToggleFavorite {
                track_id: track_id.clone(),
            },
        ))
        .unwrap();
    assert_eq!(toggled, serde_json::json!({ "favorite": true }));
    assert!(state.library().tracks[0].favorite);
    state.prune_favorites();
    assert!(state.favorite_paths().contains(&live.display().to_string()));
    assert!(!state
        .favorite_paths()
        .contains(&missing.display().to_string()));
    state
        .dispatch(command(
            "recent-start",
            Command::RecordRecent {
                track_id: track_id.clone(),
            },
        ))
        .unwrap();
    let recent = state
        .dispatch(command("recent", Command::GetRecent))
        .unwrap();
    assert_eq!(recent, serde_json::json!([track_id.0]));
    state
        .dispatch(command("recent-clear", Command::ClearRecent))
        .unwrap();
    assert_eq!(
        state
            .dispatch(command("recent-empty", Command::GetRecent))
            .unwrap(),
        serde_json::json!([])
    );
    let paths: Vec<_> = (0..101)
        .map(|index| PathBuf::from(format!("/synthetic/{index}.mp3")))
        .collect();
    state.replace_library(
        crate::contracts::types::Generation(2),
        std::collections::BTreeMap::from([(String::from("Unsorted"), paths)]),
    );
    let ids: Vec<_> = state.library().tracks.iter().map(|track| track.id.clone()).collect();
    for id in &ids {
        state
            .dispatch(command(
                "recent-cap",
                Command::RecordRecent { track_id: id.clone() },
            ))
            .unwrap();
    }
    let recent = state.dispatch(command("recent-cap-get", Command::GetRecent)).unwrap();
    assert_eq!(recent.as_array().map(Vec::len), Some(100));
    assert_eq!(
        state
            .dispatch(command(
                "recent-unknown",
                Command::RecordRecent {
                    track_id: crate::contracts::types::TrackId("unknown".into()),
                },
            ))
            .unwrap_err()
            .code,
        crate::contracts::types::ErrorCode::NotFound
    );
}

#[test]
fn target_scan_scheduler_cancellation_is_observed_through_dispatch() {
    let state = AppState::new(1);
    let entered = Arc::new(Barrier::new(2));
    let worker_entered = entered.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    state.set_event_sink(Arc::new(move |event| {
        sender.send(event).unwrap();
        Ok(())
    }));
    let scan = state
        .scan_async_with(move |_, token| {
            worker_entered.wait();
            while !token.is_cancelled() {
                std::thread::yield_now();
            }
            crate::library::scanner::ScanOutcome {
                playlists: Default::default(),
                roots: vec![],
                errors: vec![],
            }
        })
        .unwrap();
    entered.wait();
    state
        .dispatch(command("cancel", Command::CancelJob {
            job_id: crate::contracts::types::JobId("library-scan".into()),
        }))
        .unwrap();
    let report = state.shutdown(Duration::from_secs(1));
    assert!(!report.timed_out);
    assert!(receiver.try_recv().is_err());
    assert_eq!(scan, crate::contracts::types::Generation(1));
}

#[test]
fn target_command_boundary_promotes_validated_audio_then_scans_and_gets_library() {
    let dir = fixture_dir("promotion-command");
    let root = dir.join("library");
    let playlist = root.join("playlist-a");
    fs::create_dir_all(&playlist).unwrap();
    let temporary = stage_output(playlist.join("Artist - Track.mp3")).unwrap();
    fs::write(&temporary, b"ID3synthetic-audio").unwrap();
    let state = AppState::new(2);
    state.set_scan_roots(vec![root.clone()]);

    let result = state
        .dispatch(command(
            "promote",
                Command::PromoteAudioOutput {
                    temporary: temporary.display().to_string(),
                    destination: playlist.join("Artist - Track.mp3").display().to_string(),
                },
        ))
        .unwrap();
    assert_eq!(result["bytes"], 18);
    assert!(playlist.join("Artist - Track.mp3").is_file());

    state.dispatch(command("scan", Command::ScanLibrary)).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while state.library().tracks.is_empty() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    let library = state.dispatch(command("get", Command::GetLibrary)).unwrap();
    assert_eq!(library["tracks"][0]["title"], "Track");
    assert_eq!(library["playlists"][0]["name"], "playlist-a");

    let cases = [
        ("missing", root.join("missing.mp3"), playlist.join("missing.mp3")),
        ("invalid", dir.join("invalid.part"), playlist.join("invalid.mp3")),
        ("escape", dir.join("escape.part"), dir.join("outside.mp3")),
    ];
    for (request_id, temporary, destination) in cases {
        if request_id != "missing" {
            fs::write(&temporary, b"not audio").unwrap();
        }
        let error = state
            .dispatch(command(
                request_id,
                Command::PromoteAudioOutput {
                    temporary: temporary.display().to_string(),
                    destination: destination.display().to_string(),
                },
            ))
            .unwrap_err();
        assert_eq!(
            error.code,
            if request_id == "missing" {
                crate::contracts::types::ErrorCode::NotFound
            } else {
                crate::contracts::types::ErrorCode::InvalidRequest
            }
        );
        assert!(!destination.exists());
    }
    let collision_temporary = stage_output(playlist.join("collision.mp3")).unwrap();
    fs::write(&collision_temporary, b"ID3collision").unwrap();
    let collision = playlist.join("Artist - Track.mp3");
    let error = state
        .dispatch(command(
            "collision",
            Command::PromoteAudioOutput {
                temporary: collision_temporary.display().to_string(),
                destination: collision.display().to_string(),
            },
        ))
        .unwrap_err();
    assert_eq!(error.code, crate::contracts::types::ErrorCode::Busy);
    assert_eq!(fs::read(&collision).unwrap(), b"ID3synthetic-audio");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn target_command_boundary_cancels_actual_os_filesystem_scan_before_publication() {
    let dir = fixture_dir("os-scan-cancel");
    let root = dir.join("library");
    fs::create_dir_all(&root).unwrap();
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let hooked_root = root.clone();
    let _hook = crate::library::scanner::scoped_os_scan_hook({
        let entered = entered.clone();
        let release = release.clone();
        Arc::new(move |path| {
            if path == hooked_root {
                entered.wait();
                release.wait();
            }
        })
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    let received = events.clone();
    let state = AppState::new(1);
    state.set_scan_roots(vec![root]);
    state.set_event_sink(Arc::new(move |event| {
        received.lock().unwrap_or_else(|e| e.into_inner()).push(event);
        Ok(())
    }));
    state.dispatch(command("scan", Command::ScanLibrary)).unwrap();
    entered.wait();
    state
        .dispatch(command(
            "cancel-scan",
            Command::CancelJob {
                job_id: crate::contracts::types::JobId("library-scan".into()),
            },
        ))
        .unwrap();
    release.wait();
    let report = state.shutdown(Duration::from_secs(1));
    assert!(!report.timed_out);
    assert_eq!(state.tasks.active_count(), 0);
    assert!(events.lock().unwrap_or_else(|e| e.into_inner()).iter().all(|event| !matches!(
        event.event,
        Event::LibrarySnapshot { .. } | Event::LibraryRootStatus { .. }
    )));
    assert_eq!(state.library().generation.0, 0);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn target_scan_scheduler_reports_followup_spawn_capacity_failure() {
    let state = AppState::new(2);
    let blocker_started = Arc::new(Barrier::new(2));
    let blocker_worker = blocker_started.clone();
    let blocker_release = Arc::new(Barrier::new(2));
    let blocker_worker_release = blocker_release.clone();
    state
        .submit(state.generation("blocker", crate::contracts::types::Generation(1)), move |token| {
            blocker_worker.wait();
            blocker_worker_release.wait();
            while !token.is_cancelled() {
                std::thread::yield_now();
            }
        })
        .unwrap();
    blocker_started.wait();
    let first_started = Arc::new(Barrier::new(2));
    let first_release = Arc::new(Barrier::new(2));
    let first_worker_started = first_started.clone();
    let first_worker_release = first_release.clone();
    let followup_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let followup_calls_worker = followup_calls.clone();
    let first = state
        .scan_async_with(move |_, _| {
            first_worker_started.wait();
            first_worker_release.wait();
            crate::library::scanner::ScanOutcome {
                playlists: Default::default(),
                roots: vec![],
                errors: vec![],
            }
        })
        .unwrap();
    first_started.wait();
    state
        .scan_async_with(move |_, _| {
            followup_calls_worker.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            panic!("follow-up should not spawn at capacity");
        })
        .unwrap();
    first_release.wait();
    let snapshot = loop {
        if let Some(event) = state
            .events()
            .into_iter()
            .find(|event| matches!(event.event, Event::LibrarySnapshot { .. }))
        {
            break event;
        }
        std::thread::yield_now();
    };
    blocker_release.wait();
    let report = state.shutdown(Duration::from_secs(1));
    assert!(!report.timed_out);
    assert_eq!(snapshot.generation, first);
    assert_eq!(followup_calls.load(std::sync::atomic::Ordering::Acquire), 0);
    assert_eq!(first, crate::contracts::types::Generation(1));
}

#[test]
fn target_scan_scheduler_coalesces_refreshes_and_rejects_stale_results_without_sleeping() {
    let state = AppState::new(2);
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let entered_worker = entered.clone();
    let release_worker = release.clone();
    let (events, received) = std::sync::mpsc::channel();
    state.set_event_sink(Arc::new(move |event| {
        events.send(event).unwrap();
        Ok(())
    }));
    let first_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let first_calls_worker = first_calls.clone();
    let followup_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let followup_calls_worker = followup_calls.clone();
    let first = state
        .scan_async_with(move |_roots, token| {
            assert_eq!(
                first_calls_worker.fetch_add(1, std::sync::atomic::Ordering::AcqRel),
                0
            );
            entered_worker.wait();
            release_worker.wait();
            assert!(!token.is_cancelled());
            crate::library::scanner::ScanOutcome {
                playlists: Default::default(),
                roots: vec![],
                errors: vec![],
            }
        })
        .unwrap();
    entered.wait();
    let followup = followup_calls_worker.clone();
    let coalesced = state
        .scan_async_with(move |_, token| {
            followup.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            assert!(!token.is_cancelled());
            crate::library::scanner::ScanOutcome {
                playlists: Default::default(),
                roots: vec![],
                errors: vec![],
            }
        })
        .unwrap();
    assert_eq!(coalesced, first);
    release.wait();
    let first_event = received.recv().unwrap();
    let second_event = received.recv().unwrap();
    assert_eq!(first_event.generation, first);
    assert!(second_event.generation.0 > first.0);
    assert!(matches!(first_event.event, Event::LibrarySnapshot { .. }));
    assert!(matches!(second_event.event, Event::LibrarySnapshot { .. }));
    assert_eq!(first_calls.load(std::sync::atomic::Ordering::Acquire), 1);
    assert_eq!(followup_calls.load(std::sync::atomic::Ordering::Acquire), 1);
}

#[test]
fn target_scan_publication_rejects_late_older_result_after_newer_task_is_current() {
    let state = AppState::new(3);
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let old_entered = entered.clone();
    let old_release = release.clone();
    let (events, received) = std::sync::mpsc::channel();
    state.set_event_sink(Arc::new(move |event| {
        events.send(event).unwrap();
        Ok(())
    }));

    let old_generation = state
        .scan_async_with(move |_, _| {
            old_entered.wait();
            old_release.wait();
            crate::library::scanner::ScanOutcome {
                playlists: std::collections::BTreeMap::from([(
                    "old".into(),
                    vec![PathBuf::from("old.mp3")],
                )]),
                roots: vec![],
                errors: vec![],
            }
        })
        .unwrap();
    entered.wait();

    let newer_generation = crate::contracts::types::Generation(old_generation.0 + 1);
    let newer_spec = state.generation("library-scan", newer_generation);
    let newer_state = state.clone();
    state
        .submit(newer_spec.clone(), move |_| {
            assert!(newer_state.publish_scan_result_if_current(
                &newer_spec,
                newer_generation,
                crate::library::scanner::ScanOutcome {
                    playlists: std::collections::BTreeMap::from([(
                        "new".into(),
                        vec![PathBuf::from("new.mp3")],
                    )]),
                    roots: vec![],
                    errors: vec![],
                },
            ));
        })
        .unwrap();

    let newer_event = received.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(newer_event.event, Event::LibrarySnapshot { .. }));
    release.wait();
    let report = state.shutdown(Duration::from_secs(1));
    assert!(!report.timed_out);
    assert_eq!(state.library().generation, newer_generation);
    assert_eq!(state.library().playlists[0].name, "new");
    assert!(received.try_recv().is_err(), "older result was published");
}

#[cfg(target_os = "macos")]
#[test]
fn migration_state_and_rollback_are_retryable_without_touching_legacy_files() {
    use crate::migration::first_run::{
        migrate, CrashStage, Keychain, KeychainImport, MigrationOptions, MigrationPaths,
    };

    struct TestKeychain;
    impl Keychain for TestKeychain {
        fn import_lastfm_key(&mut self, _: &str) -> Result<KeychainImport, String> {
            Ok(KeychainImport::Created)
        }
        fn rollback_lastfm_key(&mut self, _: KeychainImport) -> Result<(), String> {
            Ok(())
        }
    }

    let home = fs::canonicalize(fixture_dir("migration")).unwrap();
    let music = home.join("Music");
    let target = home.join("Target");
    let preview = home.join(".luno_previews");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&target).unwrap();
    fs::create_dir_all(&preview).unwrap();
    let config = home.join(".luno_config.json");
    let config_bytes = br#"{"music_root":"Music","favorite_songs":["fixture://track-a"]}"#;
    fs::write(&config, config_bytes).unwrap();
    fs::write(
        music.join(".luno_playlist_urls.json"),
        br#"{"playlist-a":"https://example.test/list"}"#,
    )
    .unwrap();
    fs::write(
        music.join(".luno_allowed_dups.json"),
        br#"["artistatracka"]"#,
    )
    .unwrap();
    fs::write(music.join(".downloaded.txt"), b"legacy title legacy-id\n").unwrap();
    fs::write(preview.join("stale.mp3"), b"synthetic").unwrap();
    let paths = MigrationPaths {
        legacy_root: home.clone(),
        legacy_data_root: Some(music.clone()),
        configured_music_root: Some(music),
        target_root: target,
        preview_dir: preview,
        current_preview: vec![],
        promoted_assets: vec![],
    };

    let mut keychain = TestKeychain;
    let failed = migrate(
        &paths,
        &MigrationOptions {
            crash_at: Some(CrashStage::Converted),
            ..Default::default()
        },
        &mut keychain,
    );
    assert!(failed.is_err());
    assert_eq!(fs::read(&config).unwrap(), config_bytes);
    assert!(!paths.state_file().exists());

    let mut keychain = TestKeychain;
    let receipt = migrate(&paths, &MigrationOptions::default(), &mut keychain).unwrap();
    assert!(receipt.snapshot.is_some());
    assert!(paths.state_file().is_file());
    assert!(paths.version_record().is_file());
    let state = fs::read_to_string(paths.state_file()).unwrap();
    assert!(state.contains("legacy-id"));
    assert!(!paths.preview_dir.join("stale.mp3").exists());
    let mut keychain = TestKeychain;
    let retry = migrate(&paths, &MigrationOptions::default(), &mut keychain).unwrap();
    assert!(retry.snapshot.is_none());
    fs::remove_dir_all(home).unwrap();
}
