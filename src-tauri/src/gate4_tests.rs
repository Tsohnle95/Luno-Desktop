use crate::contracts::events::{Command as BackendCommand, CommandEnvelope, Event};
use crate::contracts::types::JobId;
use crate::domain::identity::{ProviderIdentity, TrackMetadata};
use crate::download::coordinator::{
    DownloadCoordinator, DownloadEvent, DownloadRequest, StartError,
};
use crate::download::csv::{parse, CsvError};
use crate::download::markers::Markers;
use crate::download::promotion::{
    promote_output, stage_output, validate_output, CollisionPolicy, Validation,
};
use crate::download::sync::{plan, plan_with_state, sync_sequential, sync_sequential_observed_with_state, ItemOutcome, PlaylistEntry, PlaylistSyncState, SyncItem};
use crate::download::workflows::{run_url_queue, search_sources, validate_http_url, QueueEvent};
use crate::persistence::atomic_write::write_atomic;
use crate::persistence::legacy::read_config;
use crate::persistence::recovery::{recover, RecoveryResult};
use crate::runtime::task_registry::CancellationToken;
use crate::transfer::codec::{decode, encode, Manifest, Playlist, Source, Track};
use crate::transfer::planner::{apply_import_plan, plan_import, MatchPlan};
use crate::transfer::provider_index::{IndexedTrack, ProviderIndex};
use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

struct Fixture(PathBuf);

impl std::ops::Deref for Fixture {
    type Target = std::path::Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<std::path::Path> for Fixture {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(label: &str) -> Fixture {
    let path = std::env::temp_dir().join(format!("luno-gate4-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    Fixture(path)
}

fn manifest() -> Manifest {
    Manifest {
        format: "luno.library.export".into(),
        manifest_version: 1,
        scope: "library".into(),
        created_at: "2026-08-15T00:00:00Z".into(),
        tracks: vec![
            Track {
                r#ref: "track-a".into(),
                title: "Track A".into(),
                artist: "Artist A".into(),
                album: "Album A".into(),
                duration_ms: 100_000,
                source: Some(Source {
                    provider: "youtube".into(),
                    id: "provider-video-1".into(),
                }),
                ambiguity_confirmation_required: false,
            },
            Track {
                r#ref: "track-b".into(),
                title: "Track B".into(),
                artist: "Artist B".into(),
                album: "".into(),
                duration_ms: 0,
                source: None,
                ambiguity_confirmation_required: true,
            },
        ],
        playlists: vec![Playlist {
            name: "Playlist Unsafe".into(),
            description: "fixture".into(),
            track_refs: vec!["track-a".into(), "track-b".into()],
        }],
        unassigned_track_refs: vec![],
    }
}

#[cfg(unix)]
fn fake_helper(root: &std::path::Path) -> PathBuf {
    let helper = root.join("fake-helper");
    fs::write(
        &helper,
        "#!/bin/sh\nprintf '%s' \"$HOME\" > \"$HOME/helper-home-seen\"\nif [ \"$1\" = \"--dump-json\" ]; then\n  printf '%s\\n' '{\"id\":\"fixture-video\",\"title\":\"Fixture Track\",\"uploader\":\"Fixture Artist\",\"webpage_url\":\"https://example.test/watch?v=fixture-video\"}'\n  exit 0\nfi\ncase \"$1\" in\n  missing) exit 0 ;;\n  partial) printf 'not audio' > \"$2\" ;;\n  valid) printf 'ID3synthetic-audio' > \"$2\" ;;\n  *) printf 'ID3synthetic-audio' > \"$1\" ;;\nesac\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&helper).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&helper, permissions).unwrap();
    // The downloader search discovers the yt-dlp BINARY beside the helper.
    // A fake that answers `--dump-json` with one fixture hit keeps the search
    // boundary deterministic without network access.
    let ytdlp = root.join("yt-dlp");
    fs::write(
        &ytdlp,
        "#!/bin/sh\nif [ \"$1\" = \"--dump-json\" ]; then\n  printf '%s\\n' '{\"id\":\"fixture-video\",\"title\":\"Fixture Track\",\"uploader\":\"Fixture Artist\",\"webpage_url\":\"https://example.test/watch?v=fixture-video\",\"duration\":268}'\n  exit 0\nfi\nexit 1\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&ytdlp).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&ytdlp, permissions).unwrap();
    helper
}

#[test]
fn downloader_search_direct_csv_and_queue_policy() {
    assert_eq!(
        validate_http_url(" HTTPS://example.test/watch ").unwrap(),
        "HTTPS://example.test/watch"
    );
    assert!(validate_http_url("file:///tmp/audio").is_err());
    assert_eq!(search_sources("artist track", true).unwrap().len(), 2);
    let tracks = parse(
        br#"Track Name,Artist Name(s),Album Name,URL
"Track, A","Artist A","Album, A",https://example.test/a
"Track B","Artist B",,
"#,
    )
    .unwrap();
    assert_eq!(tracks[0].title, "Track, A");
    assert_eq!(tracks[0].album.as_deref(), Some("Album, A"));
    assert!(matches!(
        parse(b"Track Name,Artist Name(s)\n\"broken"),
        Err(CsvError::UnterminatedQuote)
    ));
    let cancelled = Arc::new(AtomicBool::new(false));
    let events = run_url_queue(
        &["https://example.test/a".into(), "bad".into()],
        &cancelled,
        |_, _| Ok(()),
    );
    assert!(matches!(
        events.last(),
        Some(QueueEvent::Complete {
            succeeded: 1,
            failed: 1
        })
    ));
}

#[test]
fn downloader_output_states_and_atomic_promotion_are_truthful() {
    let root = fixture("promotion");
    #[cfg(unix)]
    let helper = fake_helper(&root);
    #[cfg(unix)]
    let synthetic_home = root.join("home");
    #[cfg(unix)]
    fs::create_dir_all(&synthetic_home).unwrap();
    #[cfg(unix)]
    for mode in ["missing", "partial", "valid"] {
        let destination = root.join(format!("helper-{mode}.mp3"));
        let temporary = stage_output(&destination).unwrap();
        let status = Command::new(&helper)
            .arg(mode)
            .arg(&temporary)
            .env("HOME", &synthetic_home)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            fs::read_to_string(synthetic_home.join("helper-home-seen")).unwrap(),
            synthetic_home.to_string_lossy()
        );
        let result = promote_output(
            &temporary,
            &destination,
            Validation::audio(),
            CollisionPolicy::Reject,
            None,
        );
        match mode {
            "valid" => assert!(result.is_ok()),
            _ => assert!(result.is_err()),
        }
    }
    #[cfg(not(unix))]
    eprintln!(
        "UNAVAILABLE: Gate 4 fake helper executable evidence requires Unix process semantics"
    );
    let missing = root.join("missing.part");
    assert!(validate_output(&missing, Validation::audio()).is_err());
    let partial = stage_output(root.join("partial.mp3")).unwrap();
    fs::write(&partial, b"not audio").unwrap();
    assert!(promote_output(
        &partial,
        root.join("partial.mp3"),
        Validation::audio(),
        CollisionPolicy::Reject,
        None
    )
    .is_err());
    let valid = stage_output(root.join("valid.mp3")).unwrap();
    fs::write(&valid, b"ID3synthetic-audio").unwrap();
    let receipt = promote_output(
        &valid,
        root.join("valid.mp3"),
        Validation::audio(),
        CollisionPolicy::Reject,
        None,
    )
    .unwrap();
    assert_eq!(receipt.bytes, fs::metadata(&receipt.path).unwrap().len());
    assert!(!valid.exists());
    let m4a = stage_output(root.join("valid.m4a")).unwrap();
    fs::write(&m4a, b"\x00\x00\x00\x20ftypM4A \x00\x00\x00\x00M4A ").unwrap();
    let receipt = promote_output(
        &m4a,
        root.join("valid.m4a"),
        Validation::audio(),
        CollisionPolicy::Reject,
        None,
    )
    .unwrap();
    assert_eq!(receipt.bytes, fs::metadata(&receipt.path).unwrap().len());
    assert!(!m4a.exists());
}

#[test]
fn downloader_coordinator_exposes_busy_progress_cancel_error_and_terminal_states() {
    let root = fixture("coordinator");
    let coordinator = DownloadCoordinator::new();
    let events = coordinator.events().unwrap();
    let request = |id: &str| DownloadRequest {
        job_id: id.into(),
        destination: root.join(format!("{id}.mp3")),
    };
    coordinator
        .start(
            request("cancel"),
            Box::new(|_, token, progress| {
                progress(0.5);
                while !token.is_cancelled() {
                    thread::sleep(Duration::from_millis(1));
                }
                Err("worker stopped".into())
            }),
        )
        .unwrap();
    let first = events
        .recv_timeout(Duration::from_millis(100))
        .expect("timed out waiting for start");
    let second = events
        .recv_timeout(Duration::from_millis(100))
        .expect("timed out waiting for progress");
    assert!(matches!(first, DownloadEvent::Started { .. }));
    assert!(matches!(second, DownloadEvent::Progress { .. }));
    assert_eq!(
        coordinator.start(request("busy"), Box::new(|_, _, _| Ok(()))),
        Err(StartError::Busy)
    );
    assert!(coordinator.cancel());
    let mut observed = vec![first, second];
    for _ in 0..20 {
        match events.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                let terminal = matches!(event, DownloadEvent::Cancelled { .. });
                observed.push(event);
                if terminal {
                    break;
                }
            }
            Err(_) => panic!("timed out waiting for cancellation events"),
        }
    }
    assert!(observed
        .iter()
        .any(|event| matches!(event, DownloadEvent::Started { .. })));
    assert!(observed
        .iter()
        .any(|event| matches!(event, DownloadEvent::Progress { .. })));
    assert!(observed
        .iter()
        .any(|event| matches!(event, DownloadEvent::Cancelling { .. })));
    assert!(observed
        .iter()
        .any(|event| matches!(event, DownloadEvent::Cancelled { .. })));
    let cancelling = observed.iter().position(|event| matches!(event, DownloadEvent::Cancelling { .. })).unwrap();
    let cancelled = observed.iter().position(|event| matches!(event, DownloadEvent::Cancelled { .. })).unwrap();
    assert!(cancelling < cancelled);
    coordinator
        .start(
            request("retry"),
            Box::new(|temporary, _, progress| {
                fs::write(temporary, b"ID3retry").unwrap();
                progress(1.0);
                Ok(())
            }),
        )
        .unwrap();
    assert!(matches!(events.recv_timeout(Duration::from_millis(100)).unwrap(), DownloadEvent::Started { .. }));
    assert!(matches!(events.recv_timeout(Duration::from_millis(100)).unwrap(), DownloadEvent::Progress { .. }));
    assert!(matches!(events.recv_timeout(Duration::from_millis(100)).unwrap(), DownloadEvent::Finalizing { .. }));
    assert!(matches!(events.recv_timeout(Duration::from_millis(100)).unwrap(), DownloadEvent::Succeeded { .. }));
    let error_coordinator = DownloadCoordinator::new();
    let error_events = error_coordinator.events().unwrap();
    error_coordinator
        .start(
            request("error"),
            Box::new(|_, _, progress| {
                progress(1.0);
                Err("controlled failure".into())
            }),
        )
        .unwrap();
    let mut saw_error = false;
    for _ in 0..10 {
        match error_events.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                let failed = matches!(event, DownloadEvent::Failed { .. });
                observed.push(event);
                if failed {
                    saw_error = true;
                    break;
                }
            }
            Err(_) => panic!("timed out waiting for failure event"),
        }
    }
    assert!(saw_error);
    assert!(observed
        .iter()
        .any(|event| matches!(event, DownloadEvent::Failed { .. })));
}

#[test]
fn downloader_markers_support_corruption_legacy_retry_and_sync_dedupe() {
    let root = fixture("markers");
    fs::write(
        root.join(".downloaded_vids"),
        "valid-id\n../unsafe\nwith space\n",
    )
    .unwrap();
    fs::write(
        root.join(".downloaded.txt"),
        "old title provider legacy-id\n",
    )
    .unwrap();
    fs::write(root.join(".sync_failed_vids"), "failed-id\tblocked\n").unwrap();
    let loaded = Markers::load(&root);
    assert!(loaded.successful.contains("valid-id") && loaded.successful.contains("legacy-id"));
    assert!(!loaded.successful.contains("../unsafe") && !loaded.successful.contains("with space"));
    assert!(loaded.failed.contains("failed-id"));
    let mut markers = Markers::default();
    let blocked = root.join("not-a-folder");
    fs::write(&blocked, b"x").unwrap();
    assert!(markers.record_success(&blocked, "retry-id").is_err());
    assert!(!markers.successful.contains("retry-id"));
    markers.record_success(&root, "retry-id").unwrap();
    assert!(markers.successful.contains("retry-id"));
    let music = root.join("music");
    let destination = root.join("playlist");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(music.join("Artist - Existing.mp3"), b"ID3existing").unwrap();
    let items = plan(
        &music,
        &destination,
        &[
            PlaylistEntry {
                id: Some("new".into()),
                title: "Artist - Existing".into(),
                channel: None,
                duration_secs: None,
                url: None,
            },
            PlaylistEntry {
                id: Some("new2".into()),
                title: "Artist - New".into(),
                channel: None,
                duration_secs: None,
                url: None,
            },
        ],
    );
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].destination.extension().and_then(|value| value.to_str()), Some("m4a"));
    let token = CancellationToken::new();
    let result = sync_sequential(&destination, &items, &token, |item, _| {
        fs::write(&item.destination, b"ID3new").unwrap();
        Ok(())
    });
    assert!(matches!(result.outcomes[0].1, ItemOutcome::Downloaded));
    assert!(Markers::load(&destination).successful.contains("new2"));
    let invalid = SyncItem {
        entry: PlaylistEntry {
            id: Some("invalid".into()),
            title: "Artist - Invalid".into(),
            channel: None,
            duration_secs: None,
            url: None,
        },
        source_override: None,
        destination: destination.join("Artist - Invalid.mp3"),
    };
    let result = sync_sequential(
        &destination,
        &[invalid.clone()],
        &CancellationToken::new(),
        |item, _| {
            fs::write(&item.destination, b"arbitrary non-audio output").unwrap();
            Ok(())
        },
    );
    assert!(matches!(result.outcomes[0].1, ItemOutcome::Failed(_)));
    assert!(!Markers::load(&destination).successful.contains("invalid"));
    let valid = SyncItem {
        entry: PlaylistEntry {
            id: Some("valid".into()),
            title: "Artist - Valid".into(),
            channel: None,
            duration_secs: None,
            url: None,
        },
        source_override: None,
        destination: destination.join("Artist - Valid.mp3"),
    };
    let result = sync_sequential(
        &destination,
        &[valid],
        &CancellationToken::new(),
        |item, _| {
            fs::write(&item.destination, b"ID3valid").unwrap();
            Ok(())
        },
    );
    assert!(matches!(result.outcomes[0].1, ItemOutcome::Downloaded));
    assert!(Markers::load(&destination).successful.contains("valid"));
}

#[test]
fn youtube_playlist_urls_are_canonical_and_reject_lookalike_hosts() {
    use crate::download::sync::playlist_url;
    assert_eq!(playlist_url("PL_abc-123"), Some("https://www.youtube.com/playlist?list=PL_abc-123".into()));
    assert_eq!(playlist_url("https://music.youtube.com/playlist?list=PL_abc-123&feature=share"), Some("https://www.youtube.com/playlist?list=PL_abc-123".into()));
    assert_eq!(playlist_url("https://www.youtube.com/watch?v=abc&list=PL_abc-123"), Some("https://www.youtube.com/playlist?list=PL_abc-123".into()));
    assert_eq!(playlist_url("https://youtube.com.evil.test/playlist?list=PL_abc-123"), None);
    assert_eq!(playlist_url("https://example.test/playlist?list=PL_abc-123"), None);
    assert_eq!(playlist_url("https://www.youtube.com/playlist?list=bad%20id"), None);
}

#[test]
fn settings_policy_filters_types_retains_malformed_input_and_recovers_atomically() {
    let root = fixture("settings");
    let config = root.join(".luno_config.json");
    fs::write(&config, r#"{"music_root":"fixture://library/basic","volume":0.75,"global_keys":"wrong","favorite_songs":["track-a",4],"unknown":"ignored"}"#).unwrap();
    let parsed = read_config(&config).unwrap();
    assert_eq!(
        parsed.music_root.as_deref(),
        Some("fixture://library/basic")
    );
    assert_eq!(parsed.volume, Some(0.75));
    assert_eq!(parsed.global_keys, None);
    assert_eq!(parsed.favorite_songs, Some(vec!["track-a".into()]));
    let original = fs::read(&config).unwrap();
    fs::write(&config, b"{malformed").unwrap();
    assert!(read_config(&config).is_err());
    assert_eq!(fs::read(&config).unwrap(), b"{malformed");
    let state = root.join("settings-v1.json");
    fs::write(&state, &original).unwrap();
    write_atomic(&state, b"{\"version\":1,\"volume\":0.5}").unwrap();
    fs::remove_file(&state).unwrap();
    assert_eq!(recover(&state).unwrap(), RecoveryResult::RestoredBackup);
    assert_eq!(fs::read(&state).unwrap(), original);
}

#[test]
fn settings_load_returns_typed_values_for_frontend_editing() {
    let root = fixture("settings-round-trip");
    let home = root.join("home");
    let library = root.join("library");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&library).unwrap();
    fs::write(
        home.join(".luno_config.json"),
        serde_json::json!({
            "music_root": library,
            "auto_discovery": true,
            "volume": 0.25,
            "sleep_timer": "15 mins",
            "global_keys": false,
            "favorite_songs": [library.join("track-a")]
        })
        .to_string(),
    )
    .unwrap();
    let result = crate::state::AppState::new(2)
        .dispatch(CommandEnvelope {
            version: 1,
            request_id: "load-settings-round-trip".into(),
            command: BackendCommand::LoadSettings {
                home: home.to_string_lossy().into_owned(),
            },
        })
        .unwrap();
    assert_eq!(result["auto_discovery"], true);
    assert_eq!(result["volume"], 0.25);
    assert_eq!(result["sleep_timer"], "15 mins");
    assert_eq!(result["global_keys"], false);
    let expected_favorite = library.join("track-a").to_string_lossy().to_lowercase();
    assert_eq!(result["favorite_songs"][0], expected_favorite);
}

#[test]
fn transfer_codec_rejects_unsafe_fields_and_preserves_order() {
    let valid = include_str!("../../contracts/library-export/v1/valid.json");
    let decoded = decode(valid).unwrap();
    assert_eq!(decoded.playlists[0].track_refs, vec!["src:one".to_string()]);
    assert!(encode(&decoded).unwrap().contains("luno.library.export"));
    let unsafe_fixture =
        include_str!("../../contracts/library-export/v1/invalid-unknown-field.json");
    assert!(decode(unsafe_fixture).is_err());
    let mut unsafe_manifest = manifest();
    unsafe_manifest.playlists[0].name = "../escape".into();
    assert!(crate::transfer::codec::validate(&unsafe_manifest).is_err());
}

#[test]
fn transfer_matching_is_additive_duration_aware_and_collision_safe() {
    let root = fixture("transfer");
    let source = root.join("source.mp3");
    fs::write(&source, b"ID3source").unwrap();
    let existing = root.join("Playlist Unsafe").join("Artist A - Track A.mp3");
    fs::create_dir_all(existing.parent().unwrap()).unwrap();
    fs::write(&existing, b"ID3existing").unwrap();
    let wanted = manifest();
    let index = ProviderIndex::new([
        IndexedTrack {
            path: source.clone(),
            metadata: TrackMetadata {
                title: "Track A".into(),
                artist: "Artist A".into(),
                duration_ms: 100_500,
                provider: Some(ProviderIdentity {
                    provider: "youtube".into(),
                    id: "provider-video-1".into(),
                }),
            },
        },
        IndexedTrack {
            path: root.join("ambiguous.mp3"),
            metadata: TrackMetadata {
                title: "Track B".into(),
                artist: "Artist B".into(),
                duration_ms: 0,
                provider: None,
            },
        },
        IndexedTrack {
            path: root.join("ambiguous-2.mp3"),
            metadata: TrackMetadata {
                title: "Track B".into(),
                artist: "Artist B".into(),
                duration_ms: 1_000,
                provider: None,
            },
        },
    ]);
    let plan = plan_import(&wanted, &index, &root);
    assert!(matches!(plan.tracks[0].match_plan, MatchPlan::Matched(_)));
    assert!(
        matches!(plan.tracks[1].match_plan, MatchPlan::Ambiguous(ref candidates) if candidates.len() == 2)
    );
    assert!(plan.no_deletions);
    let result = apply_import_plan(&plan, &HashMap::new());
    assert_eq!(result.copied.len(), 1);
    assert!(result.copied[0].to_string_lossy().contains("(1)"));
    assert!(existing.exists());
    let mismatch = TrackMetadata {
        title: "Track A".into(),
        artist: "Artist A".into(),
        duration_ms: 200_000,
        provider: None,
    };
    assert!(matches!(
        index.matching(&mismatch),
        crate::domain::identity::MatchResult::Unresolved
    ));
    let failing = ProviderIndex::new([IndexedTrack {
        path: root.join("missing.mp3"),
        metadata: TrackMetadata {
            title: "Track A".into(),
            artist: "Artist A".into(),
            duration_ms: 100_000,
            provider: None,
        },
    }]);
    let failure_plan = plan_import(
        &Manifest {
            tracks: vec![Track {
                source: None,
                ambiguity_confirmation_required: true,
                ..wanted.tracks[0].clone()
            }],
            playlists: wanted.playlists.clone(),
            ..wanted.clone()
        },
        &failing,
        &root,
    );
    let failed = apply_import_plan(&failure_plan, &HashMap::new());
    assert_eq!(failed.copied.len(), 0);
    assert!(!failed.failures.is_empty());
    assert!(existing.exists());
}

#[cfg(unix)]
#[test]
fn target_dispatch_runs_search_settings_and_fake_download_event_path() {
    let root = fixture("target-dispatch");
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let helper = fake_helper(&root);
    let state = crate::state::AppState::new(8);
    state.set_scan_roots(vec![root.0.clone()]);
    state.set_bundle_helper(Some(helper.clone()));
    let search = state
        .dispatch(CommandEnvelope {
            version: 1,
            request_id: "search".into(),
            command: BackendCommand::SearchDownloader {
                query: "artist-a".into(),
                try_soundcloud: true,
                limit: 10,
            },
        })
        .unwrap();
    // Real search runs the bundled yt-dlp with `--dump-json`; the fake helper
    // answers one fixture hit for every provider query (SoundCloud + YouTube).
    let results = search.as_array().unwrap();
    assert_eq!(results.len(), 2, "one hit per enabled provider: {results:?}");
    let providers: Vec<_> = results.iter().map(|r| r["provider"].as_str().unwrap().to_owned()).collect();
    assert_eq!(providers, ["soundcloud", "youtube"], "legacy provider order preserved");
    for result in results {
        assert_eq!(result["title"], "Fixture Track");
        assert_eq!(result["artist"], "Fixture Artist");
        // Every row carries the exact provider URL used by start_download.
        assert!(result["query"].as_str().unwrap().starts_with("https://"));
    }
    let settings = crate::contracts::types::SettingsInput {
        music_root: Some(root.to_string_lossy().into_owned()),
        auto_discovery: Some(false),
        volume: Some(0.5),
        sleep_timer: None,
        global_keys: Some(true),
        favorite_songs: vec![],
        video_mode: Some(false),
    };
    state
        .dispatch(CommandEnvelope {
            version: 1,
            request_id: "settings".into(),
            command: BackendCommand::SaveSettings {
                home: home.to_string_lossy().into_owned(),
                settings,
            },
        })
        .unwrap();
    assert!(home.join(".luno_config.json").exists());
    let destination = root.join("download.mp3");
    state
        .dispatch(CommandEnvelope {
            version: 1,
            request_id: "download".into(),
            command: BackendCommand::StartDownload {
                job_id: JobId("target-job".into()),
                destination: destination.to_string_lossy().into_owned(),
                helper: helper.to_string_lossy().into_owned(),
                source: String::new(),
            },
        })
        .unwrap();
    for _ in 0..200 {
        if state.events().iter().any(|envelope| matches!(&envelope.event, Event::JobUpdated { job } if job.state == crate::contracts::types::JobState::Succeeded)) { break; }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(destination.exists());
    assert!(state.events().iter().any(|envelope| matches!(&envelope.event, Event::JobUpdated { job } if job.id.0 == "target-job" && job.state == crate::contracts::types::JobState::Succeeded)));
}

#[cfg(unix)]
#[test]
fn playlist_sync_downloads_new_ids_once_and_persists_success_markers() {
    let root = fixture("playlist-sync-command");
    let playlist = root.join("Road trip");
    fs::create_dir_all(&playlist).unwrap();
    fs::write(root.join(".luno_playlist_urls.json"), br#"{"Road trip":"https://www.youtube.com/playlist?list=PL_fixture"}"#).unwrap();
    let helper = fake_helper(&root);
    let state = crate::state::AppState::new(8);
    state.set_scan_roots(vec![root.0.clone()]);
    state.set_bundle_helper(Some(helper));
    state.replace_library(crate::contracts::types::Generation(1), std::collections::BTreeMap::from([("Road trip".into(), vec![])]));

    let start = state.dispatch(CommandEnvelope { version: 1, request_id: "sync-1".into(), command: BackendCommand::SyncPlaylist {
        playlist_id: crate::contracts::types::PlaylistId("playlist:Road trip".into()),
    }}).unwrap();
    let first_job = start["job_id"].as_str().unwrap().to_owned();
    for _ in 0..400 {
        if state.events().iter().any(|event| matches!(&event.event, Event::JobUpdated { job } if job.id.0 == first_job && job.state == crate::contracts::types::JobState::Succeeded)) { break; }
        thread::sleep(Duration::from_millis(10));
    }
    let downloaded = playlist.join("Fixture Track.m4a");
    assert!(downloaded.is_file(), "sync should download into the physical playlist");
    assert!(state.events().iter().any(|event| matches!(&event.event, Event::JobUpdated { job } if job.id.0 == first_job && job.current_item.as_deref() == Some("Fixture Track"))), "sync should publish the song currently being downloaded");
    assert!(Markers::load(&playlist).successful.contains("fixture-video"));
    let log = state.dispatch(CommandEnvelope { version: 1, request_id: "sync-log".into(), command: BackendCommand::PlaylistSyncLog {
        playlist_id: crate::contracts::types::PlaylistId("playlist:Road trip".into()),
    }}).unwrap();
    assert_eq!(log["entries"][0]["title"], "Fixture Track");
    assert_eq!(log["entries"][0]["status"], "downloaded");
    assert!(log["entries"][0]["at_ms"].as_u64().unwrap() > 0);
    fs::remove_file(&downloaded).unwrap();

    let second = loop {
        match state.dispatch(CommandEnvelope { version: 1, request_id: "sync-2".into(), command: BackendCommand::SyncPlaylist {
            playlist_id: crate::contracts::types::PlaylistId("playlist:Road trip".into()),
        }}) {
            Ok(value) => break value,
            Err(error) if error.code == crate::contracts::types::ErrorCode::Busy => thread::sleep(Duration::from_millis(5)),
            Err(error) => panic!("second sync rejected: {error:?}"),
        }
    };
    let second_job = second["job_id"].as_str().unwrap().to_owned();
    for _ in 0..400 {
        if state.events().iter().any(|event| matches!(&event.event, Event::JobUpdated { job } if job.id.0 == second_job && job.state == crate::contracts::types::JobState::Succeeded)) { break; }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!downloaded.exists(), "a marker-recorded ID must not be redownloaded even if its local file was removed");
    assert_eq!(crate::download::sync::read_log(&playlist).unwrap().len(), 1, "skipped IDs must not create false download history");
}

#[test]
fn playlist_sync_log_keeps_song_name_date_and_failure_reason() {
    let root = fixture("playlist-sync-log");
    let entry = crate::download::sync::PlaylistEntry {
        id: Some("failed-video".into()), title: "Artist - Missing song".into(), channel: None, duration_secs: None, url: None,
    };
    crate::download::sync::append_log(&root, &entry, &crate::download::sync::ItemOutcome::Failed("provider refused this video".into())).unwrap();
    let entries = crate::download::sync::read_log(&root).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].title, entry.title);
    assert_eq!(entries[0].video_id, entry.id);
    assert_eq!(entries[0].status, "failed");
    assert_eq!(entries[0].reason.as_deref(), Some("provider refused this video"));
    assert!(entries[0].at_ms > 0);
}

#[test]
fn unavailable_playlist_entry_persists_review_metadata_and_stops_repeated_attempts() {
    let root = fixture("playlist-unavailable-state");
    let music = root.join("music");
    let playlist = root.join("playlist");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&playlist).unwrap();
    let entry = PlaylistEntry {
        id: Some("aPSOTX5AkiY".into()), title: "Boogie T - Get What You Give".into(),
        channel: Some("Boogie T - Topic".into()), duration_secs: Some(292),
        url: Some("https://www.youtube.com/watch?v=aPSOTX5AkiY".into()),
    };
    let items = plan_with_state(&music, &playlist, std::slice::from_ref(&entry), &PlaylistSyncState::default());
    assert_eq!(items.len(), 1);
    let mut sync_state = PlaylistSyncState::default();
    let mut logged = Vec::new();
    let result = sync_sequential_observed_with_state(
        &playlist, &items, &CancellationToken::new(), &mut sync_state,
        |_, _| Err("ERROR: [youtube] aPSOTX5AkiY: Video unavailable".into()),
        |item, outcome| {
            logged.push(outcome.clone());
            crate::download::sync::append_log_with_source(
                &playlist, &item.entry, item.entry.id.as_deref(), outcome,
            ).unwrap();
        },
    );
    assert_eq!(result.outcomes, vec![("aPSOTX5AkiY".into(), ItemOutcome::Unavailable("ERROR: [youtube] aPSOTX5AkiY: Video unavailable".into()))]);
    assert_eq!(logged.len(), 1);
    assert!(Markers::load(&playlist).failed.contains("aPSOTX5AkiY"));

    let restored = PlaylistSyncState::load(&playlist).unwrap();
    let issue = &restored.unresolved["aPSOTX5AkiY"];
    assert_eq!(issue.title, entry.title);
    assert_eq!(issue.channel.as_deref(), Some("Boogie T - Topic"));
    assert_eq!(issue.duration_secs, Some(292));
    assert!(issue.reason.contains("Video unavailable"));
    assert_eq!(crate::download::sync::read_log(&playlist).unwrap()[0].status, "unavailable");

    let next_items = plan_with_state(&music, &playlist, &[entry], &restored);
    assert!(next_items.is_empty(), "the persisted issue should prevent a pointless repeated extraction");
}

#[test]
fn selected_replacement_download_marks_the_original_playlist_id_and_logs_both_ids() {
    let root = fixture("playlist-replacement-source");
    let music = root.join("music");
    let playlist = root.join("playlist");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&playlist).unwrap();
    let original = PlaylistEntry {
        id: Some("aPSOTX5AkiY".into()), title: "Boogie T - Get What You Give".into(),
        channel: Some("Boogie T - Topic".into()), duration_secs: Some(292),
        url: Some("https://www.youtube.com/watch?v=aPSOTX5AkiY".into()),
    };
    let mut state = PlaylistSyncState::default();
    state.select_source("aPSOTX5AkiY", "6U7_Qy5c_uc").unwrap();
    state.save(&playlist).unwrap();
    let restored = PlaylistSyncState::load(&playlist).unwrap();
    let items = plan_with_state(&music, &playlist, std::slice::from_ref(&original), &restored);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].entry.id.as_deref(), Some("aPSOTX5AkiY"));
    assert_eq!(items[0].source_override.as_ref().map(|source| source.video_id.as_str()), Some("6U7_Qy5c_uc"));
    assert_eq!(items[0].source_override.as_ref().map(|source| source.url.as_str()), Some("https://www.youtube.com/watch?v=6U7_Qy5c_uc"));

    let result = sync_sequential_observed_with_state(
        &playlist, &items, &CancellationToken::new(), &mut state,
        |item, _| {
            assert_eq!(item.source_override.as_ref().unwrap().video_id, "6U7_Qy5c_uc");
            fs::write(&item.destination, b"ID3replacement").unwrap();
            Ok(())
        },
        |item, outcome| {
            crate::download::sync::append_log_with_source(
                &playlist, &item.entry, item.source_override.as_ref().map(|source| source.video_id.as_str()), outcome,
            ).unwrap();
        },
    );
    assert_eq!(result.outcomes, vec![("aPSOTX5AkiY".into(), ItemOutcome::Downloaded)]);
    let markers = Markers::load(&playlist);
    assert!(markers.successful.contains("aPSOTX5AkiY"));
    assert!(!markers.successful.contains("6U7_Qy5c_uc"));
    let log = crate::download::sync::read_log(&playlist).unwrap();
    assert_eq!(log[0].video_id.as_deref(), Some("aPSOTX5AkiY"));
    assert_eq!(log[0].source_video_id.as_deref(), Some("6U7_Qy5c_uc"));

    let mut attempts = 0;
    let repeated = sync_sequential_observed_with_state(
        &playlist, &items, &CancellationToken::new(), &mut state,
        |_, _| { attempts += 1; Err("must not redownload a marked original ID".into()) },
        |_, _| {},
    );
    assert_eq!(attempts, 0);
    assert_eq!(repeated.outcomes, vec![("aPSOTX5AkiY".into(), ItemOutcome::Matched)]);
    assert_eq!(crate::download::sync::read_log(&playlist).unwrap().len(), 1);
}

#[test]
fn legacy_unavailable_markers_migrate_from_log_but_success_markers_remain_authoritative() {
    let root = fixture("playlist-legacy-unavailable");
    let entry = PlaylistEntry {
        id: Some("aPSOTX5AkiY".into()), title: "Boogie T - Get What You Give".into(),
        channel: Some("Boogie T - Topic".into()), duration_secs: Some(292),
        url: Some("https://www.youtube.com/watch?v=aPSOTX5AkiY".into()),
    };
    fs::write(root.join(".sync_failed_vids"), "aPSOTX5AkiY\tVideo unavailable\n").unwrap();
    crate::download::sync::append_log(&root, &entry, &ItemOutcome::Failed("ERROR: [youtube] aPSOTX5AkiY: Video unavailable".into())).unwrap();
    let markers = Markers::load(&root);
    let mut restored = PlaylistSyncState::default();
    assert!(restored.migrate_legacy_unavailable(&root, std::slice::from_ref(&entry), &markers));
    assert_eq!(restored.unresolved["aPSOTX5AkiY"].channel.as_deref(), Some("Boogie T - Topic"));

    let mut successful = markers;
    successful.record_success(&root, "aPSOTX5AkiY").unwrap();
    let mut authoritative = PlaylistSyncState::default();
    assert!(!authoritative.migrate_legacy_unavailable(&root, &[entry], &successful));
    assert!(authoritative.unresolved.is_empty(), "original-ID success must override a stale failure history");
}

#[cfg(unix)]
#[test]
fn playlist_sync_cancellation_kills_enumerator_and_releases_download_admission() {
    let root = fixture("playlist-sync-cancel");
    let playlist = root.join("Road trip");
    fs::create_dir_all(&playlist).unwrap();
    fs::write(root.join(".luno_playlist_urls.json"), br#"{"Road trip":"https://www.youtube.com/playlist?list=PL_fixture"}"#).unwrap();
    let helper = fake_helper(&root);
    let pid_file = root.join("enumerator.pid");
    let ytdlp = root.join("yt-dlp");
    fs::write(&ytdlp, format!("#!/bin/sh\nprintf '%s' \"$$\" > '{}'\nsleep 30\n", pid_file.display())).unwrap();
    let mut permissions = fs::metadata(&ytdlp).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&ytdlp, permissions).unwrap();

    let state = crate::state::AppState::new(8);
    state.set_scan_roots(vec![root.0.clone()]);
    state.set_bundle_helper(Some(helper.clone()));
    state.replace_library(crate::contracts::types::Generation(1), std::collections::BTreeMap::from([("Road trip".into(), vec![])]));
    let started = state.dispatch(CommandEnvelope { version: 1, request_id: "sync-cancel".into(), command: BackendCommand::SyncPlaylist {
        playlist_id: crate::contracts::types::PlaylistId("playlist:Road trip".into()),
    }}).unwrap();
    let job_id = started["job_id"].as_str().unwrap().to_owned();
    // The full Cargo suite runs many process-heavy tests in parallel. Give the
    // worker a bounded but generous start window so this asserts behavior, not
    // host scheduler speed.
    for _ in 0..1_000 {
        if pid_file.is_file() { break; }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(pid_file.is_file(), "playlist enumerator should have started");

    let blocked = state.dispatch(CommandEnvelope { version: 1, request_id: "blocked-download".into(), command: BackendCommand::StartDownload {
        job_id: JobId("blocked-download".into()), destination: root.join("blocked.m4a").to_string_lossy().into_owned(),
        helper: helper.to_string_lossy().into_owned(), source: String::new(),
    }}).unwrap_err();
    assert_eq!(blocked.code, crate::contracts::types::ErrorCode::Busy);
    state.dispatch(CommandEnvelope { version: 1, request_id: "cancel-sync".into(), command: BackendCommand::CancelJob { job_id: JobId(job_id.clone()) }}).unwrap();
    for _ in 0..1_000 {
        if state.events().iter().any(|event| matches!(&event.event, Event::JobUpdated { job } if job.id.0 == job_id && job.state == crate::contracts::types::JobState::Cancelled)) { break; }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(state.events().iter().any(|event| matches!(&event.event, Event::JobUpdated { job } if job.id.0 == job_id && job.state == crate::contracts::types::JobState::Cancelled)));
    let pid: i32 = fs::read_to_string(&pid_file).unwrap().parse().unwrap();
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "cancelled enumerator process should no longer exist");

    let destination = root.join("after-cancel.m4a");
    let accepted = state.dispatch(CommandEnvelope { version: 1, request_id: "after-cancel".into(), command: BackendCommand::StartDownload {
        job_id: JobId("after-cancel".into()), destination: destination.to_string_lossy().into_owned(),
        helper: helper.to_string_lossy().into_owned(), source: String::new(),
    }});
    assert!(accepted.is_ok(), "playlist sync must release shared download admission after cancellation: {accepted:?}");
}

#[test]
fn target_dispatch_validates_direct_csv_queue_and_existing_root() {
    let root = fixture("target-policy-dispatch");
    let state = crate::state::AppState::new(8);
    state.set_scan_roots(vec![root.0.clone()]);
    let dispatch = |command| state.dispatch(CommandEnvelope { version: 1, request_id: "gate4-policy".into(), command });
    assert!(dispatch(BackendCommand::ValidateDirectUrl { url: "https://example.test/audio".into(), destination: root.to_string_lossy().into_owned() }).is_ok());
    assert!(dispatch(BackendCommand::ValidateDirectUrl { url: "file:///unsafe".into(), destination: root.to_string_lossy().into_owned() }).is_err());
    let csv = dispatch(BackendCommand::PreviewCsv { csv: "Track Name,Artist Name(s)\nSong,Artist\n".into() }).unwrap();
    assert_eq!(csv["rows"][0]["title"], "Song");
    let queued = dispatch(BackendCommand::QueueUrls { urls: vec!["https://example.test/a".into(), "bad".into()] }).unwrap();
    assert_eq!(queued["accepted"].as_array().unwrap().len(), 1);
    assert_eq!(queued["rejected"].as_array().unwrap().len(), 1);
    let selected = dispatch(BackendCommand::SelectLibraryRoot { root: root.to_string_lossy().into_owned() }).unwrap();
    assert_eq!(selected["root"], std::fs::canonicalize(&root.0).unwrap().to_string_lossy().as_ref());
}

#[test]
fn transfer_export_is_versioned_path_free_and_preserves_snapshot_order() {
    let root = fixture("transfer-export");
    let state = crate::state::AppState::new(4);
    state.set_scan_roots(vec![root.0.clone()]);
    let manifest = state.dispatch(CommandEnvelope { version: 1, request_id: "export".into(), command: BackendCommand::TransferExport }).unwrap();
    let json = manifest["manifest"].as_str().unwrap();
    assert!(json.contains("luno.library.export"));
    assert!(!json.contains(root.to_string_lossy().as_ref()));
}
