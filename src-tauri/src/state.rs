use crate::contracts::{
    events::{
        Command, CommandEnvelope, Event, EventEnvelope, RepeatMode as ContractRepeatMode, RootStatus, ShutdownEnvelope, ShutdownState,
    },
    types::{
        ContractError, DiscoverSnapshot, DownloadPhase, ErrorCode, Generation, JobId, LibrarySnapshot, Playlist,
        PlaylistId, PreviewSeed, PreviewSnapshot, SaveState, SaveStateKind, SessionId, Track,
        TrackId, CONTRACT_VERSION, MAX_PREVIEW_SEEDS, MAX_SAFE_EVENT_ID,
    },
};
use crate::domain::identity::TrackMetadata;
#[cfg(target_os = "macos")]
use crate::media_keys::MediaKey;
use crate::discovery::service::{
    DiscoveryError, DiscoveryResult, DiscoveryService, Recommendation,
};
use crate::download::coordinator::DownloadCoordinator;
use crate::download::markers::Markers;
use crate::download::promotion::{promote_output, CollisionPolicy, PromotionError, Validation};
use crate::library::artwork::TrackArtwork;
use crate::library::favorites::Favorites;
use crate::library::scanner::{OsFileSystem, ScanOutcome};
use crate::preview::assets::PreviewAssetStore;
use crate::preview::discover_mode::{DiscoverAction, DiscoverMode, ReadyAsset};
use crate::preview::save::{remux_and_promote, PlaylistDestination};
use crate::preview::session::{Handoff, PreviewSession, SessionAction};
use crate::runtime::preview_worker::{prepare_preview_with_progress, PreviewCompletion};
#[cfg(target_os = "macos")]
use crate::playback::{
    backend::{AudioBackend, BackendError},
};
#[cfg(all(target_os = "macos", test))]
use crate::playback::backend::FakeBackend;
#[cfg(all(target_os = "macos", not(test)))]
use crate::playback::backend_adapter::CpalSymphoniaBackend;
use crate::process::supervisor::{ProcessRegistry, Supervisor};
use crate::playback::{clock::PlaybackClock, queue::QueueState, recovery::FailureRecovery};
use crate::runtime::task_registry::{
    CancellationToken, ShutdownReport, SpawnError, TaskHandle, TaskRegistry, TaskSpec,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Arc as StdArc;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    NotRun,
    Skipped {
        reason: String,
    },
    Completed {
        warnings: usize,
        snapshot_created: bool,
        removed_previews: usize,
    },
    Failed {
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    /// Minimal valid 44.1 kHz stereo PCM WAV (mirrors the gate3 fixture).
    fn write_wav_fixture(path: &std::path::Path) -> std::path::PathBuf {
        let sample_rate = 44_100u32;
        let channels = 2u16;
        let bits = 16u16;
        // 30 s of audio: a fixture that ends too quickly lets a delayed
        // heartbeat auto-advance the queue mid-test.
        let frames = 44_100u32 * 30;
        let data_size = frames * channels as u32 * (bits as u32 / 8);
        let mut wav = Vec::with_capacity(44 + data_size as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes());
        wav.extend_from_slice(&(channels * 2).to_le_bytes());
        wav.extend_from_slice(&bits.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        for frame in 0..frames {
            let sample = (((frame * 440) % sample_rate) as i16).to_le_bytes();
            wav.extend_from_slice(&sample);
            wav.extend_from_slice(&sample);
        }
        std::fs::write(path, wav).unwrap();
        path.to_path_buf()
    }
    use super::*;

    /// Ownership guard truth table for staging-worker completion (2026-08-24
    /// night): a finishing worker may publish only into an empty slot, its
    /// OWN placeholder, or a slot nothing newer claimed. Overwriting a newer
    /// claim for ANOTHER track stranded the UI on stale results and made real
    /// ones (e.g. Unavailable) vanish behind the staleness filter.
    #[test]
    fn completion_may_publish_ownership_rules() {
        fn video(track: &str, generation: u64) -> crate::contracts::events::VideoState {
            crate::contracts::events::VideoState {
                track_id: Some(TrackId::new(track).expect("id")),
                source_url: None,
                staged_path: None,
                status: crate::contracts::events::VideoStatus::Loading,
                position_ms: 0,
                download_progress: None,
                revision: None,
                fullscreen: false,
                generation: Generation(generation),
            }
        }
        let worker_track = TrackId::new("t-worker").expect("id");
        // Empty slot: always publish.
        assert!(completion_may_publish(None, &worker_track, 7));
        // Own placeholder (any generation): publish.
        assert!(completion_may_publish(Some(&video("t-worker", 3)), &worker_track, 7));
        // Another track with an OLDER generation: publish (we are newer).
        assert!(completion_may_publish(Some(&video("t-other", 5)), &worker_track, 7));
        // Another track with a NEWER generation (a newer claim): DO NOT publish.
        assert!(!completion_may_publish(Some(&video("t-other", 9)), &worker_track, 7));
    }

    #[test]
    fn heal_track_id_recovers_collision_flipped_ids() {
        fn map(entries: &[&str]) -> std::collections::BTreeMap<TrackId, PathBuf> {
            entries.iter().map(|id| (TrackId::new(*id).expect("id"), PathBuf::from(format!("/lib/{id}")))).collect()
        }
        let requested = TrackId::new("metadata:magnatron-starfighter:0").expect("id");
        // Canonical copy won a rescan: the exact base now exists.
        let library = map(&["metadata:magnatron-starfighter:0", "metadata:other-song:0"]);
        assert_eq!(heal_track_id(&library, &requested), Some(requested.clone()));
        // The clicked row GAINED a discriminator (one suffixed sibling).
        let library = map(&["metadata:magnatron-starfighter:0#1a2b3c4d", "metadata:other-song:0"]);
        assert_eq!(heal_track_id(&library, &requested), Some(TrackId::new("metadata:magnatron-starfighter:0#1a2b3c4d").expect("id")));
        // A stale SUFFIX resolves back to the canonical copy.
        let stale = TrackId::new("metadata:magnatron-starfighter:0#deadbeef").expect("id");
        let library = map(&["metadata:magnatron-starfighter:0", "metadata:other-song:0"]);
        assert_eq!(heal_track_id(&library, &stale), Some(TrackId::new("metadata:magnatron-starfighter:0").expect("id")));
        // AMBIGUOUS siblings must still fail — never guess between copies.
        let library = map(&["metadata:magnatron-starfighter:0#1a2b3c4d", "metadata:magnatron-starfighter:0#deadbeef"]);
        assert_eq!(heal_track_id(&library, &requested), None);
        // No relatives at all.
        let library = map(&["metadata:other-song:0"]);
        assert_eq!(heal_track_id(&library, &requested), None);
    }

    /// RED repro (2026-08-23): search shows a song row, clicking it errors
    /// "track is not available locally". Track IDs are filename-derived; when
    /// two files share a stem, whichever the scan visits FIRST keeps the plain
    /// id and the other gets `#hash`. Unsorted orders by mtime, so touching or
    /// re-downloading one copy flips plain-id ownership on the next scan —
    /// every previously-rendered row/queue entry silently stops resolving.
    #[test]
    fn duplicate_stem_ids_stay_stable_when_no_files_change() {
        use std::fs;
        let state = AppState::new(1);
        let root = std::env::temp_dir().join(format!("luno-dup-stem-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Unsorted")).unwrap();
        fs::write(root.join("Unsorted/Dup Artist - Same Song.mp3"), b"a").unwrap();
        fs::write(root.join("Unsorted/Dup Artist - Same Song.m4a"), b"b").unwrap();
        state.set_scan_roots(vec![root.clone()]);

        // Scan 1: capture both derived ids.
        let generation = state.scan_async().expect("scan accepted");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while state.library().tracks.len() < 2 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let first: Vec<String> = state.library().tracks.iter().map(|t| t.id.0.clone()).collect();
        assert_eq!(first.len(), 2, "both files must appear after scan 1");
        assert!(first.iter().any(|id| id.contains('#')), "collision must be discriminated");

        // No file added/removed — only mtimes change (touch one copy).
        let mp3 = root.join("Unsorted/Dup Artist - Same Song.mp3");
        let m4a = root.join("Unsorted/Dup Artist - Same Song.m4a");
        let older = fs::metadata(&mp3).unwrap().modified().unwrap();
        fs::write(&m4a, b"b2").unwrap(); // newer mtime for the m4a

        let _ = state.scan_async().expect("scan 2 accepted");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let second: Vec<String> = state.library().tracks.iter().map(|t| t.id.0.clone()).collect();
            if second.len() == 2 && second != first && std::time::Instant::now() < deadline {
                break;
            }
            if second.len() != 2 && std::time::Instant::now() >= deadline {
                panic!("library lost tracks after rescan");
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let second: Vec<String> = state.library().tracks.iter().map(|t| t.id.0.clone()).collect();
        assert_eq!(second.len(), 2, "both files must still exist after rescan");
        assert_eq!(
            first,
            second,
            "track ids must be stable across rescans when no files are added or removed"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// Video mode must survive a save→load round trip and the backend-side
    /// toggle must persist immediately.
    #[test]
    fn video_mode_round_trips_through_settings() {
        let state = AppState::new(1);
        let home = std::env::temp_dir().join(format!("luno-video-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();

        // Backend-side toggle persists without any frontend round-trip.
        state.set_video_mode(true).expect("toggle accepted");
        let raw = std::fs::read_to_string(home.join(".luno_config.json"));
        // No settings home captured yet -> nothing written; capture it first.
        assert!(raw.is_err(), "nothing should persist before load_config");
        *state.settings_home.lock().unwrap_or_else(|e| e.into_inner()) = Some(home.clone());
        state.set_video_mode(false).expect("toggle accepted");
        let raw = std::fs::read_to_string(home.join(".luno_config.json")).unwrap();
        assert!(raw.contains("\"video_mode\": false"), "disabled mode persisted: {raw}");

        // Legacy reader accepts an existing file that carries video_mode.
        std::fs::write(
            home.join(".luno_config.json"),
            r#"{"music_root": "/", "video_mode": true}"#,
        )
        .unwrap();
        let config = crate::persistence::legacy::read_config(&home.join(".luno_config.json")).unwrap();
        assert_eq!(config.video_mode, Some(true));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn scan_with_all_roots_unreadable_preserves_library() {
        use crate::library::scanner::{RootStatus as ScanRootStatus, ScanError, ScanOutcome};
        use std::collections::BTreeMap;
        use std::sync::Mutex;

        let state = AppState::new(1);
        // Seed a non-empty library the way a successful scan would.
        let mut playlists = BTreeMap::new();
        playlists.insert(
            "Unsorted".to_string(),
            vec![PathBuf::from("/music/Fixture Artist - Fixture Song 1.m4a")],
        );
        state.replace_library(Generation(1), playlists);
        assert_eq!(state.library().tracks.len(), 1);

        let delivered: StdArc<Mutex<Vec<String>>> = StdArc::new(Mutex::new(Vec::new()));
        let sink_delivered = delivered.clone();
        state.set_event_sink(StdArc::new(move |event| {
            if matches!(event.event, Event::Error { .. }) {
                sink_delivered.lock().unwrap_or_else(|e| e.into_inner()).push(format!("{event:?}"));
            }
            Ok(())
        }));

        // Every root unreadable -> the previous library must survive and an
        // explanatory error must be published instead of an empty snapshot.
        let generation = state
            .scan_async_with(|_, token| ScanOutcome {
                playlists: BTreeMap::new(),
                roots: vec![(PathBuf::from("/gone"), ScanRootStatus::Missing)],
                errors: Vec::<ScanError>::new(),
            })
            .expect("scan accepted");
        let _ = generation;
        for _ in 0..100 {
            if !delivered.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(
            state.library().tracks.len(),
            1,
            "library must not be wiped when all roots are unreadable"
        );
        assert!(
            delivered
                .lock()
                .unwrap()
                .iter()
                .any(|message| message.contains("keeping previous library")),
            "expected a 'keeping previous library' error event"
        );
    }

    #[test]
    fn migration_status_is_sanitized_diagnostic_data() {
        let status = MigrationStatus::Completed {
            warnings: 1,
            snapshot_created: true,
            removed_previews: 2,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("snapshot_created"));
        assert!(!json.contains("/"));
    }

    fn unique_home(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("luno-{label}-{}-{nonce}", std::process::id()))
    }

    fn default_envelope() -> CommandEnvelope {
        CommandEnvelope { version: 1, request_id: "boot-test".into(), command: Command::GetLibrary }
    }

    #[test]
    fn load_settings_resolves_relative_music_root_against_home() {
        let home = unique_home("relroot");
        let music = home.join("Music");
        std::fs::create_dir_all(&music).unwrap();
        std::fs::write(home.join(".luno_config.json"), r#"{"music_root":"Music"}"#).unwrap();
        let state = AppState::new(1);
        let mut envelope = default_envelope();
        envelope.command = Command::LoadSettings { home: home.to_string_lossy().into_owned() };
        let value = state.dispatch(envelope.into()).expect("relative root should resolve");
        let expected = music.display().to_string();
        assert_eq!(value.get("music_root").and_then(serde_json::Value::as_str), Some(expected.as_str()));
        assert_eq!(state.scan_roots(), vec![music]);
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn load_settings_rejects_missing_relative_music_root() {
        let home = unique_home("relroot-missing");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".luno_config.json"), r#"{"music_root":"MissingFolder"}"#).unwrap();
        let state = AppState::new(1);
        let mut envelope = default_envelope();
        envelope.command = Command::LoadSettings { home: home.to_string_lossy().into_owned() };
        let error = state.dispatch(envelope.into()).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        std::fs::remove_dir_all(&home).ok();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn load_settings_seeds_persisted_volume_into_engine_state() {
        let home = unique_home("volseed");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".luno_config.json"), r#"{"volume":0.35}"#).unwrap();
        let state = AppState::new(1);
        let mut envelope = default_envelope();
        envelope.command = Command::LoadSettings { home: home.to_string_lossy().into_owned() };
        let value = state.dispatch(envelope.into()).expect("persisted volume should load");
        assert_eq!(value.get("volume").and_then(serde_json::Value::as_f64), Some(0.35));
        let stored = *state.volume.lock().unwrap_or_else(|e| e.into_inner());
        assert!(
            (stored - 0.35).abs() < 1e-6,
            "persisted volume must seed the engine so the first track plays at the saved level, got {stored}"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn resolve_runtime_home_returns_the_home_or_reports_unavailable() {
        let state = AppState::new(1);
        let mut envelope = default_envelope();
        envelope.command = Command::ResolveRuntimeHome;
        match state.dispatch(envelope.into()) {
            Ok(value) => {
                let home = value.get("home").and_then(serde_json::Value::as_str).unwrap();
                assert_eq!(home, std::env::var_os("HOME").unwrap().to_string_lossy());
            }
            Err(error) => assert_eq!(error.code, ErrorCode::InvalidRequest),
        }
    }

    #[test]
    fn load_settings_rejects_non_directory_home() {
        let home = unique_home("relroot-file");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".luno_config.json"), r#"{"music_root":"Music"}"#).unwrap();
        let state = AppState::new(1);
        for envelope in [
            CommandEnvelope { version: 1, request_id: "boot-file".into(), command: Command::LoadSettings { home: home.join(".luno_config.json").to_string_lossy().into_owned() } },
            CommandEnvelope { version: 1, request_id: "boot-relative".into(), command: Command::LoadSettings { home: "relative-home".into() } },
        ] {
            let error = state.dispatch(envelope).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidRequest);
        }
        std::fs::remove_dir_all(&home).ok();
    }

    fn fixture_helper(home: &Path, name: &str, body: &str) -> PathBuf {
        let path = home.join(name);
        std::fs::write(&path, body).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o755);
        }
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[test]
    fn start_download_creates_missing_destination_inside_root_and_passes_source() {
        let root = unique_home("dl-create");
        std::fs::create_dir_all(&root).unwrap();
        let helper = fixture_helper(
            &root,
            "capture-helper.sh",
            "#!/bin/sh\nprintf 'ID3\\003\\000\\000\\000' > \"$1\"\nprintf '%s' \"${2:-none}\" > \"$HOME/captured.source\"\n",
        );
        let state = AppState::new(2);
        state.set_scan_roots(vec![root.clone()]);
        let destination = root.join("Unsorted").join("job-1.mp3");
        let mut envelope = default_envelope();
        envelope.request_id = "dl-create".into();
        envelope.command = Command::StartDownload {
            job_id: JobId("job-1".into()),
            destination: destination.to_string_lossy().into_owned(),
            helper: helper.to_string_lossy().into_owned(),
            source: "scsearch1:fixture query".into(),
        };
        assert!(state.dispatch(envelope.into()).is_ok());
        assert!(root.join("Unsorted").is_dir());
        let mut succeeded = false;
        for _ in 0..300 {
            if state.events().iter().any(|envelope| matches!(&envelope.event, Event::JobUpdated { job } if job.id.0 == "job-1" && job.state == crate::contracts::types::JobState::Succeeded)) {
                succeeded = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(succeeded);
        assert!(destination.exists());
        let captured = std::fs::read_to_string(root.join(".luno-gate4-worker-home").join("captured.source")).unwrap();
        assert_eq!(captured, "scsearch1:fixture query");
        let retry = state.dispatch(default_envelope_retry("job-1")).unwrap();
        assert!(retry.get("job_id").is_some());
        std::fs::remove_dir_all(&root).ok();
    }

    fn default_envelope_retry(job_id: &str) -> CommandEnvelope {
        CommandEnvelope { version: 1, request_id: "dl-retry".into(), command: Command::RetryDownload { job_id: JobId(job_id.into()) } }
    }

    #[test]
    fn start_download_rejects_missing_parent_outside_root() {
        let root = unique_home("dl-escape");
        std::fs::create_dir_all(&root).unwrap();
        let helper = fixture_helper(&root, "simple-helper.sh", "#!/bin/sh\nprintf 'ID3' > \"$1\"\n");
        let state = AppState::new(2);
        state.set_scan_roots(vec![root.clone()]);
        let escape = std::env::temp_dir().join(format!("luno-escape-{}", std::process::id())).join("job.mp3");
        let mut envelope = default_envelope();
        envelope.request_id = "dl-escape".into();
        envelope.command = Command::StartDownload {
            job_id: JobId("job-escape".into()),
            destination: escape.to_string_lossy().into_owned(),
            helper: helper.to_string_lossy().into_owned(),
            source: "https://example.test/a".into(),
        };
        let error = state.dispatch(envelope.into()).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn bundle_helper_path_reports_configured_helper_or_unavailable() {
        let state = AppState::new(1);
        let mut envelope = default_envelope();
        envelope.command = Command::BundleHelperPath;
        let error = state.dispatch(envelope.into()).unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        let home = unique_home("bundle-helper");
        std::fs::create_dir_all(&home).unwrap();
        let helper = fixture_helper(&home, "luno-download-helper", "#!/bin/sh\nexit 0\n");
        state.set_bundle_helper(Some(helper.clone()));
        let mut envelope = default_envelope();
        envelope.command = Command::BundleHelperPath;
        let value = state.dispatch(envelope.into()).unwrap();
        assert_eq!(value.get("helper").and_then(serde_json::Value::as_str), Some(helper.to_string_lossy().as_ref()));
        std::fs::remove_dir_all(&home).ok();
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn playback_commands_report_explicit_backend_unavailability() {
        let state = AppState::new(1);
        let error = state
            .dispatch(CommandEnvelope {
                version: 1,
                request_id: "play-test".into(),
                command: Command::Play {
                    track_id: TrackId("track".into()),
                },
            })
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert!(!error.retryable);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn playback_commands_report_missing_track_and_unloaded_state() {
        let state = AppState::new(1);
        let play = state
            .dispatch(CommandEnvelope {
                version: 1,
                request_id: "play-missing".into(),
                command: Command::Play {
                    track_id: TrackId("missing".into()),
                },
            })
            .unwrap_err();
        assert_eq!(play.code, ErrorCode::NotFound);
        let pause = state
            .dispatch(CommandEnvelope {
                version: 1,
                request_id: "pause-empty".into(),
                command: Command::Pause,
            })
            .unwrap_err();
        assert_eq!(pause.code, ErrorCode::NotFound);
    }

    #[test]
    fn provider_metadata_is_used_by_scan_publication_path() {
        let state = AppState::new(1);
        let path = PathBuf::from("/music/song.mp3");
        state.set_provider_metadata(
            path.clone(),
            TrackMetadata {
                title: "Provider title".into(),
                artist: "Provider artist".into(),
                duration_ms: 1234,
                provider: Some(crate::domain::identity::ProviderIdentity {
                    provider: "youtube".into(),
                    id: "abc".into(),
                }),
            },
        );

        let snapshot = state.replace_library(
            Generation(1),
            BTreeMap::from([(String::from("Unsorted"), vec![path])]),
        );

        assert_eq!(snapshot.tracks[0].title, "Provider title");
        assert_eq!(
            snapshot.tracks[0]
                .provider
                .as_ref()
                .map(|provider| provider.id.as_str()),
            Some("abc")
        );
    }

    #[test]
    fn same_stem_files_in_different_folders_get_distinct_track_ids() {
        let state = AppState::new(1);
        // No " - " separator and no tags: both files derive the identical
        // metadata id. They must still stay distinct, playable tracks.
        let unsorted = PathBuf::from("/music/Unsorted/song.m4a");
        let favorites = PathBuf::from("/music/Favorites/song.m4a");

        let snapshot = state.replace_library(
            Generation(1),
            BTreeMap::from([
                (String::from("Unsorted"), vec![unsorted.clone()]),
                (String::from("Favorites"), vec![favorites.clone()]),
            ]),
        );

        assert_eq!(snapshot.tracks.len(), 2);
        let ids: Vec<_> = snapshot.tracks.iter().map(|track| track.id.0.clone()).collect();
        assert_ne!(ids[0], ids[1], "colliding metadata ids must be discriminated");
        assert!(ids.iter().all(|id| id.starts_with("metadata:")));
        for track in &snapshot.tracks {
            assert_eq!(track.artist, None);
            let resolved = state.track_paths.lock().unwrap_or_else(|e| e.into_inner()).get(&track.id).cloned();
            assert_eq!(resolved.is_some(), true);
        }
        // Each id resolves to its OWN file.
        let first = state.track_paths.lock().unwrap_or_else(|e| e.into_inner()).get(&snapshot.tracks[0].id).cloned().unwrap();
        let second = state.track_paths.lock().unwrap_or_else(|e| e.into_inner()).get(&snapshot.tracks[1].id).cloned().unwrap();
        assert_ne!(first, second);
    }

    /// RED repro (2026-08-25): "skipped ahead with Shuffle on, then couldn't
    /// go back a couple songs." next/previous used to re-enter
    /// play_with_context, which under shuffle RE-SHUFFLED the whole queue
    /// current-first — so the first Previous landed at index 0 and every
    /// further press was stuck, while the shuffle order mutated on each skip.
    /// Skips must walk the EXISTING shuffled order without rebuilding it.
    #[cfg(target_os = "macos")]
    #[test]
    fn previous_walks_back_through_shuffled_queue_without_rebuilding_it() {
        let state = AppState::new(8);
        // MUTE THE TEST HARNESS: this test loads REAL decodable audio through
        // the REAL backend, which would otherwise blast tones through the
        // machine's speakers at the seeded volume while `cargo test` runs.
        // Zero volume keeps the full production path intact and silent.
        *state.volume.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
        let dir = std::env::temp_dir().join(format!("luno-shuffle-back-{}", std::process::id()));
        let playlist_dir = dir.join("Unsorted");
        std::fs::create_dir_all(&playlist_dir).unwrap();
        // REAL decodable PCM WAVs: undecodable fixtures would trip the
        // failure-recovery auto-skip and race the index assertions.
        let mut paths = Vec::new();
        for name in ["Artist - Song A.wav", "Artist - Song B.wav", "Artist - Song C.wav", "Artist - Song D.wav", "Artist - Song E.wav"] {
            paths.push(write_wav_fixture(&playlist_dir.join(name)));
        }
        let snapshot = state.replace_library(Generation(1), BTreeMap::from([(String::from("Unsorted"), paths)]));
        let queue: Vec<TrackId> = snapshot.tracks.iter().map(|t| t.id.clone()).collect();
        let wait_for_selected_track = || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                let selected = state.playback.lock().unwrap_or_else(|e| e.into_inner()).current().cloned();
                let started = state.current_track.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if selected.is_some() && selected == started { return; }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };
        // Play the third track with shuffle OFF so the physical order is known.
        state.dispatch(CommandEnvelope {
            version: CONTRACT_VERSION, request_id: "play".into(),
            command: Command::PlayWithContext { track_id: queue[2].clone(), context: queue.clone() },
        }).expect("initial play");
        wait_for_selected_track();
        assert_eq!(state.playback.lock().unwrap_or_else(|e| e.into_inner()).current_index(), Some(2));
        // Shuffle ON: current-first, rest randomized. Capture the order.
        state.playback.lock().unwrap_or_else(|e| e.into_inner()).toggle_shuffle();
        let shuffled = state.playback.lock().unwrap_or_else(|e| e.into_inner()).queue().to_vec();
        assert_eq!(shuffled[0], queue[2], "current stays first");
        // Skip ahead twice (as the user did).
        for i in 0..2 {
            state.dispatch(CommandEnvelope {
                version: CONTRACT_VERSION, request_id: format!("next-{i}").into(),
                command: Command::Next,
            }).expect("next");
            wait_for_selected_track();
        }
        let after_skips = state.playback.lock().unwrap_or_else(|e| e.into_inner()).queue().to_vec();
        assert_eq!(after_skips, shuffled, "skips must NOT rebuild/re-shuffle the queue");
        let current_after_skips = state.playback.lock().unwrap_or_else(|e| e.into_inner()).current_index().unwrap();
        assert_eq!(current_after_skips, 2, "started at 0, two Next presses");
        // Go BACK a couple songs — must walk the same list, one entry per press.
        state.dispatch(CommandEnvelope {
            version: CONTRACT_VERSION, request_id: "prev1".into(),
            command: Command::Previous,
        }).expect("previous #1");
        wait_for_selected_track();
        assert_eq!(state.playback.lock().unwrap_or_else(|e| e.into_inner()).current_index(), Some(1), "first Previous moves back one");
        state.dispatch(CommandEnvelope {
            version: CONTRACT_VERSION, request_id: "prev2".into(),
            command: Command::Previous,
        }).expect("previous #2");
        wait_for_selected_track();
        assert_eq!(state.playback.lock().unwrap_or_else(|e| e.into_inner()).current_index(), Some(0), "second Previous moves back again");
        assert_eq!(*state.playback.lock().unwrap_or_else(|e| e.into_inner()).queue(), shuffled, "queue still untouched after going back");
        // And toggling shuffle OFF restores the TRUE original playlist order.
        state.playback.lock().unwrap_or_else(|e| e.into_inner()).toggle_shuffle();
        assert_eq!(state.playback.lock().unwrap_or_else(|e| e.into_inner()).queue(), &queue);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RED repro (2026-08-25 evening, revised same day): the ORIGINAL bug was
    /// an UNCANCELLABLE `.lock()` per stage request — skip-storms + the 2 s
    /// song-page poll piled blocked threads into the 32-slot registry until
    /// playback starved. The first fix (defer-on-busy) over-corrected: a
    /// deferred current track had no guaranteed retrier and videos took
    /// minutes. FINAL CONTRACT: repeated requests for one track collapse to
    /// a SINGLE cancellable waiter (inflight dedupe); the waiter polls the
    /// store with cancel checkpoints and COMPLETES once it frees.
    #[test]
    fn stage_requests_never_pile_up_behind_a_busy_store() {
        let state = AppState::new(32);
        let video_root = std::env::temp_dir().join(format!("luno-video-pileup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&video_root);
        state.set_video_runtime(video_root.clone(), Some(PathBuf::from("/nonexistent/yt-dlp")), None);
        // Index-cached fixture so completion after the store frees needs NO
        // network (nonexistent yt-dlp proves it).
        let stager = crate::video::staging::VideoStage::at(&video_root, None).expect("stager");
        stager.record_index("artistsong", "abc123");
        let cached = stager.external_destination(crate::video::staging::Slot::Current, "abc123").unwrap();
        std::fs::write(&cached, b"fake mp4 bytes").unwrap();

        *state.video_mode_enabled.lock().unwrap_or_else(|e| e.into_inner()) = true;
        let music_dir = std::env::temp_dir().join(format!("luno-video-pileup-lib-{}", std::process::id()));
        let playlist_dir = music_dir.join("Unsorted");
        std::fs::create_dir_all(&playlist_dir).unwrap();
        let track_path = playlist_dir.join("Artist - Song.wav");
        write_wav_fixture(&track_path);
        let snapshot = state.replace_library(Generation(1), BTreeMap::from([(String::from("Unsorted"), vec![track_path])]));
        let id = snapshot.tracks[0].id.clone();
        state.set_playing(id.clone(), vec![id.clone()]);

        // Hold the store exactly like a slow download would…
        let guard = state.video_stager.lock().unwrap_or_else(|e| e.into_inner());
        // …then hammer it the way skip-storms + the 2 s poll do. EVERY
        // request must coalesce into the single waiting worker.
        for _ in 0..25 {
            state.video_stage_track(id.clone()).expect("busy store must not error");
        }
        let active_while_held = state.tasks.active_count();
        assert!(active_while_held <= 2, "requests piled up: {active_while_held} active tasks");
        assert!(state.video_staging_inflight.lock().unwrap_or_else(|e| e.into_inner()).contains(&id), "the single waiter keeps its inflight marker");
        // Release the store: the waiter must proceed to Ready on its own —
        // no second kick, no poll required.
        drop(guard);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut terminal = String::from("timeout");
        while std::time::Instant::now() < deadline {
            for envelope in state.pending_events() {
                if let crate::contracts::events::Event::VideoState { state: video } = &envelope.event {
                    if video.track_id.as_ref() == Some(&id) { terminal = format!("{:?}", video.status); }
                }
            }
            if terminal.contains("Ready") || terminal.contains("Unavailable") { break; }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(terminal.contains("Ready"), "waiter must complete once the store frees, got {terminal}");
        let _ = std::fs::remove_dir_all(&video_root);
        let _ = std::fs::remove_dir_all(&music_dir);
    }

    /// The pressure gate: near the registry cap, staging defers instead of
    /// spawning so playback-load/heartbeat always have slots.
    #[test]
    fn stage_defers_when_task_registry_is_under_pressure() {
        let state = AppState::new(32);
        let video_root = std::env::temp_dir().join(format!("luno-video-pressure-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&video_root);
        state.set_video_runtime(video_root.clone(), Some(PathBuf::from("/nonexistent/yt-dlp")), None);
        *state.video_mode_enabled.lock().unwrap_or_else(|e| e.into_inner()) = true;

        let music_dir = std::env::temp_dir().join(format!("luno-video-pressure-lib-{}", std::process::id()));
        let playlist_dir = music_dir.join("Unsorted");
        std::fs::create_dir_all(&playlist_dir).unwrap();
        let track_path = playlist_dir.join("Artist - Song.wav");
        write_wav_fixture(&track_path);
        let snapshot = state.replace_library(Generation(1), BTreeMap::from([(String::from("Unsorted"), vec![track_path])]));
        let id = snapshot.tracks[0].id.clone();
        state.set_playing(id.clone(), vec![id.clone()]);

        // Saturate the registry with sleeping dummies.
        for i in 0..VIDEO_STAGE_PRESSURE_LIMIT {
            state.submit(crate::runtime::task_registry::TaskSpec::new(format!("dummy-{i}"), crate::contracts::types::Generation(i as u64 + 500), None), |_token| {
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }).expect("dummy submit");
        }
        assert!(state.tasks.active_count() >= VIDEO_STAGE_PRESSURE_LIMIT);
        let before = state.tasks.active_count();

        state.video_stage_track(id.clone()).expect("pressure must defer, not error");
        assert_eq!(state.tasks.active_count(), before, "staging must not spawn under pressure");
        assert!(!state.video_current.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|video| {
            video.track_id.as_ref() == Some(&id)
                && matches!(video.status, crate::contracts::events::VideoStatus::Loading)
        }), "pressure rejection must not publish Loading without a worker");
        // Drains cleanly once dummies finish.
        std::thread::sleep(std::time::Duration::from_millis(1700));
        let _ = std::fs::remove_dir_all(&video_root);
        let _ = std::fs::remove_dir_all(&music_dir);
    }

    /// AUDIT (2026-08-25): the pipeline must plan CURRENT + exactly
    /// VIDEO_PREFETCH_DEPTH upcoming queue tracks, in order.
    #[test]
    fn prefetch_plan_covers_current_plus_depth_successors() {
        let id = |n: &str| TrackId::new(format!("metadata:t{n}:0")).unwrap();
        let queue: Vec<TrackId> = (0..10).map(|n| id(&n.to_string())).collect();
        // Current mid-queue: current first, then exactly depth successors.
        let plan = prefetch_plan(&queue, Some(2), VIDEO_PREFETCH_DEPTH);
        assert_eq!(plan.len(), 1 + VIDEO_PREFETCH_DEPTH);
        assert_eq!(plan[0], queue[2]);
        assert_eq!(plan[1], queue[3]);
        assert_eq!(plan[VIDEO_PREFETCH_DEPTH], queue[2 + VIDEO_PREFETCH_DEPTH]);
        // No current index: first `depth` entries.
        let plan = prefetch_plan(&queue, None, VIDEO_PREFETCH_DEPTH);
        assert_eq!(plan.len(), VIDEO_PREFETCH_DEPTH);
        assert_eq!(plan[0], queue[0]);
        // Queue shorter than depth: everything that exists.
        let short = vec![id("x")];
        let plan = prefetch_plan(&short, Some(0), VIDEO_PREFETCH_DEPTH);
        assert_eq!(plan, short);

        let recommendation = id("focused-rec");
        let focused = audible_video_plan(
            &queue,
            Some(2),
            Some(recommendation.clone()),
            true,
            VIDEO_PREFETCH_DEPTH,
        );
        assert_eq!(focused[0], recommendation);
        assert_eq!(&focused[1..], &queue[3..3 + VIDEO_PREFETCH_DEPTH]);
    }

    #[test]
    fn recommendation_video_plan_covers_the_temporary_window_after_audible_item() {
        let audible = TrackId::new("library-track").unwrap();
        let recommendations = (0..VIDEO_PREFETCH_DEPTH)
            .map(|index| TrackId::new(format!("recommendation-{index}")).unwrap())
            .collect::<Vec<_>>();
        let plan = recommendation_video_plan(Some(audible.clone()), &recommendations);
        assert_eq!(plan.first(), Some(&audible));
        assert_eq!(&plan[1..], recommendations.as_slice());

        // A recommendation that is also the audible identity must not be
        // staged twice when a mode handoff races the prefetch pass.
        let deduped = recommendation_video_plan(Some(recommendations[0].clone()), &recommendations);
        assert_eq!(deduped.len(), VIDEO_PREFETCH_DEPTH);

        // Preparing recommendations while song-page video mode is disabled
        // must not pull the unrelated audible library track as a side effect.
        assert_eq!(recommendation_video_plan(None, &recommendations), recommendations);
    }

    #[cfg(unix)]
    #[test]
    fn recommendation_video_prefetch_is_adopted_without_a_second_download() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("luno-recommendation-video-reuse-{}", std::process::id()));
        let video_root = root.join("videos");
        let library_root = root.join("library");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(library_root.join("Unsorted")).unwrap();
        let helper = root.join("video-helper");
        std::fs::write(
            &helper,
            "#!/bin/sh\nif [ \"$1\" = \"--dump-json\" ]; then\n  printf '%s\\n' '{\"id\":\"rec-video\",\"title\":\"RecSong official video\",\"uploader\":\"RecArtist\",\"webpage_url\":\"https://www.youtube.com/watch?v=rec-video\"}'\n  exit 0\nfi\noutput=\"\"\nprevious=\"\"\nfor argument in \"$@\"; do\n  if [ \"$previous\" = \"output\" ]; then output=\"$argument\"; fi\n  if [ \"$argument\" = \"-o\" ]; then previous=\"output\"; else previous=\"\"; fi\ndone\nprintf 'synthetic mp4' > \"$output\"\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&helper, permissions).unwrap();

        let state = AppState::with_preview_store(
            PreviewAssetStore::at(root.join("previews")).unwrap(),
            12,
        );
        state.set_video_runtime(video_root.clone(), Some(helper), None);
        let audio = library_root.join("Unsorted/Artist - Song.wav");
        write_wav_fixture(&audio);
        let library = state.replace_library(
            Generation(1),
            BTreeMap::from([(String::from("Unsorted"), vec![audio])]),
        );
        let audible = library.tracks[0].id.clone();
        state.set_playing(audible.clone(), vec![audible.clone()]);

        // Keep the audible item out of the helper path so this test isolates
        // recommendation scheduling and handoff reuse.
        let stager = crate::video::staging::VideoStage::at(&video_root, None).unwrap();
        stager.record_index("artistsong", "current-video");
        std::fs::write(
            stager.external_destination(crate::video::staging::Slot::Current, "current-video").unwrap(),
            b"synthetic current mp4",
        )
        .unwrap();

        let seed = PreviewSeed { artist: "RecArtist".into(), title: "RecSong".into() };
        let recommendation = AppState::preview_ids(std::slice::from_ref(&seed)).unwrap().remove(0);
        *state.preview_seeds.lock().unwrap_or_else(|e| e.into_inner()) = vec![seed];
        *state.preview_session.lock().unwrap_or_else(|e| e.into_inner()) = Some(
            PreviewSession::start_prefetch(7, vec![audible.clone()], 0, vec![recommendation.clone()]).unwrap(),
        );

        state.kick_video_pipeline();
        let stager = state.video_stager.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap();
        let next = stager.external_destination(crate::video::staging::Slot::Next, "rec-video").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while (!next.is_file() || stager.index_lookup("recartistrecsong", crate::video::staging::Slot::Next).is_none())
            && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(next.is_file(), "recommendation video prefetch did not publish its Next asset");
        assert!(stager.index_lookup("recartistrecsong", crate::video::staging::Slot::Next).is_some());

        // The click path must promote the prefetched Next file through the
        // indexed stage fast path. A missing index or file would invoke the
        // helper again and leave the recommendation loading.
        state.video_stage_track(recommendation.clone()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let ready = state.video_current.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|video| {
                video.track_id.as_ref() == Some(&recommendation)
                    && matches!(video.status, crate::contracts::events::VideoStatus::Ready)
            });
            if ready { break; }
            assert!(std::time::Instant::now() < deadline, "prepared recommendation did not adopt its video");
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(stager.external_destination(crate::video::staging::Slot::Current, "rec-video").unwrap().is_file());
        assert!(!next.is_file(), "adoption must move the prefetched Next asset into Current");
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn current_video_upgrade_runs_before_speculative_recommendation_download() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("luno-recommendation-video-priority-{}", std::process::id()));
        let video_root = root.join("videos");
        let library_root = root.join("library");
        let log = root.join("helper.log");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(library_root.join("Unsorted")).unwrap();
        let helper = root.join("video-helper");
        std::fs::write(
            &helper,
            format!(
                "#!/bin/sh\nlog={};\nif [ \"$1\" = \"--dump-json\" ]; then\n  case \"$*\" in\n    *Artist*Song*) id=current-video; title=\"Song official video\"; artist=Artist ;;\n    *) id=rec-video; title=\"RecSong official video\"; artist=RecArtist ;;\n  esac\n  printf '{{\"id\":\"%s\",\"title\":\"%s\",\"uploader\":\"%s\",\"webpage_url\":\"https://www.youtube.com/watch?v=%s\"}}\\n' \"$id\" \"$title\" \"$artist\" \"$id\"\n  exit 0\nfi\noutput=\"\"\nprevious=\"\"\nfor argument in \"$@\"; do\n  if [ \"$previous\" = \"output\" ]; then output=\"$argument\"; fi\n  if [ \"$argument\" = \"-o\" ]; then previous=output; else previous=\"\"; fi\ndone\nprintf 'download:%s\\n' \"$output\" >> \"$log\"\nprintf 'synthetic mp4' > \"$output\"\n",
                log.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&helper, permissions).unwrap();

        let state = AppState::with_preview_store(
            PreviewAssetStore::at(root.join("previews")).unwrap(),
            12,
        );
        state.set_video_runtime(video_root.clone(), Some(helper), None);
        let audio = library_root.join("Unsorted/Artist - Song.wav");
        write_wav_fixture(&audio);
        let library = state.replace_library(
            Generation(1),
            BTreeMap::from([(String::from("Unsorted"), vec![audio])]),
        );
        let audible = library.tracks[0].id.clone();
        state.set_playing(audible.clone(), vec![audible.clone()]);
        let seed = PreviewSeed { artist: "RecArtist".into(), title: "RecSong".into() };
        let recommendation = AppState::preview_ids(std::slice::from_ref(&seed)).unwrap().remove(0);
        *state.preview_seeds.lock().unwrap_or_else(|e| e.into_inner()) = vec![seed];
        *state.preview_session.lock().unwrap_or_else(|e| e.into_inner()) = Some(
            PreviewSession::start_prefetch(7, vec![audible.clone()], 0, vec![recommendation]).unwrap(),
        );
        *state.video_mode_enabled.lock().unwrap_or_else(|e| e.into_inner()) = true;

        state.kick_video_pipeline();
        let rec_next = {
            let stager = state.video_stager.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap();
            stager.external_destination(crate::video::staging::Slot::Next, "rec-video").unwrap()
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
        while (!rec_next.is_file() || !log.is_file()) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(rec_next.is_file(), "recommendation download did not complete");
        let entries = std::fs::read_to_string(&log).unwrap_or_default();
        let upgrade = entries.find(".upgrading-current-current-video.mp4").expect("current high upgrade must run");
        let successor = entries.find("next-rec-video.mp4").expect("recommendation quick download must run");
        assert!(upgrade < successor, "successor video began before current upgrade: {entries}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn video_stages_a_recommendation_from_session_metadata() {
        let state = AppState::new(8);
        let video_root = std::env::temp_dir().join(format!("luno-video-recommendation-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&video_root);
        state.set_video_runtime(video_root.clone(), Some(PathBuf::from("/nonexistent/yt-dlp")), None);
        let stager = crate::video::staging::VideoStage::at(&video_root, None).expect("stager");
        stager.record_index("artistsong", "abc123");
        let cached = stager.external_destination(crate::video::staging::Slot::Current, "abc123").unwrap();
        std::fs::write(&cached, b"fake mp4 bytes").unwrap();
        let seed = PreviewSeed { artist: "Artist".into(), title: "Song".into() };
        let id = AppState::preview_ids(std::slice::from_ref(&seed)).unwrap().remove(0);
        *state.preview_seeds.lock().unwrap_or_else(|e| e.into_inner()) = vec![seed];

        state.video_stage_track(id.clone()).expect("recommendation video request");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let ready = state.video_current.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|video| {
                video.track_id.as_ref() == Some(&id)
                    && matches!(video.status, crate::contracts::events::VideoStatus::Ready)
            });
            if ready { break; }
            assert!(std::time::Instant::now() < deadline, "recommendation video did not stage");
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let _ = std::fs::remove_dir_all(&video_root);
    }

    #[test]
    fn pending_preview_work_still_allows_the_audible_video() {
        let state = AppState::new(8);
        let video_root = std::env::temp_dir().join(format!("luno-video-preview-priority-{}", std::process::id()));
        let music_root = std::env::temp_dir().join(format!("luno-video-preview-priority-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&video_root);
        let _ = std::fs::remove_dir_all(&music_root);
        state.set_video_runtime(video_root.clone(), Some(PathBuf::from("/nonexistent/yt-dlp")), None);
        let stager = crate::video::staging::VideoStage::at(&video_root, None).expect("stager");
        stager.record_index("artistsong", "abc123");
        let cached = stager.external_destination(crate::video::staging::Slot::Current, "abc123").unwrap();
        std::fs::write(&cached, b"fake mp4 bytes").unwrap();
        let playlist = music_root.join("Unsorted");
        std::fs::create_dir_all(&playlist).unwrap();
        let path = playlist.join("Artist - Song.wav");
        write_wav_fixture(&path);
        let snapshot = state.replace_library(Generation(1), BTreeMap::from([(String::from("Unsorted"), vec![path])]));
        let library_track = snapshot.tracks[0].id.clone();
        state.set_playing(library_track.clone(), vec![library_track.clone()]);
        let recommendation = TrackId::new("recommendation-track").unwrap();
        let session = PreviewSession::start_prefetch(
            7,
            vec![library_track.clone()],
            0,
            vec![recommendation],
        )
        .unwrap();
        *state.preview_session.lock().unwrap_or_else(|e| e.into_inner()) = Some(session);
        *state.video_mode_enabled.lock().unwrap_or_else(|e| e.into_inner()) = true;

        state.kick_video_pipeline();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let ready = state.video_current.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|video| {
                video.track_id.as_ref() == Some(&library_track)
                    && matches!(video.status, crate::contracts::events::VideoStatus::Ready)
            });
            if ready { break; }
            assert!(std::time::Instant::now() < deadline, "pending recommendation audio blocked the audible video");
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let _ = std::fs::remove_dir_all(&video_root);
        let _ = std::fs::remove_dir_all(&music_root);
    }

    #[test]
    fn edit_track_metadata_renames_the_file_and_moves_favorites() {
        let root = std::env::temp_dir().join(format!("luno-edit-track-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("Unsorted");
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("dl-1789755401728-httpswwwyoutubecomwatchv.m4a");
        std::fs::write(&old, b"ID3fake").unwrap();
        let state = AppState::new(4);
        state.set_scan_roots(vec![root.clone()]);
        let id = TrackId::new("metadata:test-edit:0").unwrap();
        state.track_paths.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), old.clone());
        // Favorites store the scanner's canonical path (roots are canonicalized
        // before scanning), so seed the same form the rename will compare.
        let canonical_old = std::fs::canonicalize(&old).unwrap();
        state.set_favorite_path(&canonical_old);

        let envelope = CommandEnvelope {
            version: CONTRACT_VERSION,
            request_id: "edit-1".into(),
            command: Command::EditTrackMetadata {
                track_id: id,
                artist: "Rick Astley".into(),
                title: "Never Gonna Give You Up".into(),
            },
        };
        let result = state.dispatch(envelope).expect("edit succeeds");
        let new_path = dir.join("Rick Astley - Never Gonna Give You Up.m4a");
        assert!(new_path.is_file(), "renamed file must exist: {:?}", result);
        assert!(!old.exists(), "old filename must be gone");
        assert_eq!(result["title"], "Never Gonna Give You Up");
        assert_eq!(result["artist"], "Rick Astley");
        let favorites = state.favorites.lock().unwrap_or_else(|e| e.into_inner());
        assert!(favorites.contains(std::fs::canonicalize(&new_path).unwrap()), "favorite must follow the rename");
        assert!(!favorites.contains(&canonical_old), "stale favorite path must be cleared");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn edit_track_metadata_refuses_to_overwrite_an_existing_file() {
        let root = std::env::temp_dir().join(format!("luno-edit-clash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("Unsorted");
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("dl-99.m4a");
        std::fs::write(&old, b"ID3fake").unwrap();
        let occupied = dir.join("Taken.m4a");
        std::fs::write(&occupied, b"ID3fake").unwrap();
        let state = AppState::new(4);
        state.set_scan_roots(vec![root.clone()]);
        let id = TrackId::new("metadata:test-clash:0").unwrap();
        state.track_paths.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), old.clone());

        let envelope = CommandEnvelope {
            version: CONTRACT_VERSION,
            request_id: "edit-2".into(),
            command: Command::EditTrackMetadata { track_id: id, artist: String::new(), title: "Taken".into() },
        };
        assert!(state.dispatch(envelope).is_err(), "collision must be rejected");
        assert!(old.is_file(), "source file must be untouched on collision");
        assert_eq!(std::fs::read(&occupied).unwrap(), b"ID3fake");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// RED repro (2026-08-25): the recurring "searched a song, clicked it,
    /// and it disappeared / no more matches" report. A click that races a
    /// rescan carries ids minted from the PREVIOUS snapshot. Duplicate stems
    /// let scan order decide which copy keeps the plain id vs `#<path-hash>`,
    /// and re-downloads ADD copies — so entries of the clicked context can
    /// flip meaning or dangle outright. Healing only the clicked track left
    /// the stored queue full of stale ids: Now Playing resolved nothing,
    /// auto-advance skipped, repeat clicks kept failing. Every context entry
    /// must heal onto the CURRENT map under one lock.
    #[cfg(target_os = "macos")]
    #[test]
    fn play_with_context_heals_a_stale_context_across_a_suffix_flip() {
        let state = AppState::new(4);
        let song_a = PathBuf::from("/music/Unsorted/Artist - Song A.m4a");
        let song_a_dup = PathBuf::from("/music/Favorites/Artist - Song A.m4a");
        let song_b = PathBuf::from("/music/Unsorted/Artist - Song B.m4a");
        // Snapshot A: the list the user sees. Song A already has two copies
        // (Favorites sorts first and owns the plain id; Unsorted is hashed),
        // Song B has one.
        state.replace_library(
            Generation(1),
            BTreeMap::from([
                (String::from("Favorites"), vec![song_a_dup.clone()]),
                (String::from("Unsorted"), vec![song_b.clone(), song_a.clone()]),
            ]),
        );
        let map_a = state.track_paths.lock().unwrap_or_else(|e| e.into_inner());
        let clicked = map_a.iter().find(|(_, path)| *path == &song_a_dup).map(|(id, _)| id.clone()).expect("plain id for song A");
        let stale_b = map_a.keys().find(|id| id.0.contains("song-b") || id.0.contains("songb")).cloned()
            .unwrap_or_else(|| TrackId(format!("{}#feedface", "metadata:artistsongb:0")));
        drop(map_a);
        // A fully-dangling variant of song B's id (its copy vanished since render).
        let dangling_b = TrackId(format!("{}#deadbeef", stale_b.0.split('#').next().unwrap()));

        // Snapshot B (the race): the duplicate layout CHANGED — song A's
        // Unsorted copy is gone (re-download cleaned up), so every id the
        // click carried now resolves differently or not at all.
        state.replace_library(
            Generation(2),
            BTreeMap::from([
                (String::from("Unsorted"), vec![song_b.clone(), song_a_dup.clone()]),
            ]),
        );

        // The click arrives carrying ONLY ids from snapshot A.
        let envelope = CommandEnvelope {
            version: CONTRACT_VERSION,
            request_id: "stale-click".into(),
            command: Command::PlayWithContext {
                track_id: clicked.clone(),
                context: vec![clicked.clone(), dangling_b.clone()],
            },
        };
        let result = state.dispatch(envelope);
        assert!(result.is_ok(), "a fully stale click must still play: {result:?}");

        // EVERY stored queue id must resolve against the CURRENT map, both
        // songs survive the healing (nothing silently dropped), order is
        // preserved, and no raw stale id remains.
        let queue = state.playback.lock().unwrap_or_else(|e| e.into_inner()).queue().to_vec();
        let map = state.track_paths.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(queue.len(), 2, "both songs must survive healing: {queue:?}");
        for id in &queue {
            assert!(map.contains_key(id), "queue kept dangling id {id:?}");
        }
        // First entry = what the user clicked (its canonical owner NOW);
        // second = song B's canonical id.
        assert_eq!(map.get(&queue[0]).as_deref(), Some(&song_a_dup));
        assert_ne!(queue[1], dangling_b, "raw stale id must not be stored");
        assert_eq!(map.get(&queue[1]).as_deref(), Some(&song_b));
    }

    /// DIAGNOSIS (2026-08-25): "when I pause a song its video never loads;
    /// resuming much later finally loads it." Proves the REAL pipeline stages
    /// AND publishes Ready while playback_status is PAUSED. The video is an
    /// index-cached fixture (zero network — a nonexistent yt-dlp path proves
    /// no search/download runs), so the only question under test is whether
    /// pause state gates staging/publication. It must not.
    #[test]
    fn video_stages_to_ready_while_playback_is_paused() {
        let state = AppState::new(8);
        let video_root = std::env::temp_dir().join(format!("luno-video-pause-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&video_root);
        state.set_video_runtime(video_root.clone(), Some(PathBuf::from("/nonexistent/yt-dlp")), None);

        // Seed the persistent cache index + staged file for Artist/Song.
        let stager = crate::video::staging::VideoStage::at(&video_root, None).expect("stager");
        stager.record_index("artistsong", "abc123");
        let cached = stager.external_destination(crate::video::staging::Slot::Current, "abc123").unwrap();
        std::fs::write(&cached, b"fake mp4 bytes").unwrap();

        // One-track library; title/artist fold to the seeded key.
        let music_dir = std::env::temp_dir().join(format!("luno-video-pause-lib-{}", std::process::id()));
        let playlist_dir = music_dir.join("Unsorted");
        std::fs::create_dir_all(&playlist_dir).unwrap();
        let track_path = playlist_dir.join("Artist - Song.m4a");
        std::fs::write(&track_path, b"placeholder audio").unwrap();
        let snapshot = state.replace_library(Generation(1), BTreeMap::from([(String::from("Unsorted"), vec![track_path])]));
        let id = snapshot.tracks[0].id.clone();

        state.set_playing(id.clone(), vec![id.clone()]);
        // THE CONDITION UNDER TEST: the user paused before/while staging.
        *state.playback_status.lock().unwrap_or_else(|e| e.into_inner()) = crate::contracts::types::PlaybackStatus::Paused;
        *state.video_mode_enabled.lock().unwrap_or_else(|e| e.into_inner()) = true;

        state.kick_video_pipeline();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut terminal = String::from("timeout");
        while std::time::Instant::now() < deadline {
            for envelope in state.pending_events() {
                if let crate::contracts::events::Event::VideoState { state: video } = &envelope.event {
                    if video.track_id.as_ref() == Some(&id) {
                        terminal = format!("{:?}", video.status);
                    }
                }
            }
            if terminal.contains("Ready") || terminal.contains("Unavailable") { break; }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(terminal.contains("Ready"), "staging must complete while paused, got {terminal}");
        assert_eq!(*state.playback_status.lock().unwrap_or_else(|e| e.into_inner()), crate::contracts::types::PlaybackStatus::Paused);
        let current = state.video_current.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert!(current.map(|v| v.staged_path.is_some()).unwrap_or(false));
        let _ = std::fs::remove_dir_all(&video_root);
        let _ = std::fs::remove_dir_all(&music_dir);
    }

    /// REGRESSION (2026-08-25): a Loading slot whose worker died (superseded
    /// mid-search by the old double-kick race) used to be skipped by every
    /// later pipeline pass — "Loading video preview..." forever while the
    /// song sat paused. A corpse must restage.
    #[test]
    fn kick_restages_a_loading_slot_with_no_live_worker() {
        let state = AppState::new(8);
        let video_root = std::env::temp_dir().join(format!("luno-video-corpse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&video_root);
        state.set_video_runtime(video_root.clone(), Some(PathBuf::from("/nonexistent/yt-dlp")), None);
        let stager = crate::video::staging::VideoStage::at(&video_root, None).expect("stager");
        stager.record_index("artistsong", "abc123");
        let cached = stager.external_destination(crate::video::staging::Slot::Current, "abc123").unwrap();
        std::fs::write(&cached, b"fake mp4 bytes").unwrap();

        let music_dir = std::env::temp_dir().join(format!("luno-video-corpse-lib-{}", std::process::id()));
        let playlist_dir = music_dir.join("Unsorted");
        std::fs::create_dir_all(&playlist_dir).unwrap();
        let track_path = playlist_dir.join("Artist - Song.m4a");
        std::fs::write(&track_path, b"placeholder audio").unwrap();
        let snapshot = state.replace_library(Generation(1), BTreeMap::from([(String::from("Unsorted"), vec![track_path])]));
        let id = snapshot.tracks[0].id.clone();

        state.set_playing(id.clone(), vec![id.clone()]);
        *state.video_mode_enabled.lock().unwrap_or_else(|e| e.into_inner()) = true;
        // Install the CORPSE: Loading, owned by this track, NO inflight marker.
        *state.video_current.lock().unwrap_or_else(|e| e.into_inner()) = Some(crate::contracts::events::VideoState {
            track_id: Some(id.clone()),
            source_url: None,
            staged_path: None,
            status: crate::contracts::events::VideoStatus::Loading,
            position_ms: 0,
            download_progress: None,
            revision: None,
            fullscreen: false,
            generation: Generation(7),
        });

        state.kick_video_pipeline();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut terminal = String::from("timeout");
        while std::time::Instant::now() < deadline {
            for envelope in state.pending_events() {
                if let crate::contracts::events::Event::VideoState { state: video } = &envelope.event {
                    if video.track_id.as_ref() == Some(&id) { terminal = format!("{:?}", video.status); }
                }
            }
            if terminal.contains("Ready") || terminal.contains("Unavailable") { break; }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(terminal.contains("Ready"), "a dead Loading slot must restage, got {terminal}");
        let _ = std::fs::remove_dir_all(&video_root);
        let _ = std::fs::remove_dir_all(&music_dir);
    }

    /// REGRESSION (2026-08-25): resuming after a long pause must find the
    /// video ALREADY staged ("ready to go"), not start loading on resume.
    /// Resume now runs a catch-up pipeline pass when the current track's
    /// video is unresolved. The audio backend intentionally has nothing
    /// loaded here — its error proves the kick happens BEFORE audio work.
    #[test]
    fn resume_kicks_the_video_pipeline_when_the_current_video_is_unresolved() {
        let state = AppState::new(8);
        let video_root = std::env::temp_dir().join(format!("luno-video-resume-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&video_root);
        state.set_video_runtime(video_root.clone(), Some(PathBuf::from("/nonexistent/yt-dlp")), None);
        let stager = crate::video::staging::VideoStage::at(&video_root, None).expect("stager");
        stager.record_index("artistsong", "abc123");
        let cached = stager.external_destination(crate::video::staging::Slot::Current, "abc123").unwrap();
        std::fs::write(&cached, b"fake mp4 bytes").unwrap();

        let music_dir = std::env::temp_dir().join(format!("luno-video-resume-lib-{}", std::process::id()));
        let playlist_dir = music_dir.join("Unsorted");
        std::fs::create_dir_all(&playlist_dir).unwrap();
        let track_path = playlist_dir.join("Artist - Song.m4a");
        std::fs::write(&track_path, b"placeholder audio").unwrap();
        let snapshot = state.replace_library(Generation(1), BTreeMap::from([(String::from("Unsorted"), vec![track_path])]));
        let id = snapshot.tracks[0].id.clone();

        state.set_playing(id.clone(), vec![id.clone()]);
        *state.playback_status.lock().unwrap_or_else(|e| e.into_inner()) = crate::contracts::types::PlaybackStatus::Paused;
        *state.video_mode_enabled.lock().unwrap_or_else(|e| e.into_inner()) = true;

        let result = state.dispatch(CommandEnvelope {
            version: CONTRACT_VERSION,
            request_id: "resume".into(),
            command: Command::Resume,
        });
        // Backend has no loaded track in this harness; the audio failure is
        // expected and irrelevant — the catch-up kick must have run already.
        let _ = result;

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut terminal = String::from("timeout");
        while std::time::Instant::now() < deadline {
            for envelope in state.pending_events() {
                if let crate::contracts::events::Event::VideoState { state: video } = &envelope.event {
                    if video.track_id.as_ref() == Some(&id) { terminal = format!("{:?}", video.status); }
                }
            }
            if terminal.contains("Ready") || terminal.contains("Unavailable") { break; }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(terminal.contains("Ready"), "resume must catch up the video pipeline, got {terminal}");
        let _ = std::fs::remove_dir_all(&video_root);
        let _ = std::fs::remove_dir_all(&music_dir);
    }

    /// RED repro (2026-08-23 evening): with video mode ON the song page sat on
    /// "Loading video preview..." forever even though the staging store held
    /// the fully downloaded MP4. Drives the REAL pipeline: play a queued
    /// track, kick the video-mode pipeline exactly as the play hook does, and
    /// require the current track to reach Ready. Network-dependent,
    /// hence #[ignore] — run explicitly:
    ///   cargo test --lib video_mode_auto_stage_reaches_ready -- --ignored --nocapture
    #[test]
    #[ignore]
    fn video_mode_auto_stage_reaches_ready() {
        fn command(request_id: &str, cmd: Command) -> CommandEnvelope {
            CommandEnvelope { version: CONTRACT_VERSION, request_id: request_id.into(), command: cmd }
        }
        let state = AppState::new(8);
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let bin = std::path::PathBuf::from(&manifest_dir).join("resources").join("bin");
        let video_root = std::env::temp_dir().join(format!("luno-video-mode-e2e-{}", std::process::id()));
        state.set_video_runtime(video_root.clone(), Some(bin.join("yt-dlp")), Some(bin.clone()));

        let music_dir = std::env::temp_dir().join(format!("luno-video-mode-lib-{}", std::process::id()));
        let playlist_dir = music_dir.join("Unsorted");
        std::fs::create_dir_all(&playlist_dir).unwrap();
        let mut ids = Vec::new();
        for name in ["Rick Astley - Never Gonna Give You Up.m4a", "Fixture Artist Two - Second Song.m4a", "Fixture Artist Three - Third Song.m4a"] {
            let path = playlist_dir.join(name);
            std::fs::write(&path, b"placeholder audio bytes").unwrap();
            ids.push(path);
        }
        let snapshot = state.replace_library(Generation(1), BTreeMap::from([(String::from("Unsorted"), ids)]));
        let track_id = |title: &str| snapshot.tracks.iter().find(|t| t.title == title).map(|t| t.id.clone()).unwrap();
        let first = track_id("Never Gonna Give You Up");

        // Persisted home so the backend-side toggle can save; then enable mode.
        let home = std::env::temp_dir().join(format!("luno-video-mode-home-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        *state.settings_home.lock().unwrap_or_else(|e| e.into_inner()) = Some(home.clone());
        let result = state.dispatch(command("vm-on", Command::SetVideoMode { enabled: true }));
        eprintln!("[e2e] set_video_mode -> {result:?}");

        // Play the first track with a queue behind it (prefetch targets).
        state.set_playing(first.clone(), snapshot.tracks.iter().map(|t| t.id.clone()).collect());
        // Mirror the play_with_context hook exactly.
        state.kick_video_pipeline();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(150);
        let mut final_status = String::from("timeout");
        while std::time::Instant::now() < deadline {
            for envelope in state.pending_events() {
                if let crate::contracts::events::Event::VideoState { state: video } = &envelope.event {
                    if video.track_id.as_ref() == Some(&first) {
                        eprintln!("[e2e] video event: status={:?} staged={:?} gen={}", video.status, video.staged_path, video.generation.0);
                        final_status = format!("{:?}", video.status);
                    }
                }
            }
            if final_status.contains("Ready") || final_status.contains("Unavailable") { break; }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        eprintln!("[e2e] FINAL STATUS for current track: {final_status}");
        let entries: Vec<_> = std::fs::read_dir(&video_root).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect::<Vec<_>>()).unwrap_or_default();
        eprintln!("[e2e] staged files: {entries:?} (prefetch populates next-*.mp4 when the synthetic titles match YouTube results)");
        assert_eq!(final_status, "Ready", "current track must reach Ready with video mode on");
    }

    #[test]
    #[ignore]
    fn video_stage_real_pipeline() {
        fn command(request_id: &str, cmd: Command) -> CommandEnvelope {
            CommandEnvelope { version: CONTRACT_VERSION, request_id: request_id.into(), command: cmd }
        }
        let state = AppState::new(4);
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let bin = std::path::PathBuf::from(&manifest_dir).join("resources").join("bin");
        assert!(bin.join("yt-dlp").is_file(), "bundled yt-dlp missing");
        let video_root = std::env::temp_dir().join(format!("luno-video-e2e-{}", std::process::id()));
        state.set_video_runtime(video_root.clone(), Some(bin.join("yt-dlp")), Some(bin.clone()));

        // A file whose FILENAME parses into artist/title like real libraries.
        let music_dir = std::env::temp_dir().join(format!("luno-video-lib-{}", std::process::id()));
        let playlist_dir = music_dir.join("Unsorted");
        std::fs::create_dir_all(&playlist_dir).unwrap();
        let track_path = playlist_dir.join("Rick Astley - Never Gonna Give You Up.m4a");
        std::fs::write(&track_path, b"placeholder audio bytes").unwrap();
        state.replace_library(
            Generation(1),
            BTreeMap::from([(String::from("Unsorted"), vec![track_path])]),
        );
        let track_id = {
            let paths = state.track_paths.lock().unwrap_or_else(|e| e.into_inner());
            paths.keys().next().cloned().expect("library must expose the fixture track")
        };

        let result = state.dispatch(command("video-e2e", Command::VideoStage { track_id: track_id.clone() }));
        eprintln!("[e2e] dispatch acknowledged: {result:?}");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(240);
        let mut final_status = String::from("timeout");
        let mut quick_ready = false;
        let mut high_ready = false;
        let mut staged_path: Option<PathBuf> = None;
        while std::time::Instant::now() < deadline {
            // Drain so each event prints once (otherwise the growing pending
            // queue re-prints the same envelope every poll and hides progress).
            for envelope in state.pending_events() {
                state.acknowledge_event(&envelope.event_id);
                if let crate::contracts::events::Event::VideoState { state: video } = &envelope.event {
                    eprintln!("[e2e] video event: status={:?} progress={:?} staged={:?} gen={}", video.status, video.download_progress, video.staged_path, video.generation.0);
                    final_status = format!("{:?}", video.status);
                    if matches!(video.status, crate::contracts::events::VideoStatus::Ready | crate::contracts::events::VideoStatus::Playing | crate::contracts::events::VideoStatus::Paused) {
                        quick_ready = true;
                        staged_path = video.staged_path.as_ref().map(PathBuf::from);
                        high_ready |= video.revision.is_some_and(|revision| revision >= 2);
                    }
                }
            }
            if high_ready || final_status.contains("Unavailable") { break; }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        eprintln!("[e2e] FINAL STATUS: {final_status}");
        let entries: Vec<_> = std::fs::read_dir(&video_root).map(|rd| rd.flatten().map(|e| e.path()).collect::<Vec<_>>()).unwrap_or_default();
        eprintln!("[e2e] staged files: {entries:?}");
        assert!(quick_ready, "pipeline never published the quick tier: {final_status}");
        assert!(high_ready, "pipeline never published the high-tier replacement: {final_status}");
        let staged_path = staged_path.expect("ready video path");
        let stream = crate::video::stream::response_for(
            &video_root,
            "GET",
            &staged_path.to_string_lossy(),
            Some("bytes=0-1023"),
        );
        assert_eq!(stream.status(), 206);
        assert!(!stream.body().is_empty());

        let probe = std::process::Command::new(bin.join("ffprobe"))
            .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name", "-of", "csv=p=0"])
            .arg(&staged_path)
            .output()
            .expect("ffprobe staged video");
        assert!(probe.status.success());
        assert_eq!(String::from_utf8_lossy(&probe.stdout).trim(), "h264");
    }

    #[test]
    fn failed_event_delivery_preserves_event_until_acknowledged() {
        let state = AppState::new(1);
        state.set_event_sink(StdArc::new(|_| Err("emit failed".into())));
        let event = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        state.publish(event.clone());

        let pending = state.pending_events();
        assert_eq!(pending[0], event);
        assert!(matches!(pending[1].event, Event::Error { .. }));

        state.acknowledge_event(&pending[0].event_id);
        assert_eq!(state.pending_events().len(), 1);
    }

    #[test]
    fn delivery_error_id_is_outside_domain_sequence() {
        let state = AppState::new(1);
        state.set_event_sink(StdArc::new(|_| Err("offline".into())));
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        let ids: Vec<_> = state
            .pending_events()
            .into_iter()
            .map(|event| event.event_id)
            .collect();
        assert_eq!(ids[0], "event-1");
        assert!(ids[1]
            .strip_prefix("delivery-error-")
            .is_some_and(|id| id.parse::<u64>().is_ok()));
        assert_ne!(ids[1], "delivery-error-2");
    }

    #[test]
    fn event_drain_has_one_consumer_and_requeues_failures() {
        let state = AppState::new(1);
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        let worker_state = state.clone();
        let entered = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = entered.clone();
        let first = std::thread::spawn(move || {
            worker_state
                .drain_events(|_| {
                    release.wait();
                    std::thread::sleep(Duration::from_millis(20));
                    Ok(())
                })
                .unwrap()
        });
        entered.wait();
        assert_eq!(state.drain_events(|_| Ok(())).unwrap(), false);
        assert_eq!(first.join().unwrap(), true);

        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        ));
        assert!(state.drain_events(|_| Err("offline".into())).is_err());
        assert_eq!(state.pending_events().len(), 1);
    }

    #[test]
    fn panicking_worker_is_a_shutdown_failure() {
        let state = AppState::new(1);
        state
            .submit(TaskSpec::new("panic", Generation(1), None), |_| -> () {
                panic!("worker failure")
            })
            .unwrap();
        let report = state.shutdown_events(
            crate::contracts::events::ShutdownReason::System,
            Duration::from_secs(1),
        );
        assert!(report.failed);
        assert!(!report.timed_out);
        assert!(state.events().iter().any(|event| matches!(
            event.event,
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    state: ShutdownState::Failed,
                    ..
                }
            }
        )));
    }

    #[test]
    fn shutdown_admission_barrier_is_active_before_requested_event() {
        let state = AppState::new(1);
        let observed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed_sink = observed.clone();
        let sink_state = state.clone();
        state.set_event_sink(StdArc::new(move |event| {
            if matches!(
                &event.event,
                Event::Shutdown {
                    envelope: ShutdownEnvelope {
                        state: ShutdownState::Requested,
                        ..
                    }
                }
            ) {
                observed_sink.store(
                    sink_state.is_closing(),
                    std::sync::atomic::Ordering::Release,
                );
            }
            Ok(())
        }));
        state.shutdown_events(
            crate::contracts::events::ShutdownReason::System,
            Duration::from_secs(1),
        );
        assert!(observed.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn publication_is_serialized_and_latest_events_replay() {
        let state = AppState::new(1);
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink_delivered = delivered.clone();
        state.set_event_sink(StdArc::new(move |event| {
            sink_delivered.lock().unwrap_or_else(|e| e.into_inner()).push(event.event_id.clone());
            Ok(())
        }));
        let snapshot = state.event(
            Generation(1),
            Event::LibrarySnapshot {
                snapshot: state.library(),
            },
        );
        let shutdown = state.event(
            Generation(0),
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    version: CONTRACT_VERSION,
                    state: ShutdownState::Complete,
                    reason: crate::contracts::events::ShutdownReason::System,
                },
            },
        );
        state.publish(snapshot.clone());
        state.publish(shutdown.clone());
        *delivered.lock().unwrap_or_else(|e| e.into_inner()) = Vec::new();
        state.replay_latest();
        assert_eq!(
            *delivered.lock().unwrap_or_else(|e| e.into_inner()),
            vec![snapshot.event_id, shutdown.event_id]
        );
    }

    #[test]
    fn newer_publication_waits_for_older_sink_delivery() {
        let state = AppState::new(1);
        let entered = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = std::sync::Arc::new(std::sync::Barrier::new(2));
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink_delivered = delivered.clone();
        let sink_release = release.clone();
        let sink_entered = entered.clone();
        state.set_event_sink(StdArc::new(move |event| {
            sink_delivered.lock().unwrap_or_else(|e| e.into_inner()).push(event.event_id.clone());
            if event.event_id == "event-1" {
                sink_entered.wait();
                sink_release.wait();
            }
            Ok(())
        }));
        let first = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        let second = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        );
        let worker_state = state.clone();
        let first_thread = std::thread::spawn(move || worker_state.publish(first));
        entered.wait();
        let worker_state = state.clone();
        let second_thread = std::thread::spawn(move || worker_state.publish(second));
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(delivered.lock().unwrap_or_else(|e| e.into_inner()).len(), 1);
        release.wait();
        first_thread.join().unwrap();
        second_thread.join().unwrap();
        assert_eq!(delivered.lock().unwrap_or_else(|e| e.into_inner()).len(), 2);
    }

    #[test]
    fn allocated_event_ids_publish_in_allocation_order() {
        let state = AppState::new(1);
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink_delivered = delivered.clone();
        state.set_event_sink(StdArc::new(move |event| {
            sink_delivered.lock().unwrap_or_else(|e| e.into_inner()).push(event.event_id);
            Ok(())
        }));
        let first = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        let second = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        );
        state.publish(second);
        assert!(delivered.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
        state.publish(first);
        assert_eq!(*delivered.lock().unwrap_or_else(|e| e.into_inner()), vec!["event-1", "event-2"]);
    }

    #[test]
    fn retry_without_sink_reports_buffered_events() {
        let state = AppState::new(1);
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        assert_eq!(state.retry_event_delivery(Duration::ZERO), 1);
    }

    #[test]
    fn retry_reports_an_active_drainer_until_it_finishes() {
        let state = AppState::new(1);
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        let entered = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = entered.clone();
        let worker_state = state.clone();
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let worker = std::thread::spawn(move || {
            worker_state.drain_events(|_| {
                worker_entered.wait();
                worker_release.wait();
                Ok(())
            })
        });
        entered.wait();
        assert_eq!(state.retry_event_delivery(Duration::ZERO), 1);
        release.wait();
        assert_eq!(worker.join().unwrap(), Ok(true));
        assert_eq!(state.retry_event_delivery(Duration::ZERO), 0);
    }

    #[test]
    fn replay_uses_the_pending_order_without_duplicates() {
        let state = AppState::new(1);
        let snapshot = state.event(
            Generation(1),
            Event::LibrarySnapshot {
                snapshot: state.library(),
            },
        );
        let shutdown = state.event(
            Generation(0),
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    version: CONTRACT_VERSION,
                    state: ShutdownState::Complete,
                    reason: crate::contracts::events::ShutdownReason::System,
                },
            },
        );
        state.publish(snapshot.clone());
        state.publish(shutdown.clone());
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink_delivered = delivered.clone();
        state.set_event_sink(StdArc::new(move |event| {
            sink_delivered.lock().unwrap_or_else(|e| e.into_inner()).push(event.event_id);
            Ok(())
        }));

        state.replay_latest();

        assert_eq!(
            *delivered.lock().unwrap_or_else(|e| e.into_inner()),
            vec![snapshot.event_id.clone(), shutdown.event_id.clone()]
        );
        assert!(state.pending_events().is_empty());
        state.replay_latest();
        assert_eq!(
            *delivered.lock().unwrap_or_else(|e| e.into_inner()),
            vec![snapshot.event_id, shutdown.event_id]
        );
    }

    #[test]
    fn publication_queues_new_events_behind_a_failed_delivery() {
        let state = AppState::new(1);
        let attempts = std::sync::Arc::new(Mutex::new(Vec::new()));
        let fail_first = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let sink_attempts = attempts.clone();
        let sink_fail_first = fail_first.clone();
        state.set_event_sink(StdArc::new(move |event| {
            sink_attempts.lock().unwrap_or_else(|e| e.into_inner()).push(event.event_id.clone());
            if event.event_id == "event-1"
                && sink_fail_first.swap(false, std::sync::atomic::Ordering::AcqRel)
            {
                Err("offline".into())
            } else {
                Ok(())
            }
        }));
        let first = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        let second = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        );

        state.publish(first);
        state.publish(second);

        assert_eq!(*attempts.lock().unwrap_or_else(|e| e.into_inner()), vec!["event-1"]);
        assert_eq!(state.retry_event_delivery(Duration::ZERO), 0);
        assert_eq!(
            *attempts.lock().unwrap_or_else(|e| e.into_inner()),
            vec!["event-1", "event-1", "event-2", "delivery-error-1"]
        );
    }

    #[test]
    fn panicking_delivery_resets_the_drainer() {
        let state = AppState::new(1);
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        assert!(state.drain_events(|_| panic!("sink panic")).is_err());
        assert_eq!(state.drain_events(|_| Ok(())), Ok(true));
    }

    #[test]
    fn panicking_batch_retries_only_undelivered_suffix() {
        let state = AppState::new(1);
        for index in 0..3 {
            state.publish(state.event(
                Generation(1),
                Event::LibraryRootStatus {
                    root_index: index,
                    status: RootStatus::Scanned,
                },
            ));
        }
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let seen = delivered.clone();
        assert!(state
            .drain_events(|event| {
                seen.lock().unwrap_or_else(|e| e.into_inner()).push(event.event_id.clone());
                if event.event_id == "event-2" {
                    panic!("sink panic");
                }
                Ok(())
            })
            .is_err());
        assert_eq!(*delivered.lock().unwrap_or_else(|e| e.into_inner()), vec!["event-1", "event-2"]);
        assert_eq!(
            state
                .pending_events()
                .iter()
                .map(|event| event.event_id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-2", "event-3"]
        );
    }

    #[test]
    fn discarded_event_id_does_not_block_later_publication() {
        let state = AppState::new(1);
        let orphan = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        let later = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        );
        state.discard_event(&orphan.event_id);
        state.publish(later);
        assert_eq!(
            state
                .pending_events()
                .iter()
                .map(|event| event.event_id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-2"]
        );
    }

    #[test]
    fn discarding_unpublished_event_removes_its_buffered_envelope() {
        let state = AppState::new(1);
        let first = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        let second = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        );
        state.publish(second.clone());
        state.discard_event(&second.event_id);
        state.publish(first);
        assert_eq!(
            state
                .pending_events()
                .iter()
                .map(|event| event.event_id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-1"]
        );
    }

    #[test]
    fn discard_rejects_future_and_repeated_ids() {
        let state = AppState::new(1);
        state.discard_event("event-99");
        let event = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        state.discard_event("event-99");
        state.publish(event);
        assert_eq!(
            state
                .pending_events()
                .iter()
                .map(|event| event.event_id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-1"]
        );
    }

    #[test]
    fn delivery_error_ids_are_separate_and_monotonic() {
        let state = AppState::new(1);
        state.set_event_sink(StdArc::new(|_| Err("offline".into())));
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        let first_ids: Vec<_> = state
            .events()
            .into_iter()
            .map(|event| event.event_id)
            .collect();
        assert_eq!(first_ids, vec!["event-1", "delivery-error-1"]);
        state.acknowledge_event("delivery-error-1");
        assert!(state
            .events()
            .iter()
            .all(|event| event.event_id != "delivery-error-1"));
        state.acknowledge_event("event-1");
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        ));
        let errors: Vec<_> = state
            .events()
            .into_iter()
            .filter(|event| matches!(event.event, Event::Error { .. }))
            .map(|event| event.event_id)
            .collect();
        assert_eq!(errors, vec!["delivery-error-2"]);
    }

    #[test]
    fn delivery_error_counter_exhaustion_reports_without_unsafe_id() {
        let state = AppState::new(1);
        state
            .next_delivery_error
            .store(MAX_SAFE_EVENT_ID, Ordering::Relaxed);
        state.set_event_sink(StdArc::new(|_| Err("offline".into())));
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        assert!(state.delivery_failure().is_some());
        assert!(state
            .events()
            .iter()
            .all(|event| !event.event_id.starts_with("delivery-error-")));
    }
}

#[derive(Clone)]
pub struct AppState {
    pub tasks: Arc<TaskRegistry>,
    pub migration_status: Arc<Mutex<MigrationStatus>>,
    library: Arc<Mutex<LibrarySnapshot>>,
    discovery: Arc<Mutex<DiscoveryService>>,
    preview_store: Arc<Mutex<PreviewAssetStore>>,
    preview_session: Arc<Mutex<Option<PreviewSession>>>,
    preview_seeds: Arc<Mutex<Vec<PreviewSeed>>>,
    preview_tasks: Arc<Mutex<Vec<String>>>,
    preview_completions: Arc<Mutex<Option<std::sync::mpsc::Sender<PreviewCompletion>>>>,
    preview_cancelled: Arc<AtomicBool>,
    preview_helper: Arc<Mutex<Option<PathBuf>>>,
    preview_ffmpeg: Arc<Mutex<Option<PathBuf>>>,
    discover: Arc<Mutex<Option<DiscoverMode>>>,
    discover_seeds: Arc<Mutex<Vec<PreviewSeed>>>,
    discover_tasks: Arc<Mutex<Vec<String>>>,
    discover_completions: Arc<Mutex<Option<std::sync::mpsc::Sender<PreviewCompletion>>>>,
    save_in_flight: Arc<Mutex<std::collections::HashSet<TrackId>>>,
    pending_download_saves: Arc<Mutex<BTreeMap<String, (TrackId, PlaylistId, PathBuf)>>>,
    roots: Arc<Mutex<Vec<PathBuf>>>,
    root_statuses: Arc<Mutex<Vec<RootStatus>>>,
    events: Arc<Mutex<Vec<EventEnvelope>>>,
    event_draining: Arc<Mutex<bool>>,
    event_in_flight: Arc<Mutex<usize>>,
    next_event: Arc<AtomicU64>,
    next_delivery_error: Arc<AtomicU64>,
    next_publish: Arc<AtomicU64>,
    next_generation: Arc<AtomicU64>,
    allocated_events: Arc<Mutex<BTreeSet<u64>>>,
    event_sink:
        Arc<Mutex<Option<StdArc<dyn Fn(EventEnvelope) -> Result<(), String> + Send + Sync>>>>,
    publication: Arc<Mutex<()>>,
    unpublished: Arc<Mutex<BTreeMap<u64, EventEnvelope>>>,
    skipped_publications: Arc<Mutex<BTreeSet<u64>>>,
    admission: Arc<Mutex<()>>,
    latest_snapshot: Arc<Mutex<Option<EventEnvelope>>>,
    latest_control: Arc<Mutex<Option<EventEnvelope>>>,
    replayed_events: Arc<Mutex<BTreeSet<String>>>,
    event_retry_stopped: Arc<AtomicBool>,
    shutdown_result: Arc<Mutex<Option<ShutdownReport>>>,
    delivery_failure: Arc<Mutex<Option<ContractError>>>,
    supervisor: Arc<Supervisor>,
    downloads: Arc<DownloadCoordinator>,
    download_requests: Arc<Mutex<BTreeMap<String, (PathBuf, PathBuf, String)>>>,
    bundle_helper: Arc<Mutex<Option<PathBuf>>>,
    video_root: Arc<Mutex<Option<PathBuf>>>,
    video_stager: Arc<Mutex<Option<crate::video::staging::VideoStage>>>,    video_current: Arc<Mutex<Option<crate::contracts::events::VideoState>>>,
    video_cancel: Arc<Mutex<Arc<AtomicBool>>>,
    video_helper: Arc<Mutex<Option<PathBuf>>>,
    video_ffmpeg_dir: Arc<Mutex<Option<PathBuf>>>,
    discovery_configured: Arc<AtomicBool>,
    provider_metadata: Arc<Mutex<BTreeMap<PathBuf, TrackMetadata>>>,
    track_paths: Arc<Mutex<BTreeMap<TrackId, PathBuf>>>,
    artwork: Arc<TrackArtwork>,
    current_track: Arc<Mutex<Option<TrackId>>>,
    playback: Arc<Mutex<QueueState>>,
    clock: Arc<Mutex<Option<PlaybackClock>>>,
    recovery: Arc<Mutex<FailureRecovery>>,
    favorites: Arc<Mutex<Favorites>>,
    recent: Arc<Mutex<Vec<TrackId>>>,
    scan: Arc<Mutex<ScanState>>,
    #[cfg(target_os = "macos")]
    playback_status: Arc<Mutex<crate::contracts::types::PlaybackStatus>>,
    #[cfg(target_os = "macos")]
    audio: Arc<Mutex<Box<dyn AudioBackend + Send>>>,
    /// Intended master volume (`0.0..=1.0`). Tracked at the state level so the
    /// value survives backend state replacement on every track load, seeded
    /// from persisted settings, and is reapplied whenever a new track starts.
    #[cfg(target_os = "macos")]
    volume: Arc<Mutex<f32>>,
    /// Stall watchdog for the macOS heartbeat: `Some((last_position, ticks))`
    /// while status is Playing and the backend position has stopped advancing.
    /// A silent cpal stream death (device switch) freezes `played_frames`
    /// without any error callback; without this watchdog the track sat frozen
    /// forever and play/pause clicks appeared to do nothing.
    #[cfg(target_os = "macos")]
    playback_stall: Arc<Mutex<Option<(std::time::Duration, u32)>>>,
    /// Song-page video mode (persisted in `.luno_config.json`). When true the
    /// backend auto-stages the playing track's music video and sequentially
    /// pre-downloads the next five queue videos so each handoff is instant.
    video_mode_enabled: Arc<Mutex<bool>>,
    /// Serializes video-mode prefetch passes: one loop at a time.
    video_prefetch_running: Arc<Mutex<bool>>,
    /// TrackIds with a live staging worker. The song-page effect and the
    /// video-mode auto-stager can both fire for the same track within
    /// milliseconds; without this dedupe two workers race through the same
    /// multi-second search and each publishes over the other.
    video_staging_inflight: Arc<Mutex<std::collections::HashSet<TrackId>>>,
    /// Live download percentage for the CURRENT track's staging worker.
    video_download_progress: Arc<crate::video::runtime::VideoProgress>,
    /// Cancellation for the CURRENT prefetch pass. Replaced on every kick;
    /// a stale flag means the pass was superseded and must stop.
    video_prefetch_cancel: Arc<Mutex<Arc<AtomicBool>>>,
    /// Serializes BACKGROUND heavy video work — next-track prefetch downloads
    /// and the quality-upgrade transcode — so only one of them ever competes
    /// with playback for disk/CPU/network. Interactive current-track staging
    /// (the small quick-tier pull) is deliberately NOT gated by it. Acquire
    /// via polling `try_lock` with cancellation checkpoints, never `.lock()`.
    video_heavy_work: Arc<Mutex<()>>,
    /// Set while the audible track has published its quick tier and is waiting
    /// for its high-quality replacement. Speculative successor downloads yield
    /// before taking `video_heavy_work`, so a five-item recommendation window
    /// cannot delay the current track's upgrade.
    video_upgrade_waiting: Arc<AtomicBool>,
    /// Runtime home captured at load time so backend-side setting changes can
    /// persist immediately without an extra IPC round-trip.
    settings_home: Arc<Mutex<Option<PathBuf>>>,
    /// Settings ▸ Playback "media keys" toggle, mirrored here so hardware
    /// key handlers can check it cheaply on every press. Defaults to TRUE —
    /// Luno must own the media keys unless the user opts out.
    pub media_keys_enabled: Arc<AtomicBool>,
    /// Cached (title, artist, album) for the Now Playing center. Updated only
    /// when a track actually starts, so the audio-lock-held heartbeat path can
    /// publish now-playing state without taking the library lock.
    #[cfg(target_os = "macos")]
    now_playing_track: Arc<Mutex<Option<(String, String, String)>>>,
}

type ScanWork = Arc<dyn Fn(&[PathBuf], CancellationToken) -> ScanOutcome + Send + Sync>;

struct ScanState {
    active: bool,
    pending: Option<ScanWork>,
}

struct VideoUpgradeWaitGuard(Arc<AtomicBool>);

impl Drop for VideoUpgradeWaitGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Whether a FINISHING staging worker may publish its result into the shared
/// video slot. Ownership rules (unit-tested):
/// - empty slot → always yes;
/// - slot already shows THIS worker's track → yes (its Loading placeholder);
/// - a NEWER claim (higher generation) for another track → NO: overwriting it
///   stranded the UI on stale results and made real ones vanish.
pub fn completion_may_publish(
    current: Option<&crate::contracts::events::VideoState>,
    worker_track_id: &TrackId,
    worker_generation: u64,
) -> bool {
    match current {
        None => true,
        Some(video) => {
            video.track_id.as_ref() == Some(worker_track_id)
                || video.generation.0 <= worker_generation
        }
    }
}

/// Recover a STALE or COLLISION-FLIPPED track id so a visible row can never
/// hard-fail with "track is not available locally".
///
/// Track ids are `metadata:<key>:<duration>` (or `provider:…`), and files
/// whose metadata cannot distinguish them append a `#<8-hex path hash>`.
/// WHICH copy keeps the unsuffixed id depends on scan order, so adding,
/// removing, or re-downloading a duplicate can flip ids between scans —
/// ids captured by the queue panel, recent history, Home cards, or a click
/// that raced a rescan then dangle. When the EXACT id misses but another id
/// sharing its base exists, play that file instead of rejecting:
/// - exact base present → use it (the canonical copy won a rescan);
/// - exactly ONE suffixed sibling → use it (the clicked row gained a
///   discriminator);
/// - zero or several siblings → still fail (ambiguous).
pub fn heal_track_id(candidates: &std::collections::BTreeMap<TrackId, PathBuf>, requested: &TrackId) -> Option<TrackId> {    let base = requested.0.split('#').next()?.to_string();
    if base.is_empty() { return None; }
    let base_id = TrackId(base.clone());
    if candidates.contains_key(&base_id) { return Some(base_id); }
    let matches: Vec<&TrackId> = candidates.keys().filter(|id| id.0.split('#').next() == Some(base.as_str())).collect();
    if matches.len() == 1 { return Some(matches[0].clone()); }
    None
}

/// How many UPCOMING context tracks get a quick-tier video pre-download (the
/// audible track is always included on top of this).
pub const VIDEO_PREFETCH_DEPTH: usize = 5;

/// Task-registry pressure at which video staging DEFERS instead of spawning
/// (registry cap is 32; keeping slots free guarantees playback-load,
/// heartbeat, scan and discovery tasks never starve behind media work).
pub const VIDEO_STAGE_PRESSURE_LIMIT: usize = 24;

/// How long a current-track worker may wait for the staging store. Speculative
/// work is cancellable and uses cloned store handles, so a longer wait means a
/// broken owner; the worker settles its Loading slot instead of orphaning it.
pub const VIDEO_STAGE_WAIT_CAP_SECS: u64 = 15;

/// Pure plan builder for one pipeline pass: the CURRENT track first (its
/// staging publishes UI events), then up to `depth` upcoming queue ids in
/// order. With no current index, the first `depth` queue entries are the
/// plan. Unit-tested.
pub fn prefetch_plan(queue: &[TrackId], current_index: Option<usize>, depth: usize) -> Vec<TrackId> {
    let mut plan: Vec<TrackId> = Vec::new();
    match current_index {
        Some(index) => {
            if let Some(id) = queue.get(index) { plan.push(id.clone()); }
            plan.extend(queue.iter().skip(index + 1).take(depth).cloned());
        }
        None => plan.extend(queue.iter().take(depth).cloned()),
    }
    plan
}

/// Video plan for the audible identity and its authoritative successors.
/// Focused preview keeps a recommendation outside the physical queue, so its
/// first entry is the audible overlay followed by physical successors.
pub fn audible_video_plan(
    queue: &[TrackId],
    current_index: Option<usize>,
    audible: Option<TrackId>,
    focused_preview: bool,
    depth: usize,
) -> Vec<TrackId> {
    if !focused_preview {
        return prefetch_plan(queue, current_index, depth);
    }
    let mut plan = Vec::new();
    if let Some(audible) = audible { plan.push(audible); }
    if let Some(index) = current_index {
        plan.extend(queue.iter().skip(index.saturating_add(1)).take(depth).cloned());
    }
    plan
}

/// Video plan for an initial recommendation prefetch window. The audible
/// library item remains first so its quick stage and quality upgrade retain
/// admission priority; recommendation ids follow in their temporary-window
/// order even though they are outside the physical playback queue.
pub fn recommendation_video_plan(audible: Option<TrackId>, recommendations: &[TrackId]) -> Vec<TrackId> {
    let mut plan = Vec::with_capacity(recommendations.len() + usize::from(audible.is_some()));
    if let Some(audible) = audible {
        plan.push(audible);
    }
    for recommendation in recommendations {
        if !plan.contains(recommendation) {
            plan.push(recommendation.clone());
        }
    }
    plan
}

impl AppState {
    pub fn new(task_limit: usize) -> Self {
        Self::with_preview_store(
            PreviewAssetStore::new().unwrap_or_else(|_| {
                PreviewAssetStore::at(std::env::temp_dir().join("luno_previews"))
                    .expect("temporary preview store")
            }),
            task_limit,
        )
    }

    /// Constructor with a caller-owned preview store.  Tests and embedders
    /// use this to isolate preview assets instead of the HOME default.
    pub fn with_preview_store(preview_store: PreviewAssetStore, task_limit: usize) -> Self {
        let track_paths: Arc<Mutex<BTreeMap<TrackId, PathBuf>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let artwork_resolver = {
            let track_paths = track_paths.clone();
            move |track_id: &str| {
                track_paths
                    .lock()
                    .ok()
                    .and_then(|paths| paths.get(&TrackId(track_id.to_string())).cloned())
            }
        };
        Self {
            tasks: Arc::new(TaskRegistry::new(task_limit)),
            migration_status: Arc::new(Mutex::new(MigrationStatus::NotRun)),
            library: Arc::new(Mutex::new(LibrarySnapshot {
                version: CONTRACT_VERSION,
                generation: Generation(0),
                tracks: Vec::new(),
                playlists: Vec::new(),
            })),
            discovery: Arc::new(Mutex::new(DiscoveryService::new(None))),
            preview_store: Arc::new(Mutex::new(preview_store)),
            preview_session: Arc::new(Mutex::new(None)),
            preview_seeds: Arc::new(Mutex::new(Vec::new())),
            preview_tasks: Arc::new(Mutex::new(Vec::new())),
            preview_completions: Arc::new(Mutex::new(None)),
            preview_cancelled: Arc::new(AtomicBool::new(false)),
            preview_helper: Arc::new(Mutex::new(None)),
            preview_ffmpeg: Arc::new(Mutex::new(None)),
            discover: Arc::new(Mutex::new(None)),
            discover_seeds: Arc::new(Mutex::new(Vec::new())),
            discover_tasks: Arc::new(Mutex::new(Vec::new())),
            discover_completions: Arc::new(Mutex::new(None)),
            save_in_flight: Arc::new(Mutex::new(std::collections::HashSet::new())),
            pending_download_saves: Arc::new(Mutex::new(BTreeMap::new())),
            roots: Arc::new(Mutex::new(Vec::new())),
            root_statuses: Arc::new(Mutex::new(Vec::new())),
            events: Arc::new(Mutex::new(Vec::new())),
            event_draining: Arc::new(Mutex::new(false)),
            event_in_flight: Arc::new(Mutex::new(0)),
            next_event: Arc::new(AtomicU64::new(0)),
            next_delivery_error: Arc::new(AtomicU64::new(0)),
            next_publish: Arc::new(AtomicU64::new(1)),
            next_generation: Arc::new(AtomicU64::new(0)),
            allocated_events: Arc::new(Mutex::new(BTreeSet::new())),
            event_sink: Arc::new(Mutex::new(None)),
            publication: Arc::new(Mutex::new(())),
            unpublished: Arc::new(Mutex::new(BTreeMap::new())),
            skipped_publications: Arc::new(Mutex::new(BTreeSet::new())),
            admission: Arc::new(Mutex::new(())),
            latest_snapshot: Arc::new(Mutex::new(None)),
            latest_control: Arc::new(Mutex::new(None)),
            replayed_events: Arc::new(Mutex::new(BTreeSet::new())),
            event_retry_stopped: Arc::new(AtomicBool::new(false)),
            shutdown_result: Arc::new(Mutex::new(None)),
            delivery_failure: Arc::new(Mutex::new(None)),
            supervisor: Arc::new(Supervisor::new(ProcessRegistry::new())),
            downloads: Arc::new(DownloadCoordinator::new()),
            download_requests: Arc::new(Mutex::new(BTreeMap::new())),
            bundle_helper: Arc::new(Mutex::new(None)),
            video_root: Arc::new(Mutex::new(None)),
            video_stager: Arc::new(Mutex::new(None)),
            video_current: Arc::new(Mutex::new(None)),
            video_cancel: Arc::new(Mutex::new(Arc::new(AtomicBool::new(false)))),
            video_helper: Arc::new(Mutex::new(None)),
            video_ffmpeg_dir: Arc::new(Mutex::new(None)),
            discovery_configured: Arc::new(AtomicBool::new(false)),
            provider_metadata: Arc::new(Mutex::new(BTreeMap::new())),
            track_paths: track_paths.clone(),
            artwork: Arc::new(TrackArtwork::new(artwork_resolver)),
            current_track: Arc::new(Mutex::new(None)),
            playback: Arc::new(Mutex::new(QueueState::new())),
            clock: Arc::new(Mutex::new(None)),
            recovery: Arc::new(Mutex::new(FailureRecovery::new())),
            favorites: Arc::new(Mutex::new(Favorites::default())),
            recent: Arc::new(Mutex::new(Vec::new())),
            scan: Arc::new(Mutex::new(ScanState {
                active: false,
                pending: None,
            })),
            #[cfg(target_os = "macos")]
            playback_status: Arc::new(Mutex::new(crate::contracts::types::PlaybackStatus::Stopped)),
            #[cfg(target_os = "macos")]
            audio: Arc::new(Mutex::new({
                #[cfg(test)]
                { Box::new(FakeBackend::new()) as Box<dyn AudioBackend + Send> }
                #[cfg(not(test))]
                { Box::new(CpalSymphoniaBackend::new()) as Box<dyn AudioBackend + Send> }
            })),
            #[cfg(target_os = "macos")]
            // TESTS ARE ALWAYS SILENT: the harness initializes volume to 0 so
            // no unit test can ever emit audio through real speakers (a
            // shuffle regression test once blasted 30 s tones at full
            // volume). Production still seeds from persisted settings.
            volume: Arc::new(Mutex::new(if cfg!(test) { 0.0 } else { 1.0 })),
            #[cfg(target_os = "macos")]
            playback_stall: Arc::new(Mutex::new(None)),
            video_mode_enabled: Arc::new(Mutex::new(false)),
            video_prefetch_running: Arc::new(Mutex::new(false)),
            video_staging_inflight: Arc::new(Mutex::new(std::collections::HashSet::new())),
            video_download_progress: Arc::new(crate::video::runtime::VideoProgress::new()),
            video_prefetch_cancel: Arc::new(Mutex::new(Arc::new(AtomicBool::new(false)))),
            video_heavy_work: Arc::new(Mutex::new(())),
            video_upgrade_waiting: Arc::new(AtomicBool::new(false)),
            settings_home: Arc::new(Mutex::new(None)),
            media_keys_enabled: Arc::new(AtomicBool::new(true)),
            #[cfg(target_os = "macos")]
            now_playing_track: Arc::new(Mutex::new(None)),
        }
    }
    pub fn library(&self) -> LibrarySnapshot {
        self.library.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Lazily extracted embedded artwork for a library track, as a `data:`
    /// URL. Returns `Ok(None)` for tracks outside the current snapshot or
    /// without embedded art; extraction failures are silent by design.
    pub fn track_artwork(&self, track_id: &str) -> Result<Option<String>, String> {
        Ok(self.artwork.artwork(track_id))
    }

    pub fn set_discovery_key(&self, key: Option<String>) {
        self.discovery_configured.store(key.is_some(), Ordering::Release);
        self.discovery.lock().unwrap_or_else(|e| e.into_inner()).set_api_key(key);
    }

    /// Whether a Last.fm API key is configured (drives honest settings UI).
    pub fn has_discovery_key(&self) -> bool {
        self.discovery_configured.load(Ordering::Acquire)
    }

    /// Installs the app-lifetime video store + helper binaries. Called once
    /// from Tauri setup; the store lives OUTSIDE the music library root.
    pub fn set_video_runtime(&self, root: PathBuf, ytdlp: Option<PathBuf>, ffmpeg_dir: Option<PathBuf>) {
        let library_root = self.roots.lock().unwrap_or_else(|e| e.into_inner()).first().cloned();
        match crate::video::staging::VideoStage::at(&root, library_root.as_deref()) {
            Ok(stager) => { *self.video_stager.lock().unwrap_or_else(|e| e.into_inner()) = Some(stager); }
            Err(error) => { eprintln!("video storage unavailable at {}: {error:?}", root.display()); }
        }
        *self.video_root.lock().unwrap_or_else(|e| e.into_inner()) = Some(root.clone());
        *self.video_helper.lock().unwrap_or_else(|e| e.into_inner()) = ytdlp.clone();
        *self.video_ffmpeg_dir.lock().unwrap_or_else(|e| e.into_inner()) = ffmpeg_dir.clone();
        eprintln!("[luno-video] runtime ready: store={} yt-dlp={:?} ffmpeg-dir={:?}", root.display(), ytdlp, ffmpeg_dir);
    }

    /// The configured video store root, used by the `luno-video` stream
    /// protocol handler to contain every served path. `None` before setup.
    pub fn video_root_dir(&self) -> Option<PathBuf> {
        self.video_root.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn publish_video_state(&self) {
        if let Some(video) = self.video_current.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            let generation = video.generation;
            self.publish(self.event(generation, Event::VideoState { state: video }));
        }
    }

    /// Resolve this worker's Loading promise when it cannot continue. The
    /// identity check prevents an old cancellation from overwriting a newer
    /// track's slot.
    fn settle_video_loading(&self, track_id: &TrackId, generation: u64) {
        let changed = {
            let mut current = self.video_current.lock().unwrap_or_else(|e| e.into_inner());
            let owns_loading = current.as_ref().is_some_and(|video| {
                video.track_id.as_ref() == Some(track_id)
                    && video.generation.0 == generation
                    && matches!(video.status, crate::contracts::events::VideoStatus::Loading)
            });
            if owns_loading {
                *current = Some(crate::contracts::events::VideoState {
                    track_id: Some(track_id.clone()),
                    source_url: None,
                    staged_path: None,
                    status: crate::contracts::events::VideoStatus::Unavailable,
                    position_ms: 0,
                    download_progress: None,
                    revision: None,
                    fullscreen: false,
                    generation: Generation(generation),
                });
            }
            owns_loading
        };
        if changed { self.publish_video_state(); }
    }

    fn video_snapshot_json(&self) -> Value {
        match self.video_current.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            Some(video) => crate::video::runtime::video_state_json(&video),
            None => serde_json::json!({
                "track_id": null, "source_url": null, "staged_path": null,
                "status": "not_checked", "position_ms": 0, "fullscreen": false,
                "generation": 0,
            }),
        }
    }

    /// VideoSource/VideoStage entry point: fast-path an already-staged track,
    /// otherwise publish `loading` and spawn the resolve+download worker.
    fn video_stage_track(&self, track_id: TrackId) -> Result<Value, ContractError> {
        {
            let current = self.video_current.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(video) = current {
                if video.track_id.as_ref() == Some(&track_id)
                    && matches!(video.status, crate::contracts::events::VideoStatus::Ready | crate::contracts::events::VideoStatus::Playing | crate::contracts::events::VideoStatus::Paused)
                {
                    return Ok(crate::video::runtime::video_state_json(&video));
                }
            }
        }
        let helper = self.video_helper.lock().unwrap_or_else(|e| e.into_inner()).clone().ok_or_else(|| ContractError {
            code: ErrorCode::Unavailable,
            message: "video search is unavailable until the download helper is configured".into(),
            retryable: false,
        })?;
        // Pressure rejection happens BEFORE publishing Loading. A Loading
        // state is a promise that an inflight worker owns it; returning a
        // placeholder without a worker is how the permanent spinner formed.
        if self.tasks.active_count() >= VIDEO_STAGE_PRESSURE_LIMIT {
            let corpse_generation = self.video_current.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|video| {
                let corpse = video.track_id.as_ref() == Some(&track_id)
                    && matches!(video.status, crate::contracts::events::VideoStatus::Loading)
                    && !self.video_staging_inflight.lock().unwrap_or_else(|e| e.into_inner()).contains(&track_id);
                corpse.then_some(video.generation.0)
            });
            if let Some(generation) = corpse_generation {
                self.settle_video_loading(&track_id, generation);
            }
            eprintln!("[luno-video] runtime pressure high ({} tasks); deferring stage for {track_id:?}", self.tasks.active_count());
            return Ok(self.video_snapshot_json());
        }
        let (artist, title) = {
            let snapshot = self.library();
            let track = snapshot.tracks.iter().find(|track| track.id == track_id);
            match track {
                Some(track) => (track.artist.clone(), track.title.clone()),
                None => match self.seed_for(&track_id.0) {
                    Some(seed) => (Some(seed.artist), seed.title),
                    None => return Err(ContractError { code: ErrorCode::NotFound, message: "track metadata is unavailable".into(), retryable: false }),
                },
            }
        };
        eprintln!("[luno-video] stage request {track_id:?} artist={artist:?} title={title:?}");
        // Dedupe concurrent kicks for the SAME track: whoever is already
        // staging it will publish Loading → Ready/Unavailable; piling a second
        // worker on just re-runs the search and races the publication.
        {
            let mut inflight = self.video_staging_inflight.lock().unwrap_or_else(|e| e.into_inner());
            if !inflight.insert(track_id.clone()) {
                eprintln!("[luno-video] stage request {track_id:?} already in flight; coalescing");
                return Ok(self.video_snapshot_json());
            }
        }
        let generation = self.next_generation().0;
        // Cancel any in-flight staging for a previous track.
        self.video_cancel.lock().unwrap_or_else(|e| e.into_inner()).store(true, Ordering::Release);
        let cancel = Arc::new(AtomicBool::new(false));
        *self.video_cancel.lock().unwrap_or_else(|e| e.into_inner()) = Arc::clone(&cancel);
        *self.video_current.lock().unwrap_or_else(|e| e.into_inner()) = Some(crate::contracts::events::VideoState {
            track_id: Some(track_id.clone()),
            source_url: None,
            staged_path: None,
            status: crate::contracts::events::VideoStatus::Loading,
            position_ms: 0,
            download_progress: None,
            revision: None,
            fullscreen: false,
            generation: Generation(generation),
        });
        self.publish_video_state();

        let state = self.clone();
        // The worker gets its own copy of the id so a failed spawn below can
        // still resolve the Loading state we just published.
        let worker_track_id = track_id.clone();
        let progress = Arc::clone(&self.video_download_progress);
        progress.percent.store(0, Ordering::Release);
        progress.started.store(false, Ordering::Release);
        // Progress pump: while the download runs, fold the latest percentage
        // into the published Loading state every 500ms so the UI shows
        // "Downloading video… N%" instead of a static spinner. Exits when
        // this request is superseded or the worker finishes (flag reset).
        let pump_state = state.clone();
        let pump_cancel = Arc::clone(&cancel);
        let pump_progress = Arc::clone(&progress);
        let pump_track_id = worker_track_id.clone();
        let _pump = std::thread::Builder::new().name("luno-video-progress".into()).spawn(move || {
            let mut last_sent: Option<u8> = None;
            loop {
                // 150ms (not 500ms): YouTube's download phase can finish in
                // well under a second on fast links, so a coarse poll often
                // observed only the final value and the ring never showed a
                // real percentage.
                std::thread::sleep(std::time::Duration::from_millis(150));
                if pump_cancel.load(Ordering::Acquire) { break; }
                // Stop when the owning stage settles or another track takes
                // the slot. The old pump only stopped on cancellation, so one
                // sleeping thread leaked after EVERY successful video.
                let still_loading = pump_state.video_current.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
                    .is_some_and(|video| video.track_id.as_ref() == Some(&pump_track_id)
                        && matches!(video.status, crate::contracts::events::VideoStatus::Loading));
                if !still_loading { break; }
                if !pump_progress.started.load(Ordering::Acquire) { continue; }
                let percent = pump_progress.percent.load(Ordering::Acquire);
                if last_sent == Some(percent) { continue; }
                let mut changed = false;
                {
                    let mut current = pump_state.video_current.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(video) = current.as_mut() {
                        if video.track_id.as_ref() == Some(&pump_track_id)
                            && matches!(video.status, crate::contracts::events::VideoStatus::Loading)
                        {
                            video.download_progress = Some(percent);
                            changed = true;
                        }
                    }
                }
                if changed {
                    // Publish OUTSIDE the guard (deadlock rule).
                    pump_state.publish_video_state();
                    last_sent = Some(percent);
                }
            }
        }).ok();
        let spawned = self.submit(self.generation(format!("video-{generation}"), Generation(generation)), move |_token| {
            // CANCELLABLE BOUNDED WAIT (2026-08-25 revision): the original
            // blocking `.lock()` piled an UNCANCELLABLE thread per request and
            // starved the registry; the one-shot try_lock that replaced it
            // over-corrected — a deferred current track had no guaranteed
            // retrier, so videos took minutes. The balance: poll try_lock
            // every 100 ms until the store frees, the request is superseded
            // (cancel flag), shutdown, or the cap. Inflight dedupe keeps this
            // to ONE waiter per track regardless of poll/skip spam.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(VIDEO_STAGE_WAIT_CAP_SECS);
            let mut stager_guard = loop {
                if cancel.load(Ordering::Acquire) || state.is_closing() {
                    state.video_staging_inflight.lock().unwrap_or_else(|e| e.into_inner()).remove(&worker_track_id);
                    state.settle_video_loading(&worker_track_id, generation);
                    return;
                }
                match state.video_stager.try_lock() {
                    Ok(guard) => break guard,
                    Err(_) => {
                        if std::time::Instant::now() >= deadline {
                            state.video_staging_inflight.lock().unwrap_or_else(|e| e.into_inner()).remove(&worker_track_id);
                            state.settle_video_loading(&worker_track_id, generation);
                            eprintln!("[luno-video] store busy past {VIDEO_STAGE_WAIT_CAP_SECS}s cap; settled {worker_track_id:?}");
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            };
            let outcome = match stager_guard.as_mut() {
                Some(stager) => match crate::video::runtime::stage_track(
                    &helper,
                    state.video_ffmpeg_dir.lock().unwrap_or_else(|e| e.into_inner()).as_deref(),
                    stager,
                    artist.as_deref(),
                    &title,
                    &cancel,
                    Some(&progress),
                ) {
                    Ok(outcome) => outcome,
                    Err(error) => crate::video::runtime::VideoOutcome::Unavailable(format!("video staging failed: {error:?}")),
                },
                // Never exit silently — an unpublished worker leaves the
                // UI stuck on "loading" forever.
                None => crate::video::runtime::VideoOutcome::Unavailable("video storage is unavailable".into()),
            };
            let next_status = match &outcome {
                crate::video::runtime::VideoOutcome::Staged { .. } => crate::contracts::events::VideoStatus::Ready,
                crate::video::runtime::VideoOutcome::Unavailable(_) => crate::contracts::events::VideoStatus::Unavailable,
            };
            let source_url = match &outcome {
                crate::video::runtime::VideoOutcome::Staged { source_url, .. } => Some(source_url.clone()),
                crate::video::runtime::VideoOutcome::Unavailable(_) => None,
            };
            let staged_path = match &outcome {
                crate::video::runtime::VideoOutcome::Staged { staged_path, .. } => Some(staged_path.display().to_string()),
                crate::video::runtime::VideoOutcome::Unavailable(_) => None,
            };
            let needs_upgrade = matches!(&outcome, crate::video::runtime::VideoOutcome::Staged { needs_upgrade: true, .. });
            // A newer request superseded this one while we worked. If the
            // slot now belongs to a DIFFERENT track, that request owns the
            // UI — return silently. If OUR track is still published as
            // Loading, we MUST resolve it (Unavailable): a silent return left
            // the song page stuck on "Loading video preview..." forever even
            // though the file had finished downloading.
            let superseded_by_other = {
                let current = state.video_current.lock().unwrap_or_else(|e| e.into_inner());
                current.as_ref().map(|v| v.track_id.as_ref() != Some(&worker_track_id)).unwrap_or(false)
            };
            // This worker is finished either way — release the dedupe slot
            // before any exit path below.
            state.video_staging_inflight.lock().unwrap_or_else(|e| e.into_inner()).remove(&worker_track_id);
            if cancel.load(Ordering::Acquire) {
                if !superseded_by_other {
                    let mut current = state.video_current.lock().unwrap_or_else(|e| e.into_inner());
                    if current.as_ref().map(|v| v.track_id.as_ref() == Some(&worker_track_id) && matches!(v.status, crate::contracts::events::VideoStatus::Loading)).unwrap_or(false) {
                        *current = Some(crate::contracts::events::VideoState {
                            track_id: Some(worker_track_id),
                            source_url: None,
                            staged_path: None,
                            status: crate::contracts::events::VideoStatus::Unavailable,
                            position_ms: 0,
                            download_progress: None,
                            revision: None,
                            fullscreen: false,
                            generation: Generation(generation),
                        });
                        drop(current);
                        state.publish_video_state();
                    }
                }
                return;
            }
            let _ = superseded_by_other;
            // OWNERSHIP GUARD: only publish our result if the slot still
            // belongs to this request — see completion_may_publish.
            if completion_may_publish(state.video_current.lock().unwrap_or_else(|e| e.into_inner()).as_ref(), &worker_track_id, generation) {
                // Clones for the background upgrade BEFORE the move below.
                let upgrade_source = source_url.clone();
                let upgrade_path = staged_path.clone();
                if needs_upgrade {
                    // Advertise the upgrade before publishing Ready. The
                    // successor worker can observe the Ready transition and
                    // must still yield its heavy slot to this upgrade.
                    state.video_upgrade_waiting.store(true, Ordering::Release);
                }
                *state.video_current.lock().unwrap_or_else(|e| e.into_inner()) = Some(crate::contracts::events::VideoState {
                    track_id: Some(worker_track_id.clone()),
                    source_url,
                    staged_path,
                    status: next_status,
                    position_ms: 0,
                    download_progress: None,
                    revision: Some(1),
                    fullscreen: false,
                    generation: Generation(generation),
                });
                state.publish_video_state();
                // BACKGROUND QUALITY UPGRADE (two-tier staging): the file was
                // just pulled through the viewable QUICK ladder for a fast
                // first frame. Replace it with the full ≤1080p version out of band,
                // then republish with revision 2 so the webview hot-swaps onto
                // the upgraded bytes. The shared cancel flag is swapped by the
                // NEXT stage request, so skipping tracks aborts the upgrade at
                // its checkpoints; the ownership check keeps a superseded
                // upgrade from ever touching another request's slot.
                if needs_upgrade {
                    let upgrade_state = state.clone();
                    let upgrade_helper = helper.clone();
                    let upgrade_cancel = Arc::clone(&cancel);
                    let upgrade_track = worker_track_id.clone();
                    let upgrade_generation = generation;
                    let upgrade_waiting = Arc::clone(&state.video_upgrade_waiting);
                    let upgrade_spawned = std::thread::Builder::new().name("luno-video-upgrade".into()).spawn(move || {
                        let _upgrade_wait_guard = VideoUpgradeWaitGuard(upgrade_waiting);
                        // Let the webview open the quick file before the
                        // replacement competes for bandwidth.
                        for _ in 0..4 {
                            if upgrade_cancel.load(Ordering::Acquire) || upgrade_state.is_closing() { return; }
                            std::thread::sleep(std::time::Duration::from_millis(250));
                        }
                        let outcome = {
                            // Resolve the destination under a SHORT lock, then
                            // release the store before the heavy work below.
                            // (The upgrade itself is serialized against
                            // prefetch downloads via video_heavy_work — see the
                            // match arm.)
                            let id = upgrade_path.as_deref()
                                .and_then(|path| std::path::Path::new(path).file_name())
                                .and_then(|name| name.to_str())
                                .and_then(|name| name.strip_prefix("current-"))
                                .and_then(|rest| rest.strip_suffix(".mp4"));
                            let destination = id.and_then(|video_id| {
                                let stager_guard = upgrade_state.video_stager.lock().unwrap_or_else(|e| e.into_inner());
                                stager_guard.as_ref().and_then(|stager| stager.external_destination(crate::video::staging::Slot::Current, video_id).ok())
                            });
                            match (id, destination) {
                                (Some(video_id), Some(destination)) => {
                                    // ONE heavy background job at a time: wait
                                    // for any prefetch download to finish
                                    // before pulling + transcoding the upgrade.
                                    // Polling try_lock (never a blocking lock)
                                    // keeps this thread responsive to the
                                    // cancel flag while it waits.
                                    let heavy = loop {
                                        if upgrade_cancel.load(Ordering::Acquire) || upgrade_state.is_closing() {
                                            return;
                                        }
                                        match upgrade_state.video_heavy_work.try_lock() {
                                            Ok(guard) => break guard,
                                            Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
                                        }
                                    };
                                    let outcome = crate::video::runtime::upgrade_staged_video(
                                        &upgrade_helper,
                                        upgrade_state.video_ffmpeg_dir.lock().unwrap_or_else(|e| e.into_inner()).as_deref(),
                                        &destination,
                                        &upgrade_cancel,
                                    );
                                    drop(heavy);
                                    // The file is now final quality: flip the
                                    // cache-index tier so later plays adopt it
                                    // as done instead of re-running the HD
                                    // pass on every play of a quick-tier entry.
                                    if outcome.is_ok() {
                                        let stager_guard = upgrade_state.video_stager.lock().unwrap_or_else(|e| e.into_inner());
                                        if let Some(stager) = stager_guard.as_ref() {
                                            stager.set_tier_for_video_id(&video_id.to_string(), crate::video::staging::INDEX_TIER_HIGH);
                                        }
                                    }
                                    outcome
                                }
                                (Some(_), None) => Err("video storage is unavailable".into()),
                                (None, _) => Err("staged path has no video id".into()),
                            }
                        };
                        if outcome.is_err() { return; }
                        // Republish ONLY if the slot still belongs to this
                        // exact request (same track AND same generation).
                        let follow_status = match *upgrade_state.playback_status.lock().unwrap_or_else(|e| e.into_inner()) {
                            crate::contracts::types::PlaybackStatus::Paused => crate::contracts::events::VideoStatus::Paused,
                            _ => crate::contracts::events::VideoStatus::Playing,
                        };
                        let next_generation = upgrade_state.next_generation();
                        let mut current = upgrade_state.video_current.lock().unwrap_or_else(|e| e.into_inner());
                        let owns_slot = current.as_ref().map(|v| {
                            v.track_id.as_ref() == Some(&upgrade_track)
                                && v.generation.0 == upgrade_generation
                                && matches!(v.status, crate::contracts::events::VideoStatus::Ready | crate::contracts::events::VideoStatus::Playing | crate::contracts::events::VideoStatus::Paused)
                        }).unwrap_or(false);
                        if owns_slot {
                            *current = Some(crate::contracts::events::VideoState {
                                track_id: Some(upgrade_track),
                                source_url: upgrade_source,
                                staged_path: upgrade_path,
                                status: follow_status,
                                position_ms: 0,
                                download_progress: None,
                                revision: Some(2),
                                fullscreen: false,
                                generation: next_generation,
                            });
                            drop(current);
                            upgrade_state.publish_video_state();
                        }
                    });
                    if upgrade_spawned.is_err() {
                        state.video_upgrade_waiting.store(false, Ordering::Release);
                    }
                }
            }
        });
        if let Err(_spawn_error) = spawned {
            // A rejected spawn means NO worker will ever resolve the Loading
            // state published above — the UI would sit on "Loading video
            // preview..." forever. Settle it as Unavailable before surfacing
            // the busy error. The dedupe slot is released here too, since its
            // worker will never run.
            self.video_staging_inflight.lock().unwrap_or_else(|e| e.into_inner()).remove(&track_id);
            *self.video_current.lock().unwrap_or_else(|e| e.into_inner()) = Some(crate::contracts::events::VideoState {
                track_id: Some(track_id),
                source_url: None,
                staged_path: None,
                status: crate::contracts::events::VideoStatus::Unavailable,
                position_ms: 0,
                download_progress: None,
                revision: None,
                fullscreen: false,
                generation: Generation(generation),
            });
            self.publish_video_state();
        }
        Ok(self.video_snapshot_json())
    }

    /// Song-page video mode toggle: flips the persisted flag and, when
    /// enabling, immediately stages the playing track plus prefetches the
    /// next five queue videos. Disabling cancels any prefetch in flight.
    fn set_video_mode(&self, enabled: bool) -> Result<Value, ContractError> {
        *self.video_mode_enabled.lock().unwrap_or_else(|e| e.into_inner()) = enabled;
        if let Some(home) = self.settings_home.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            let settings = crate::contracts::types::SettingsInput {
                music_root: self.scan_roots().first().map(|path| path.display().to_string()),
                auto_discovery: None,
                volume: None,
                sleep_timer: None,
                global_keys: None,
                favorite_songs: self.favorites.lock().unwrap_or_else(|e| e.into_inner()).paths().map(str::to_owned).collect(),
                video_mode: Some(enabled),
            };
            if let Err(error) = self.save_settings(home, settings) {
                eprintln!("[luno-video] video_mode persistence failed: {:?}", error.message);
            }
        }
        if !enabled {
            // Stop an in-flight prefetch loop; the current download is left
            // alone (it holds its own cancel flag).
            self.video_prefetch_cancel.lock().unwrap_or_else(|e| e.into_inner()).store(true, Ordering::Release);
            return Ok(serde_json::json!({ "video_mode": false }));
        }
        self.kick_video_pipeline();
        Ok(serde_json::json!({ "video_mode": true }))
    }

    pub fn video_mode_enabled(&self) -> bool {
        *self.video_mode_enabled.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether `track`'s video slot is alive: resolved, or Loading WITH a
    /// live worker behind it. A Loading slot WITHOUT an inflight marker is a
    /// corpse — its worker was cancelled by a supersede (e.g. the old
    /// double-kick-per-switch race) and nothing would ever resolve it, which
    /// read as "the paused song's video never loads". Such slots must be
    /// restaged, not skipped.
    fn current_video_live(&self, track: &TrackId) -> bool {
        let current = self.video_current.lock().unwrap_or_else(|e| e.into_inner());
        let Some(video) = current.as_ref() else { return false; };
        if video.track_id.as_ref() != Some(track) { return false; }
        match video.status {
            crate::contracts::events::VideoStatus::Ready
            | crate::contracts::events::VideoStatus::Playing
            | crate::contracts::events::VideoStatus::Paused => true,
            crate::contracts::events::VideoStatus::Loading => {
                self.video_staging_inflight.lock().unwrap_or_else(|e| e.into_inner()).contains(track)
            }
            _ => false,
        }
    }

    /// True while `track`'s video slot is Loading. Used to yield speculative
    /// prefetch to the current track's own worker (current-first priority).
    fn video_current_loading(&self, track: &TrackId) -> bool {
        self.video_current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|video| {
                video.track_id.as_ref() == Some(track)
                    && matches!(video.status, crate::contracts::events::VideoStatus::Loading)
            })
    }

    /// Video-mode core: stage the audible track, then sequentially prepare
    /// quick-tier video for the next [`VIDEO_PREFETCH_DEPTH`] tracks from the
    /// authoritative context. Focused preview uses physical successors;
    /// Discover uses its projected recommendation queue. An explicit initial
    /// recommendation preparation pass also runs while song-page video mode
    /// is disabled: its temporary five-item assets are part of preparation,
    /// rather than ordinary physical-queue video mode.
    pub fn kick_video_pipeline(&self) {
        let video_mode_enabled = self.video_mode_enabled();
        let preview_recommendations = {
            let session = self.preview_session.lock().unwrap_or_else(|e| e.into_inner());
            session
                .as_ref()
                .filter(|session| session.is_active() && session.is_prefetch())
                .map(|session| session.window().to_vec())
        };
        if !video_mode_enabled && preview_recommendations.is_none() { return; }
        let preview_pending = preview_recommendations.as_ref().is_some_and(|recommendations| {
            let session = self.preview_session.lock().unwrap_or_else(|e| e.into_inner());
            session.as_ref().is_some_and(|session| {
                recommendations.iter().any(|key| {
                    !session.is_prepared(key) && !session.unavailable().contains(key)
                })
            })
        });
        if preview_pending {
            // A fresh recommendation window supersedes any older successor
            // pass, while the new pass below starts the audible item and the
            // temporary recommendation videos together.
            self.video_prefetch_cancel.lock().unwrap_or_else(|e| e.into_inner()).store(true, Ordering::Release);
        }
        let helper = match self.video_helper.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            Some(helper) => helper,
            None => return,
        };
        // SUPERSEDE FIRST: a track switch must never be gated on a previous
        // pass that can sit inside a yt-dlp download for minutes. Cancel +
        // install a fresh flag BEFORE any running-gate logic; an old worker
        // exits at its next checkpoint (every loop iteration checks these).
        self.video_prefetch_cancel.lock().unwrap_or_else(|e| e.into_inner()).store(true, Ordering::Release);
        let cancel = Arc::new(AtomicBool::new(false));
        *self.video_prefetch_cancel.lock().unwrap_or_else(|e| e.into_inner()) = Arc::clone(&cancel);
        *self.video_prefetch_running.lock().unwrap_or_else(|e| e.into_inner()) = true;

        let (current, queue) = {
            let playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
            (playback.current_index(), playback.queue().to_vec())
        };
        let audible = self.current_track.lock().unwrap_or_else(|e| e.into_inner()).clone()
            .or_else(|| current.and_then(|index| queue.get(index).cloned()));
        let focused_preview = self
            .preview_session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|session| session.is_active() && !session.is_prefetch())
            && !self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|mode| mode.is_active());
        // Track the CURRENT id so speculative prefetch can yield the staging
        // lock to it (below): otherwise a prefetch download can win the lock
        // and the current video spins with no progress until the prefetch ends.
        let current_id = audible.clone();
        let library = self.library();
        let resolve = |track_id: &TrackId| -> Option<(String, Option<String>)> {
            library
                .tracks
                .iter()
                .find(|track| track.id == *track_id)
                .map(|track| (track.title.clone(), track.artist.clone()))
                .or_else(|| self.seed_for(&track_id.0).map(|seed| (seed.title, Some(seed.artist))))
        };
        let plan = preview_recommendations
            .as_deref()
            // Recommendation preparation owns its five temporary videos even
            // with song-page video mode off. The audible library track remains
            // governed by that setting and must not be downloaded as a side
            // effect of preparing recommendations.
            .map(|recommendations| recommendation_video_plan(if video_mode_enabled { audible.clone() } else { None }, recommendations))
            .unwrap_or_else(|| audible_video_plan(&queue, current, audible, focused_preview, VIDEO_PREFETCH_DEPTH));
        let plans: Vec<(TrackId, String, Option<String>)> = plan
            .into_iter()
            .filter_map(|id| resolve(&id).map(|(title, artist)| (id, title, artist)))
            .collect();

        let state = self.clone();
        let cancel_error_path = Arc::clone(&cancel);
        let spawned = self.submit(self.generation("video-prefetch", self.next_generation()), move |_token| {
            for (track_id, title, artist) in plans {
                if cancel.load(Ordering::Acquire) || state.is_closing() { break; }
                if state.video_mode_changed_since(&cancel) { break; }
                // Skip only when the slot is ALIVE (resolved, or Loading with
                // a live worker). A Loading corpse — superseded worker, no
                // inflight marker — must fall through and restage, or the
                // track sits on "Loading video preview..." until some other
                // trigger happens to run (the paused-song video complaint).
                if state.current_video_live(&track_id) {
                    continue;
                }
                // CURRENT slot work must go through the same machinery as a
                // manual request so the UI sees Loading → Ready events.
                let is_current = current_id.as_ref() == Some(&track_id);
                if is_current {
                    let _ = state.video_stage_track(track_id.clone());
                } else {
                    // Current-first priority: do not steal the staging lock
                    // while the CURRENT track's video is still Loading. Its
                    // worker is the one publishing the live percentage; a
                    // speculative prefetch winning the lock left the spinner
                    // with no number until the prefetch finished.
                    if let Some(current_id) = &current_id {
                        while state.video_current_loading(current_id) {
                            if cancel.load(Ordering::Acquire)
                                || state.is_closing()
                                || state.video_mode_changed_since(&cancel)
                            {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                    }
                    // Next-slot prefetch must NOT block on video_stager: a
                    // staging worker holds that lock for its whole download,
                    // and blocking here parked the pass (deaf to cancellation)
                    // for minutes. Poll the lock with cancellation checkpoints.
                    let stager_snapshot = loop {
                        if cancel.load(Ordering::Acquire)
                            || state.is_closing()
                            || state.video_mode_changed_since(&cancel)
                        {
                            break None;
                        }
                        match state.video_stager.try_lock() {
                            Ok(guard) => break Some(guard.clone()),
                            Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
                        }
                    };
                    if let Some(Some(stager)) = stager_snapshot {
                        // ONE heavy background job at a time: wait here while a
                        // quality-upgrade transcode runs (and vice versa).
                        // Polling try_lock keeps this worker responsive to
                        // cancellation — a blocking `.lock()` would park it
                        // deaf to track switches for the whole wait.
                        let heavy = loop {
                            if cancel.load(Ordering::Acquire)
                                || state.is_closing()
                                || state.video_mode_changed_since(&cancel)
                            {
                                break None;
                            }
                            // The current track's high-tier replacement has
                            // already published its quick file and is waiting
                            // for this same heavy-work mutex. Keep successors
                            // out until that upgrade either completes or is
                            // cancelled by a track switch.
                            if state.video_upgrade_waiting.load(Ordering::Acquire) {
                                std::thread::sleep(std::time::Duration::from_millis(100));
                                continue;
                            }
                            match state.video_heavy_work.try_lock() {
                                Ok(guard) => break Some(guard),
                                Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
                            }
                        };
                        if let Some(_heavy_guard) = heavy {
                            crate::video::runtime::prefetch_track(
                                &helper,
                                state.video_ffmpeg_dir.lock().unwrap_or_else(|e| e.into_inner()).as_deref(),
                                &stager,
                                artist.as_deref(),
                                &title,
                                &cancel,
                            );
                        }
                    }
                }
            }
            // Reset the gate ONLY when this pass is still the installed one —
            // a superseded worker must never clear a newer pass's flag.
            if !state.video_mode_changed_since(&cancel) {
                *state.video_prefetch_running.lock().unwrap_or_else(|e| e.into_inner()) = false;
            }
        });
        if spawned.is_err() {
            if !self.video_mode_changed_since(&cancel_error_path) {
                *self.video_prefetch_running.lock().unwrap_or_else(|e| e.into_inner()) = false;
            }
        }
    }

    fn video_mode_changed_since(&self, cancel: &Arc<AtomicBool>) -> bool {
        // The orchestrator swaps in a fresh flag whenever it re-runs; holding
        // a stale flag means this pass was superseded.
        !Arc::ptr_eq(&self.video_prefetch_cancel.lock().unwrap_or_else(|e| e.into_inner()), cancel)
    }

    fn video_mutate(&self, mutate: impl FnOnce(&mut crate::contracts::events::VideoState)) -> Result<Value, ContractError> {        let mut guard = self.video_current.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_mut() {
            Some(video) if video.staged_path.is_some() || matches!(video.status, crate::contracts::events::VideoStatus::Paused | crate::contracts::events::VideoStatus::Playing) => {
                mutate(video);
                Ok(crate::video::runtime::video_state_json(video))
            }
            _ => Err(ContractError { code: ErrorCode::NotFound, message: "no active video".into(), retryable: false }),
        }
    }

    /// Resolves the binaries the preview worker shells out to.  The helper
    /// follows the downloader's process contract; ffmpeg is the bundled
    /// LGPL build or a PATH fallback.
    pub fn set_preview_toolchain(&self, helper: Option<PathBuf>, ffmpeg: Option<PathBuf>) {
        *self.preview_helper.lock().unwrap_or_else(|e| e.into_inner()) = helper;
        *self.preview_ffmpeg.lock().unwrap_or_else(|e| e.into_inner()) = ffmpeg;
    }

    /// Removes every published preview asset.  The preview directory is
    /// disposable by design: nothing outside it references these files.
    pub fn cleanup_previews(&self) {
        let _ = self.preview_store.lock().unwrap_or_else(|e| e.into_inner()).cleanup_session(None);
    }

    /// Library tracks expressed as discovery recommendations for
    /// local-result filtering (per AC-DISC-02: filter local and duplicate
    /// results before delivery).
    fn library_recommendations(&self) -> Vec<Recommendation> {
        self.library()
            .tracks
            .into_iter()
            .filter_map(|track| {
                let title = track.title.trim().to_owned();
                let artist = track.artist.unwrap_or_default().trim().to_owned();
                if title.is_empty() {
                    return None;
                }
                Some(Recommendation {
                    artist,
                    title,
                    match_score: 0.0,
                    artwork_url: None,
                })
            })
            .collect()
    }

    fn preview_ids(seeds: &[PreviewSeed]) -> Result<Vec<TrackId>, ContractError> {
        seeds
            .iter()
            .map(|seed| {
                let key = crate::preview::assets::safe_key(&format!("{}-{}", seed.artist.trim(), seed.title.trim()))
                    .map_err(|error| ContractError::invalid(format!("{error:?}")))?;
                TrackId::new(key)
            })
            .collect()
    }

    /// Begins (or restarts) a preview session around the tapped
    /// recommendation, captures the physical playback queue, raises the
    /// prefetch window, and reports the session projection.
    fn preview_tap(&self, seeds: &[PreviewSeed], key: &str) -> Result<Value, ContractError> {
        if seeds.is_empty() || seeds.len() > MAX_PREVIEW_SEEDS {
            return Err(ContractError::invalid("preview seeds must contain between 1 and 50 items"));
        }
        let ids = Self::preview_ids(seeds)?;
        let tapped = TrackId::new(key)?;
        if !ids.iter().any(|id| id == &tapped) {
            return Err(ContractError::invalid("tapped preview key is not among the seeds"));
        }
        // A tap is an explicit handoff from the rolling discover queue to a
        // focused preview session. Restore the physical queue first so the
        // preview session never captures temporary recommendation IDs as its
        // supposed library queue.
        let discover_active = self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|mode| mode.is_active());
        if discover_active {
            self.handoff_discover_to_preview();
        }
        let (queue, index) = {
            let playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
            let queue = playback.queue().to_vec();
            if queue.is_empty() {
                return Err(ContractError {
                    code: ErrorCode::Unavailable,
                    message: "a preview session requires an active playback queue".into(),
                    retryable: false,
                });
            }
            (queue, playback.current_index().unwrap_or(0))
        };
        self.cancel_preview_tasks();
        let generation: u64 = self.next_generation().0;
        let session = PreviewSession::start(generation, queue, index, ids, &tapped)
            .ok_or_else(|| ContractError::invalid("preview session could not start"))?;
        *self.preview_session.lock().unwrap_or_else(|e| e.into_inner()) = Some(session);
        self.video_prefetch_cancel.lock().unwrap_or_else(|e| e.into_inner()).store(true, Ordering::Release);
        *self.preview_seeds.lock().unwrap_or_else(|e| e.into_inner()) = seeds.to_vec();
        self.preview_cancelled.store(false, Ordering::Release);
        self.spawn_preview_driver(generation);
        // Adopt cached files before publishing the entry snapshot. A ready
        // row must never flash back to "Finding a playable source" merely
        // because the user left and re-entered focused playback.
        self.adopt_existing_preview_window(generation);
        self.ensure_window_prepared(generation);
        self.preview_snapshot_value()
    }

    /// Stages the first recommendation window without entering discover mode,
    /// changing the playback queue, or starting audio.
    fn preview_prefetch(&self, seeds: &[PreviewSeed]) -> Result<Value, ContractError> {
        if seeds.is_empty() || seeds.len() > MAX_PREVIEW_SEEDS {
            return Err(ContractError::invalid("preview seeds must contain between 1 and 50 items"));
        }
        let ids = Self::preview_ids(seeds)?;
        let (queue, index) = {
            let playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
            if playback.current().is_none() {
                return Err(ContractError { code: ErrorCode::Unavailable, message: "preview prefetch needs a playing track".into(), retryable: true });
            }
            (playback.queue().to_vec(), playback.current_index().unwrap_or(0))
        };
        let (had_session, active_session, active_prefetch, same_recommendations) = self
            .preview_session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|session| (
                true,
                session.is_active(),
                session.is_active() && session.is_prefetch(),
                session.recommendations() == ids.as_slice(),
            ))
            .unwrap_or((false, false, false, false));
        if active_session && !active_prefetch {
            return self.preview_snapshot_value();
        }
        // React can re-issue prefetch when the route changes while the audible
        // seed stays the same. The existing session already owns those assets;
        // restarting it used to delete the completed files and begin again.
        if active_prefetch && same_recommendations {
            return self.preview_snapshot_value();
        }
        self.cancel_preview_tasks();
        if had_session && !same_recommendations {
            // A genuinely different seed replaces the bounded temporary
            // cache. Mode changes for the same seed keep completed files.
            let _ = self.preview_store.lock().unwrap_or_else(|e| e.into_inner()).cleanup_session(None);
        }
        let generation = self.next_generation().0;
        let session = PreviewSession::start_prefetch(generation, queue, index, ids)
            .ok_or_else(|| ContractError::invalid("preview prefetch could not start"))?;
        *self.preview_session.lock().unwrap_or_else(|e| e.into_inner()) = Some(session);
        self.video_prefetch_cancel.lock().unwrap_or_else(|e| e.into_inner()).store(true, Ordering::Release);
        *self.preview_seeds.lock().unwrap_or_else(|e| e.into_inner()) = seeds.to_vec();
        self.preview_cancelled.store(false, Ordering::Release);
        self.spawn_preview_driver(generation);
        self.adopt_existing_preview_window(generation);
        self.ensure_window_prepared(generation);
        // Audio preparation is serialized by the preview driver, while the
        // video pass can fetch the same five-item window independently. This
        // starts both facets together so a recommendation can be fully ready
        // before the user taps it.
        self.kick_video_pipeline();
        self.preview_snapshot_value()
    }

    fn preview_step(&self, forward: bool) -> Result<Value, ContractError> {
        let generation = {
            let session = self.preview_session.lock().unwrap_or_else(|e| e.into_inner());
            let Some(session) = session.as_ref() else {
                return Err(ContractError {
                    code: ErrorCode::NotFound,
                    message: "no active preview session".into(),
                    retryable: false,
                });
            };
            if !session.is_active() {
                return Err(ContractError {
                    code: ErrorCode::NotFound,
                    message: "no active preview session".into(),
                    retryable: false,
                });
            }
            session.generation()
        };
        let action = {
            let mut session = self.preview_session.lock().unwrap_or_else(|e| e.into_inner());
            let session = session.as_mut().expect("guarded above");
            if forward { session.next() } else { session.previous() }
        };
        match action {
            SessionAction::Play(_) | SessionAction::Wait(_) => {
                self.ensure_current_prepared(generation);
                self.publish_preview_snapshot(generation);
                self.preview_snapshot_value()
            }
            action @ (SessionAction::LeaveToNext { .. } | SessionAction::ReturnToSeed { .. } | SessionAction::Ended) => {
                self.handle_session_action(action, generation);
                self.publish_preview_snapshot(generation);
                self.preview_snapshot_value()
            }
        }
    }

    fn preview_cancel(&self) -> Result<Value, ContractError> {
        let (generation, handoff) = {
            let mut session = self.preview_session.lock().unwrap_or_else(|e| e.into_inner());
            let Some(session) = session.as_mut() else {
                return Err(ContractError {
                    code: ErrorCode::NotFound,
                    message: "no active preview session".into(),
                    retryable: false,
                });
            };
            let handoff = session.invalidate(self.next_generation().0);
            (session.generation(), handoff)
        };
        self.preview_cancelled.store(true, Ordering::Release);
        self.tasks.cancel_key("preview-heartbeat");
        self.cancel_preview_tasks();
        self.restore_preview_queue(handoff.queue, handoff.index);
        self.publish_preview_snapshot(generation);
        self.preview_snapshot_value()
    }

    /// The session driver applies worker completions while the session is
    /// still current and publishes a snapshot after every transition.
    fn spawn_preview_driver(&self, generation: u64) {
        let (tx, rx) = std::sync::mpsc::channel::<PreviewCompletion>();
        *self.preview_completions.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        let state = self.clone();
        let session = self.preview_session.clone();
        let key = format!("preview-driver-{generation}");
        self.preview_tasks.lock().unwrap_or_else(|e| e.into_inner()).push(key.clone());
        let spec = self.generation(key, Generation(generation));
        let submitted = self.submit(spec, move |task_token| {
            while !state.is_closing() && !task_token.is_cancelled() {
                let current = session.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.is_active() && s.generation() == generation);
                if !current { break; }
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(PreviewCompletion::Prepared { key, path }) => {
                        state.finish_preview_task(generation, &key);
                        let action = session.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(|s| s.prepared_asset(generation, TrackId(key.clone()), path)).unwrap_or(SessionAction::Ended);
                        state.publish_preview_snapshot(generation);
                        state.handle_session_action(action, generation);
                        state.ensure_window_prepared(generation);
                        // The initial prefetch pass already owns the whole
                        // recommendation window. Re-kicking it after every
                        // serialized audio completion would cancel whichever
                        // successor video is downloading and restart it from
                        // the first item. Focused sessions still re-kick so a
                        // tapped item can stage its audible overlay.
                        let prefetch_active = session.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.is_active() && s.is_prefetch());
                        if !prefetch_active { state.kick_video_pipeline(); }
                    }
                    Ok(PreviewCompletion::Progress { key, percent }) => {
                        if let Some(session) = session.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                            session.set_progress(generation, TrackId(key), percent);
                        }
                        state.publish_preview_snapshot(generation);
                    }
                    Ok(PreviewCompletion::Failed { key, error }) => {
                        state.finish_preview_task(generation, &key);
                        eprintln!("[luno-preview] preparation failed for {key}: {error}");
                        let id = TrackId::new(key.clone()).unwrap_or_else(|_| TrackId(key.clone()));
                        let action = session.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(|s| s.failed_asset(generation, &id)).unwrap_or(SessionAction::Ended);
                        // Per-row truth: the preview snapshot's `unavailable`
                        // list drives "Preview unavailable" on the exact row.
                        // A global error event added nothing and, because the
                        // rolling prefetch retries every seed, it stacked an
                        // undismissable banner on every failed recommendation
                        // ("preview could not be prepared").
                        state.publish_preview_snapshot(generation);
                        state.handle_session_action(action, generation);
                        state.ensure_window_prepared(generation);
                        let prefetch_active = session.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.is_active() && s.is_prefetch());
                        if !prefetch_active { state.kick_video_pipeline(); }
                    }
                    Err(_) => {}
                }
            }
        });
        if submitted.is_err() {
            self.preview_tasks.lock().unwrap_or_else(|e| e.into_inner()).retain(|task| task != &format!("preview-driver-{generation}"));
            self.publish_preview_snapshot(generation);
        }
    }

    fn finish_preview_task(&self, generation: u64, key: &str) {
        let task_key = format!("preview-{generation}-{key}");
        self.preview_tasks.lock().unwrap_or_else(|e| e.into_inner()).retain(|task| task != &task_key);
    }

    fn handle_session_action(&self, action: SessionAction, generation: u64) {
        match action {
            SessionAction::Play(key) => {
                if let Ok(path) = self.preview_store.lock().unwrap_or_else(|e| e.into_inner()).ready_path(&key.0) {
                    self.play_preview_audio(key, path, generation);
                }
            }
            SessionAction::Wait(key) => {
                let prepared = self.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|session| session.is_prepared(&key));
                if !prepared { self.ensure_prepared(generation, &key); }
            }
            SessionAction::LeaveToNext { queue, index } => {
                self.tasks.cancel_key("preview-heartbeat");
                self.cancel_preview_tasks();
                self.restore_preview_queue(queue, index);
            }
            SessionAction::ReturnToSeed { queue, index } => {
                self.tasks.cancel_key("preview-heartbeat");
                self.cancel_preview_tasks();
                self.restore_preview_queue(queue, Some(index));
            }
            SessionAction::Ended => {
                self.tasks.cancel_key("preview-heartbeat");
                self.cancel_preview_tasks();
            }
        }
    }

    fn restore_preview_queue(&self, queue: Vec<TrackId>, index: Option<usize>) {
        if let Some(index) = index {
            self.playback.lock().unwrap_or_else(|e| e.into_inner()).replace(queue, index);
        } else {
            self.playback.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
        self.publish_queue_snapshot();
        #[cfg(target_os = "macos")]
        if index.is_some() { let _ = self.start_current_track(); }
    }

    /// A preview asset that could not be loaded/played must stop the transport
    /// from reporting the (silent) recommendation as playing.
    #[cfg(target_os = "macos")]
    fn publish_preview_failure_status(&self) {
        if let Ok(mut backend) = self.audio.lock() {
            let _ = backend.stop();
        }
        if let Some(clock) = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            clock.stop();
        }
        *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) =
            crate::contracts::types::PlaybackStatus::Stopped;
        *self.current_track.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let snapshot = self.playback_snapshot(
            crate::contracts::types::PlaybackStatus::Stopped,
            Duration::ZERO,
            None,
        );
        self.publish_playback(snapshot, None);
    }

    #[cfg(target_os = "macos")]
    fn play_preview_audio(&self, key: TrackId, path: PathBuf, generation: u64) {
        self.tasks.cancel_key("playback-heartbeat");
        self.tasks.cancel_key("preview-heartbeat");
        // The preview session is authoritative while it owns the output.
        // Stop the library stream before the async load starts; otherwise the
        // UI can report the prepared preview as current while the old stream
        // continues playing during the load window.
        if let Ok(mut backend) = self.audio.lock() { let _ = backend.stop(); }
        let state = self.clone();
        let task_key = format!("preview-audio-{generation}");
        self.preview_tasks.lock().unwrap_or_else(|e| e.into_inner()).push(task_key.clone());
        let _ = self.submit(self.generation(task_key, Generation(generation)), move |token| {
            if token.is_cancelled() { return; }
            let volume = *state.volume.lock().unwrap_or_else(|e| e.into_inner());
            let loaded = {
                let mut backend = match state.audio.lock() { Ok(backend) => backend, Err(_) => return };
                match backend.load_streaming(&path) {
                    Ok(info) => { backend.set_volume(volume); info }
                    Err(error) => {
                        eprintln!("[luno-preview] audio load failed for {}: {error:?}", key.0);
                        drop(backend);
                        // Truthful failure: mark the asset unavailable so the row
                        // reads "Preview unavailable" instead of sticking on
                        // "Previewing now" with a frozen footer tracker.
                        let action = state.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(|s| s.failed_asset(generation, &key)).unwrap_or(SessionAction::Ended);
                        // Per-row failure only; the global error stream is for
                        // commands, not a retrying autoplay pipeline. The
                        // transport must also stop claiming it is playing: the
                        // stream was stopped before the load attempt.
                        state.publish_preview_snapshot(generation);
                        state.publish_preview_failure_status();
                        state.handle_session_action(action, generation);
                        return;
                    }
                }
            };
            if token.is_cancelled() { return; }
            {
                let mut backend = match state.audio.lock() { Ok(backend) => backend, Err(_) => return };
                if let Err(error) = backend.play() {
                    eprintln!("[luno-preview] audio play failed for {}: {error:?}", key.0);
                    drop(backend);
                    let action = state.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(|s| s.failed_asset(generation, &key)).unwrap_or(SessionAction::Ended);
                    state.publish_preview_snapshot(generation);
                    state.publish_preview_failure_status();
                    state.handle_session_action(action, generation);
                    return;
                }
            }
            *state.clock.lock().unwrap_or_else(|e| e.into_inner()) = Some(PlaybackClock::new(loaded.duration));
            if let Some(clock) = state.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() { clock.start(0.0); }
            *state.current_track.lock().unwrap_or_else(|e| e.into_inner()) = Some(key.clone());
            *state.playback_status.lock().unwrap_or_else(|e| e.into_inner()) = crate::contracts::types::PlaybackStatus::Playing;
            state.record_recent(key.clone());
            // Publish immediately so the footer leaves its stale library
            // position instead of sticking, then heartbeat the preview
            // position the same way library playback does.
            let snapshot = state.playback_snapshot(
                crate::contracts::types::PlaybackStatus::Playing,
                Duration::ZERO,
                loaded.duration,
            );
            state.publish_playback(snapshot, Some(key.clone()));
            let heartbeat_state = state.clone();
            let heartbeat_key = key.clone();
            let heartbeat_worker = heartbeat_state.clone();
            let spec = heartbeat_state.generation("preview-heartbeat", Generation(generation));
            let _ = heartbeat_state.submit(spec, move |heartbeat_token| {
                while !heartbeat_token.is_cancelled() && !heartbeat_worker.is_closing() {
                    std::thread::sleep(crate::playback::clock::HEARTBEAT);
                    if heartbeat_token.is_cancelled() || heartbeat_worker.is_closing() { break; }
                    // Stop when the session moved on: another preview (or a
                    // library resume) owns the output now.
                    let still_current = heartbeat_worker.current_track.lock().unwrap_or_else(|e| e.into_inner()).as_ref() == Some(&heartbeat_key);
                    if !still_current { break; }
                    let (position, duration) = {
                        let backend = match heartbeat_worker.audio.lock() { Ok(backend) => backend, Err(_) => break };
                        let position = match backend.position() { Ok(position) => position, Err(_) => break };
                        let duration = backend.duration().ok().flatten();
                        (position, duration)
                    };
                    let status = *heartbeat_worker.playback_status.lock().unwrap_or_else(|e| e.into_inner());
                    let due = heartbeat_worker.clock.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|clock| clock.completion_due(position, true, position));
                    if due && heartbeat_worker.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut().is_some_and(|clock| clock.dispatch_completion()) {
                        // Focused preview is one audible overlay. Natural
                        // completion follows the same physical Next policy as
                        // the global transport; only Discover mode autoplays
                        // recommendation successors.
                        let _ = heartbeat_worker.next_track();
                        break;
                    }
                    let snapshot = heartbeat_worker.playback_snapshot(status, position, duration);
                    heartbeat_worker.publish_playback(snapshot, None);
                }
            });
        });
    }

    #[cfg(not(target_os = "macos"))]
    fn play_preview_audio(&self, _key: TrackId, _path: PathBuf, _generation: u64) {}

    fn ensure_window_prepared(&self, generation: u64) {
        let keys = {
            let session = self.preview_session.lock().unwrap_or_else(|e| e.into_inner());
            session.as_ref().filter(|s| s.is_active()).map(|s| s.window().to_vec()).unwrap_or_default()
        };
        let raised = self.preview_tasks.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for key in keys {
            let prepared = self.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.is_prepared(&key));
            let task_key = format!("preview-{generation}-{}", key.0);
            if !prepared && !raised.iter().any(|task| task == &task_key) {
                // One yt-dlp helper at a time. Launching the whole six-item
                // window concurrently exhausted the bundled executable and
                // made the tapped item lose the race to its own prefetches.
                self.ensure_prepared(generation, &key);
                break;
            }
        }
    }

    fn adopt_existing_preview_window(&self, generation: u64) {
        let keys = self.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|session| session.window().to_vec()).unwrap_or_default();
        for key in keys {
            let path = self.preview_store.lock().unwrap_or_else(|e| e.into_inner()).ready_path(&key.0).ok().filter(|path| path.is_file());
            let Some(path) = path else { continue };
            let action = self.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(|session| session.prepared_asset(generation, key, path)).unwrap_or(SessionAction::Ended);
            self.handle_session_action(action, generation);
        }
        self.publish_preview_snapshot(generation);
    }

    fn ensure_current_prepared(&self, generation: u64) {
        let current = { self.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|s| s.current().cloned()) };
        if let Some(key) = current {
            let prepared = self.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.is_prepared(&key));
            if !prepared { self.ensure_prepared(generation, &key); }
        }
    }

    fn ensure_prepared(&self, generation: u64, key: &TrackId) {
        // A worker may have published the file just as a focused tap or a
        // next/previous action invalidated its old session. Re-adopt that
        // atomically published asset before raising another helper. Without
        // this check a ready recommendation was downloaded a second time.
        let existing = self
            .preview_store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ready_path(&key.0)
            .ok()
            .filter(|path| path.is_file());
        if let Some(path) = existing {
            let action = self
                .preview_session
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_mut()
                .map(|session| session.prepared_asset(generation, key.clone(), path))
                .unwrap_or(SessionAction::Ended);
            self.handle_session_action(action, generation);
            self.publish_preview_snapshot(generation);
            return;
        }
        let Some(query) = self.seed_for(&key.0) else { return };
        let query = format!("{} {}", query.artist.trim(), query.title.trim());
        let task_key = format!("preview-{generation}-{}", key.0);
        {
            let mut raised = self.preview_tasks.lock().unwrap_or_else(|e| e.into_inner());
            if raised.iter().any(|task| task == &task_key) { return; }
            raised.push(task_key.clone());
        }
        let state = self.clone();
        let store = self.preview_store.clone();
        // The store and session generations are independent counters; the
        // store's own download() gates publishes against its generation, so
        // tasks capture the store generation at raise time.
        let store_generation = self.preview_store.lock().unwrap_or_else(|e| e.into_inner()).current_generation();
        let helper = self.preview_helper.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let ffmpeg = self.preview_ffmpeg.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let key_for_task = key.clone();
        let generation_capture = generation;
        let sender = self.preview_completions.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let spec = self.generation(task_key.clone(), Generation(generation));
        let task_key_for_worker = task_key.clone();
        let submitted = self.submit(spec, move |task_token| {
            let progress_sender = sender.clone();
            let progress_key = key_for_task.0.clone();
            let progress = std::sync::Arc::new(move |percent| { if let Some(sender) = &progress_sender { let _ = sender.send(PreviewCompletion::Progress { key: progress_key.clone(), percent }); } });
            let completion = prepare_one(&store, store_generation, &key_for_task, &query, &helper, &ffmpeg, &task_token, progress);
            if let Some(sender) = sender {
                let current = state
                    .tasks
                    .is_current(&TaskSpec::new(&task_key_for_worker, Generation(generation_capture), None));
                if current { let _ = sender.send(completion); }
            }
        });
        if submitted.is_err() {
            self.preview_tasks.lock().unwrap_or_else(|e| e.into_inner()).retain(|task| task != &task_key);
            if let Some(sender) = self.preview_completions.lock().unwrap_or_else(|e| e.into_inner()).clone() {
                let _ = sender.send(PreviewCompletion::Failed { key: key.0.clone(), error: "preview task could not be scheduled".into() });
            }
        }
    }
}

fn prepare_one(
    store: &Arc<Mutex<PreviewAssetStore>>,
    store_generation: u64,
    key: &TrackId,
    query: &str,
    helper: &Option<PathBuf>,
    ffmpeg: &Option<PathBuf>,
    task_token: &crate::runtime::task_registry::CancellationToken,
    progress: std::sync::Arc<dyn Fn(u8) + Send + Sync>,
) -> PreviewCompletion {
    let failed = |error: String| PreviewCompletion::Failed { key: key.0.clone(), error };
    let (Some(helper), Some(ffmpeg)) = (helper, ffmpeg) else { return failed("preview toolchain is unavailable".into()); };
    let source = if crate::download::workflows::validate_http_url(query).is_ok() || query.starts_with("ytsearch") || query.starts_with("scsearch") {
        query.to_owned()
    } else {
        // Give the managed helper an explicit provider query. The previous
        // best-effort web scrape could return a transient/blocked URL, adding
        // a second network failure surface before yt-dlp even resolved the
        // recommendation.
        format!("ytsearch1:{query}")
    };
    let store = store.lock().unwrap_or_else(|e| e.into_inner());
    match prepare_preview_with_progress(&store, &key.0, store_generation, &source, helper, ffmpeg, task_token, progress) {
        Ok(path) => PreviewCompletion::Prepared { key: key.0.clone(), path },
        Err(crate::preview::assets::AssetError::Failed(message)) => failed(message),
        Err(crate::preview::assets::AssetError::Timeout) => failed("recommendation preparation timed out".into()),
        Err(crate::preview::assets::AssetError::Cancelled) => failed("recommendation preparation was cancelled".into()),
        Err(crate::preview::assets::AssetError::StaleGeneration) => failed("recommendation session changed before preparation completed".into()),
        Err(error) => failed(format!("recommendation preparation failed: {error:?}")),
    }
}

impl AppState {
    fn cancel_preview_tasks(&self) {
        let keys: Vec<String> = self.preview_tasks.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect();
        for key in keys { self.tasks.cancel_key(&key); }
        // Bump the store generation so in-flight publishes cannot land
        // after a handoff or a fresh session.
        self.preview_store.lock().unwrap_or_else(|e| e.into_inner()).begin_generation();
    }

    fn preview_snapshot(&self) -> Option<PreviewSnapshot> {
        let session = self.preview_session.lock().unwrap_or_else(|e| e.into_inner());
        session.as_ref().map(|session| PreviewSnapshot {
            generation: session.generation(),
            active: session.is_active(),
            current: session.current().map(|id| id.0.clone()),
            window: session.window().iter().map(|id| id.0.clone()).collect(),
            prepared: session.prepared_keys().into_iter().map(|id| id.0).collect(),
            unavailable: session.unavailable().iter().map(|id| id.0.clone()).collect(),
            cancelled: self.preview_cancelled.load(Ordering::Acquire),
            progress: session.progress().into_iter().map(|(key, percent)| crate::contracts::types::PreviewProgress { key: key.0, percent }).collect(),
        })
    }

    fn publish_preview_snapshot(&self, generation: u64) {
        if let Some(snapshot) = self.preview_snapshot() {
            self.publish(self.event(Generation(generation), Event::PreviewSnapshot { snapshot }));
        }
    }

    fn preview_snapshot_value(&self) -> Result<Value, ContractError> {
        self.preview_snapshot()
            .map(|snapshot| serde_json::to_value(snapshot).map_err(|error| ContractError::invalid(error.to_string())))
            .unwrap_or_else(|| {
                Err(ContractError {
                    code: ErrorCode::NotFound,
                    message: "no active preview session".into(),
                    retryable: false,
                })
            })
    }

    fn seed_matches(seed: &PreviewSeed, key: &str) -> bool {
        crate::preview::assets::safe_key(&format!("{}-{}", seed.artist.trim(), seed.title.trim()))
            .map(|candidate| candidate == key)
            .unwrap_or(false)
    }

    /// Resolution metadata for a recommendation key, taken from the preview
    /// session seeds first and the discover seeds second.
    fn seed_for(&self, key: &str) -> Option<PreviewSeed> {
        let found = self
            .preview_seeds
            .lock()
            .unwrap()
            .iter()
            .find(|seed| Self::seed_matches(seed, key))
            .cloned();
        if found.is_some() {
            return found;
        }
        self.discover_seeds
            .lock()
            .unwrap()
            .iter()
            .find(|seed| Self::seed_matches(seed, key))
            .cloned()
    }

    fn sanitize_name(value: &str) -> String {
        let cleaned: String = value
            .trim()
            .chars()
            .map(|c| {
                if c.is_ascii_control() || matches!(c, '/' | '\\' | ':') { '-' } else { c }
            })
            .collect();
        if cleaned.is_empty() || cleaned == "." || cleaned == ".." { String::new() } else { cleaned }
    }

    /// The durable filename for a saved recommendation: the seed identity in
    /// a scannable M4A container, falling back to the key.
    fn save_filename(seed: Option<&PreviewSeed>, key: &str) -> String {
        if let Some(seed) = seed {
            let artist = Self::sanitize_name(seed.artist.trim());
            let title = Self::sanitize_name(seed.title.trim());
            if !artist.is_empty() && !title.is_empty() {
                let name = format!("{artist} - {title}.m4a");
                if std::path::Path::new(&name).file_name().and_then(|n| n.to_str()) == Some(name.as_str()) {
                    return name;
                }
            }
        }
        format!("{key}.m4a")
    }

    /// Starts Discover mode: captures the physical queue, adopts every
    /// recommendation whose asset is already staged, and keeps preparing the
    /// rolling window through the preview worker pipeline.
    fn discover_enable(&self, seeds: &[PreviewSeed]) -> Result<Value, ContractError> {
        if seeds.is_empty() {
            return Err(ContractError::invalid("no recommendations to play yet"));
        }
        let ids = Self::preview_ids(seeds)?;
        let (queue, index) = {
            let playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
            if playback.current().is_none() {
                return Err(ContractError {
                    code: ErrorCode::Unavailable,
                    message: "Discover mode needs a playing track".into(),
                    retryable: false,
                });
            }
            (playback.queue().to_vec(), playback.current_index().unwrap_or(0))
        };
        let active = self
            .discover
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|mode| mode.is_active());
        if active {
            return self.discover_snapshot_value();
        }
        self.handoff_preview_to_discover();
        *self.discover_seeds.lock().unwrap_or_else(|e| e.into_inner()) = seeds.to_vec();
        self.cancel_discover_tasks();
        let generation = self.next_generation().0;
        let mut mode = DiscoverMode::new(generation, queue.clone(), index, ids.clone())
            .ok_or_else(|| ContractError::invalid("discover session could not start"))?;
        let ready: Vec<ReadyAsset> = {
            let store = self.preview_store.lock().unwrap_or_else(|e| e.into_inner());
            ids.iter()
                .filter_map(|id| {
                    let path = store.ready_path(&id.0).ok()?;
                    path.is_file().then_some(ReadyAsset { id: id.clone(), path })
                })
                .collect()
        };
        mode.enable();
        for asset in ready {
            let _ = mode.ready_asset(generation, asset);
        }
        let projected = mode.projected();
        *self.discover.lock().unwrap_or_else(|e| e.into_inner()) = Some(mode);
        self.spawn_discover_driver(generation);
        self.playback.lock().unwrap_or_else(|e| e.into_inner()).replace(projected.0, projected.1);
        self.publish_queue_snapshot();
        self.discover_refill(generation);
        self.publish_discover_snapshot(generation);
        self.discover_snapshot_value()
    }

    fn discover_disable(&self) -> Result<Value, ContractError> {
        let active = self
            .discover
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|mode| mode.is_active());
        if !active {
            // Keep the bridge response typed even when disabling an already
            // inactive session. This also makes restarting auto-discovery on
            // a new song deterministic.
            return self.discover_snapshot_value();
        }
        // Only a RECOMMENDATION must be torn down. If the user never left the
        // seed, the sounding track is still the library current and disabling
        // must leave it playing untouched (restarting it from 0:00 was a
        // needless interruption). Resolve the audible id before the restore
        // overwrites the queue.
        let sounding_recommendation = {
            let current = self.playback.lock().unwrap_or_else(|e| e.into_inner()).current().cloned();
            current.is_some_and(|id| {
                self.discover
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .is_some_and(|mode| mode.recommendations().contains(&id))
            })
        };
        let was_playing = sounding_recommendation
            && *self.playback_status.lock().unwrap_or_else(|e| e.into_inner())
                == crate::contracts::types::PlaybackStatus::Playing;
        if was_playing {
            if let Ok(mut backend) = self.audio.lock() {
                let _ = backend.stop();
            }
        }
        self.end_discover(true);
        #[cfg(target_os = "macos")]
        if was_playing {
            let _ = self.start_current_track();
        }
        self.discover_snapshot_value()
    }

    fn discover_advance(&self) -> Result<Value, ContractError> {
        let active = self
            .discover
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|mode| mode.is_active());
        if !active {
            return Err(ContractError {
                code: ErrorCode::NotFound,
                message: "no active discover session".into(),
                retryable: false,
            });
        }
        // Bind before branching: the guard from the if-let scrutinee would
        // stay held for the whole statement, deadlocking the re-lock inside
        // discover_track_started.
        let current = self.playback.lock().unwrap_or_else(|e| e.into_inner()).current().cloned();
        if let Some(current) = current {
            self.discover_track_started(&current);
        }
        self.discover_snapshot_value()
    }

    fn discover_select(&self, key: &TrackId) -> Result<Value, ContractError> {
        let (queue, index) = {
            let mut discover = self.discover.lock().unwrap_or_else(|e| e.into_inner());
            let mode = discover.as_mut().filter(|mode| mode.is_active()).ok_or_else(|| ContractError {
                code: ErrorCode::NotFound,
                message: "no active discover session".into(),
                retryable: false,
            })?;
            if !mode.recommendations().contains(key) {
                return Err(ContractError::invalid("selected recommendation is outside the discover session"));
            }
            let _ = mode.retry_asset(key);
            let queue = mode.projected_queue();
            let index = queue.iter().position(|id| id == key).ok_or_else(|| ContractError {
                code: ErrorCode::NotFound,
                message: "selected recommendation is unavailable".into(),
                retryable: true,
            })?;
            (queue, index)
        };
        self.playback.lock().unwrap_or_else(|e| e.into_inner()).replace(queue, index);
        self.publish_queue_snapshot();
        #[cfg(target_os = "macos")]
        {
            let _ = self.start_current_track()?;
        }
        self.discover_snapshot_value()
    }

    /// Rolling window + handoff on natural playback movement. Called whenever
    /// a track starts through the player.
    fn discover_track_started(&self, track_id: &TrackId) {
        let generation = match self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            Some(mode) if mode.is_active() => mode.generation(),
            _ => return,
        };
        let queue = self.playback.lock().unwrap_or_else(|e| e.into_inner()).queue().to_vec();
        // Bind the action before matching: the lock guard from the scrutinee
        // would otherwise stay held for the whole match, deadlocking the arms.
        let handoff = self
            .discover
            .lock()
            .unwrap()
            .as_mut()
            .map(|mode| mode.handoff(&queue));
        match handoff {
            Some(DiscoverAction::Noop) => {
                // A user-owned queue replaced the projection: end the session
                // without touching that queue.
                self.end_discover(false);
            }
            Some(DiscoverAction::Restore { .. }) => {
                let prepared = self
                    .discover
                    .lock()
                    .unwrap()
                    .as_mut()
                    .map(|mode| mode.advance(track_id));
                if let Some(DiscoverAction::Prepare { id }) = prepared {
                    self.ensure_discover_prepared(generation, &id);
                }
                self.discover_refill(generation);
                self.discover_sync_queue();
                self.publish_discover_snapshot(generation);
            }
            _ => {}
        }
    }

    /// Ends the session. `restore` applies the captured physical queue when
    /// it is still the session owner; a user-replaced queue is left alone.
    /// The teardown (task cancellation, staged-file cleanup, disabled
    /// snapshot) runs even when a handoff already deactivated the session.
    fn end_discover(&self, restore: bool) {
        let Some(generation) = self
            .discover
            .lock()
            .unwrap()
            .as_ref()
            .map(|mode| mode.generation())
        else {
            return;
        };
        let active = self
            .discover
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|mode| mode.is_active());
        let action = if active {
            self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(|mode| mode.disable(generation))
        } else {
            None
        };
        if restore && active {
            self.apply_discover_action(action);
        }
        self.cancel_discover_tasks();
        self.publish_discover_snapshot(generation);
    }

    /// Ends a focused/prefetch preview session when discover mode takes over.
    /// The two temporary-queue mechanisms must not both drive the asset store
    /// and output; the queue itself is left for discover to replace with its
    /// projection.
    fn handoff_preview_to_discover(&self) {
        let (active, is_prefetch) = {
            let session = self.preview_session.lock().unwrap_or_else(|e| e.into_inner());
            match session.as_ref() {
                Some(session) if session.is_active() => (true, session.is_prefetch()),
                _ => (false, true),
            }
        };
        if !active {
            return;
        }
        let was_playing = *self.playback_status.lock().unwrap_or_else(|e| e.into_inner())
            == crate::contracts::types::PlaybackStatus::Playing;
        self.tasks.cancel_key("playback-heartbeat");
        self.tasks.cancel_key("preview-heartbeat");
        self.cancel_preview_tasks();
        let generation = self.next_generation().0;
        if let Some(session) = self.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            session.invalidate(generation);
        }
        self.publish_preview_snapshot(generation);
        // A focused preview owns the output until invalidated. Stop it so the
        // (still-audible) recommendation does not keep sounding while every
        // surface names the library seed. A prefetch session never played.
        if was_playing && !is_prefetch {
            if let Ok(mut backend) = self.audio.lock() {
                let _ = backend.stop();
            }
            if let Some(clock) = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                clock.stop();
            }
            *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) =
                crate::contracts::types::PlaybackStatus::Stopped;
            *self.current_track.lock().unwrap_or_else(|e| e.into_inner()) = None;
            self.publish_queue_snapshot();
        }
    }

    /// Leaves discover mode for an explicit recommendation tap without
    /// deleting staged assets. The focused preview session can then adopt the
    /// files that were already downloaded by discover mode.
    fn handoff_discover_to_preview(&self) {
        let Some(generation) = self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|mode| mode.generation()) else { return };
        let action = self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(|mode| mode.disable(generation));
        self.apply_discover_action(action);
        self.cancel_discover_tasks();
        self.publish_discover_snapshot(generation);
    }

    fn apply_discover_action(&self, action: Option<DiscoverAction>) {
        if let Some(DiscoverAction::QueueChanged { queue, index }
        | DiscoverAction::Restore { queue, index }) = action
        {
            self.playback.lock().unwrap_or_else(|e| e.into_inner()).replace(queue, index);
            self.publish_queue_snapshot();
        }
    }

    /// The discover driver applies worker completions while the session is
    /// still current, adopts staged assets into the queue, refills the
    /// rolling window, and publishes the snapshot after every transition.
    fn spawn_discover_driver(&self, generation: u64) {
        let (tx, rx) = std::sync::mpsc::channel::<PreviewCompletion>();
        *self.discover_completions.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        let state = self.clone();
        let discover = self.discover.clone();
        let key = format!("discover-driver-{generation}");
        self.discover_tasks.lock().unwrap_or_else(|e| e.into_inner()).push(key.clone());
        let spec = self.generation(key, Generation(generation));
        let _ = self.submit(spec, move |task_token| {
            while !state.is_closing() && !task_token.is_cancelled() {
                let current = discover
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|mode| mode.is_active() && mode.generation() == generation);
                if !current {
                    break;
                }
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(PreviewCompletion::Prepared { key, path }) => {
                        state.finish_discover_task(generation, &key);
                        let action = discover.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(|mode| {
                            mode.ready_asset(
                                generation,
                                ReadyAsset {
                                    id: TrackId(key.clone()),
                                    path,
                                },
                            )
                        });
                        if let Some(DiscoverAction::Prepare { id }) = action {
                            state.ensure_discover_prepared(generation, &id);
                        }
                        state.discover_refill(generation);
                        state.discover_sync_queue();
                        state.publish_discover_snapshot(generation);
                        // If the transport is parked on this recommendation
                        // because it was still staging, start it now.
                        #[cfg(target_os = "macos")]
                        {
                            let target = TrackId(key);
                            let is_current = state.playback.lock().unwrap_or_else(|e| e.into_inner()).current() == Some(&target);
                            let not_playing = *state.playback_status.lock().unwrap_or_else(|e| e.into_inner())
                                != crate::contracts::types::PlaybackStatus::Playing;
                            if is_current && not_playing {
                                let _ = state.start_current_track();
                            }
                        }
                    }
                    Ok(PreviewCompletion::Progress { .. }) => {
                        // Discover mode exposes aggregate readiness; progress
                        // is published by the focused preview driver.
                    }
                    Ok(PreviewCompletion::Failed { key, error }) => {
                        state.finish_discover_task(generation, &key);
                        eprintln!("[luno-discover] preparation failed for {key}: {error}");
                        let id = TrackId::new(key.clone()).unwrap_or_else(|_| TrackId(key.clone()));
                        let failed_was_current = state.playback.lock().unwrap_or_else(|e| e.into_inner()).current() == Some(&id);
                        let first = {
                            let mut mode = discover.lock().unwrap_or_else(|e| e.into_inner());
                            mode.as_mut().map(|mode| {
                                let first = mode.note_failure(generation, &id);
                                let _ = mode.failed_asset(generation, &id);
                                first
                            }).unwrap_or(false)
                        };
                        // Remove the failed id from the transport projection,
                        // preserve the audible identity when possible, and
                        // continue preparing the rest of the rolling window.
                        state.discover_sync_queue();
                        state.discover_refill(generation);
                        state.publish_discover_snapshot(generation);
                        #[cfg(target_os = "macos")]
                        if failed_was_current {
                            let _ = state.start_current_track();
                        }
                        // Discover is user-initiated and otherwise silent (the
                        // transport simply parks when nothing stages); report
                        // the first failure once so the dead-end is explicable.
                        if first {
                            // Mint a fresh generation: the frontend filters
                            // `error` events by the shared errors watermark, and
                            // the session generation captured at enable can be
                            // older than an unrelated error published since.
                            state.publish(state.event(state.next_generation(), Event::Error {
                                error: ContractError {
                                    code: ErrorCode::Unavailable,
                                    message: format!("Discovery could not prepare recommendations: {error}"),
                                    retryable: true,
                                },
                            }));
                        }
                    }
                    Err(_) => {}
                }
            }
        });
    }

    /// Applies the current queue projection when it changed; the projection
    /// is always rebuilt from the captured physical queue so restoration
    /// stays exact.
    fn discover_sync_queue(&self) {
        let changed = {
            let mut discover = self.discover.lock().unwrap_or_else(|e| e.into_inner());
            let Some(mode) = discover.as_mut() else { return };
            let projected = mode.projected();
            let playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
            let live = playback.queue().to_vec();
            let live_index = playback.current_index();
            let live_current = playback.current().cloned();
            drop(playback);
            if projected.0 != live {
                // Readiness and rolling-window changes may rebuild the queue
                // while playback is inside the recommendation block. Preserve
                // the selected identity; falling back to the original physical
                // index reset every Next/Previous transition to the seed.
                let index = live_current
                    .as_ref()
                    .and_then(|current| projected.0.iter().position(|id| id == current))
                    .or_else(|| live_index.map(|index| index.min(projected.0.len().saturating_sub(1))))
                    .unwrap_or(projected.1);
                self.playback.lock().unwrap_or_else(|e| e.into_inner()).replace(projected.0, index);
                true
            } else {
                false
            }
        };
        if changed {
            self.publish_queue_snapshot();
        }
    }

    /// Registers staged assets discovered on disk and raises the first
    /// missing recommendation; worker completions re-trigger the refill so
    /// the window keeps rolling under load.
    fn discover_refill(&self, generation: u64) {
        let ids: Vec<TrackId> = self
            .discover
            .lock()
            .unwrap()
            .as_ref()
            .map(|mode| mode.preparation_window().to_vec())
            .unwrap_or_default();
        if ids.is_empty() {
            return;
        }
        let staged: Vec<(TrackId, PathBuf)> = {
            let store = self.preview_store.lock().unwrap_or_else(|e| e.into_inner());
            ids.iter()
                .filter_map(|id| {
                    let path = store.ready_path(&id.0).ok()?;
                    path.is_file().then_some((id.clone(), path))
                })
                .collect()
        };
        let mut raise: Option<TrackId> = None;
        {
            let mut discover = self.discover.lock().unwrap_or_else(|e| e.into_inner());
            let Some(mode) = discover.as_mut() else { return };
            for id in &ids {
                if mode.path_for(id).is_some() {
                    continue;
                }
                if mode.is_failed(id) {
                    continue;
                }
                if let Some((_, path)) = staged.iter().find(|(staged_id, _)| staged_id == id) {
                    let _ = mode.ready_asset(generation, ReadyAsset { id: id.clone(), path: path.clone() });
                    continue;
                }
                if raise.is_none() {
                    raise = Some(id.clone());
                }
            }
        }
        if let Some(id) = raise {
            self.ensure_discover_prepared(generation, &id);
        }
    }

    fn ensure_discover_prepared(&self, generation: u64, key: &TrackId) {
        // Discover and focused preview share the same private asset store.
        // Adopt a published file before starting a second helper when a
        // handoff raced the completion event.
        let existing = self
            .preview_store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ready_path(&key.0)
            .ok()
            .filter(|path| path.is_file());
        if let Some(path) = existing {
            let action = self
                .discover
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_mut()
                .map(|mode| mode.ready_asset(generation, ReadyAsset { id: key.clone(), path }))
                .unwrap_or(DiscoverAction::Noop);
            self.apply_discover_action(Some(action));
            self.discover_sync_queue();
            self.publish_discover_snapshot(generation);
            return;
        }
        let Some(query) = self.seed_for(&key.0) else { return };
        let query = format!("{} {}", query.artist.trim(), query.title.trim());
        let task_key = format!("discover-{generation}-{}", key.0);
        {
            let mut raised = self.discover_tasks.lock().unwrap_or_else(|e| e.into_inner());
            if raised.iter().any(|task| task == &task_key) {
                return;
            }
            raised.push(task_key.clone());
        }
        let state = self.clone();
        let store = self.preview_store.clone();
        let store_generation = self.preview_store.lock().unwrap_or_else(|e| e.into_inner()).current_generation();
        let helper = self.preview_helper.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let ffmpeg = self.preview_ffmpeg.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let key_for_task = key.clone();
        let generation_capture = generation;
        let sender = self.discover_completions.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let spec = self.generation(task_key.clone(), Generation(generation));
        let task_key_for_worker = task_key.clone();
        let submitted = self.submit(spec, move |task_token| {
            let progress_sender = sender.clone();
            let progress_key = key_for_task.0.clone();
            let progress = std::sync::Arc::new(move |percent| { if let Some(sender) = &progress_sender { let _ = sender.send(PreviewCompletion::Progress { key: progress_key.clone(), percent }); } });
            let completion = prepare_one(
                &store,
                store_generation,
                &key_for_task,
                &query,
                &helper,
                &ffmpeg,
                &task_token,
                progress,
            );
            if let Some(sender) = sender {
                let current = state
                    .tasks
                    .is_current(&TaskSpec::new(&task_key_for_worker, Generation(generation_capture), None));
                if current { let _ = sender.send(completion); }
            }
        });
        if submitted.is_err() {
            self.discover_tasks.lock().unwrap_or_else(|e| e.into_inner()).retain(|task| task != &task_key);
            if let Some(sender) = self.discover_completions.lock().unwrap_or_else(|e| e.into_inner()).clone() {
                let _ = sender.send(PreviewCompletion::Failed { key: key.0.clone(), error: "discover task could not be scheduled".into() });
            }
        }
    }

    fn finish_discover_task(&self, generation: u64, key: &str) {
        let task_key = format!("discover-{generation}-{key}");
        self.discover_tasks.lock().unwrap_or_else(|e| e.into_inner()).retain(|task| task != &task_key);
    }

    fn cancel_discover_tasks(&self) {
        let keys: Vec<String> = self.discover_tasks.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect();
        for key in keys {
            self.tasks.cancel_key(&key);
        }
        // Bump the store generation so in-flight preview publishes cannot
        // land after the session ended.
        self.preview_store.lock().unwrap_or_else(|e| e.into_inner()).begin_generation();
        *self.discover_completions.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn discover_snapshot(&self) -> Option<DiscoverSnapshot> {
        self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|mode| DiscoverSnapshot {
            generation: mode.generation(),
            enabled: mode.is_active(),
            window: mode.recommendations().iter().map(|id| id.0.clone()).collect(),
            ready: mode.ready_count(),
            queued: mode.queued_assets().len(),
            prepared: mode.prepared_keys().into_iter().map(|id| id.0).collect(),
            unavailable: mode.failed_keys().into_iter().map(|id| id.0).collect(),
        })
    }

    fn publish_discover_snapshot(&self, generation: u64) {
        if let Some(snapshot) = self.discover_snapshot() {
            self.publish(self.event(Generation(generation), Event::DiscoverSnapshot { snapshot }));
        }
    }

    fn discover_snapshot_value(&self) -> Result<Value, ContractError> {
        self.discover_snapshot()
            .map(|snapshot| {
                serde_json::to_value(snapshot).map_err(|error| ContractError::invalid(error.to_string()))
            })
            .unwrap_or_else(|| {
                Err(ContractError {
                    code: ErrorCode::NotFound,
                    message: "no discover session".into(),
                    retryable: false,
                })
            })
    }

    /// Explicit save-to-playlist: staged recommendations are remuxed into a
    /// scannable M4A and promoted once per destination; unstaged ones queue a
    /// durable download through the configured downloader. Duplicate requests
    /// for the same key are rejected while one is in flight.
    fn save_preview(&self, key: &TrackId, playlist_id: &PlaylistId) -> Result<Value, ContractError> {
        let known_directory = self
            .library
            .lock()
            .unwrap()
            .playlists
            .iter()
            .find(|playlist| &playlist.id == playlist_id)
            .map(|playlist| playlist.path.clone().map(PathBuf::from));
        let known_playlist = known_directory.is_some();
        let directory = match known_directory {
            Some(Some(directory)) => directory,
            Some(None) | None => {
                // Create-new + save is intentionally one frontend gesture.
                // The folder exists before the asynchronous library rescan
                // publishes it, so safely resolve that just-created stable id
                // instead of racing the snapshot and returning "not found".
                let name = playlist_id.0.strip_prefix("playlist:").ok_or_else(|| ContractError { code: ErrorCode::NotFound, message: "playlist not found".into(), retryable: false })?;
                if Self::sanitize_name(name) != name || name.eq_ignore_ascii_case("unsorted") {
                    return Err(ContractError::invalid("playlist is unavailable"));
                }
                let root = self.playlist_mutation_root()?;
                let candidate = root.join(name);
                let canonical = std::fs::canonicalize(&candidate).map_err(|_| ContractError {
                    code: if known_playlist { ErrorCode::InvalidRequest } else { ErrorCode::NotFound },
                    message: if known_playlist { "playlist has no directory".into() } else { "playlist not found".into() },
                    retryable: !known_playlist,
                })?;
                if !canonical.starts_with(&root) || !canonical.is_dir() { return Err(ContractError::invalid("playlist is unavailable")); }
                canonical
            }
        };
        {
            let mut in_flight = self.save_in_flight.lock().unwrap_or_else(|e| e.into_inner());
            if in_flight.contains(key) {
                return Err(ContractError::invalid(
                    "a save for this recommendation is already in progress",
                ));
            }
            in_flight.insert(key.clone());
        }
        let seed = self.seed_for(&key.0);
        let filename = Self::save_filename(seed.as_ref(), &key.0);
        let query = seed
            .map(|seed| {
                let artist = seed.artist.trim();
                let title = seed.title.trim();
                if artist.is_empty() && title.is_empty() {
                    String::new()
                } else if artist.is_empty() {
                    format!("ytsearch1:{title}")
                } else if title.is_empty() {
                    format!("ytsearch1:{artist}")
                } else {
                    format!("ytsearch1:{artist} {title}")
                }
            })
            .unwrap_or_default();
        let staged = self
            .preview_store
            .lock()
            .unwrap()
            .ready_path(&key.0)
            .ok()
            .filter(|path| path.is_file());
        let destination = PlaylistDestination {
            playlist_id: playlist_id.0.clone(),
            directory: directory.clone(),
        };
        let Some(staged) = staged else {
            // No staged asset: queue a durable download through the
            // configured downloader; completion arrives as a job event.
            let helper = self.bundle_helper.lock().unwrap_or_else(|e| e.into_inner()).clone().ok_or_else(|| {
                self.save_in_flight.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
                ContractError {
                    code: ErrorCode::Unavailable,
                    message: "no staged audio and no downloader configured".into(),
                    retryable: true,
                }
            })?;
            let job_id = JobId(format!("save-{}", key.0));
            let target = directory.join(&filename);
            self.pending_download_saves.lock().unwrap_or_else(|e| e.into_inner()).insert(job_id.0.clone(), (key.clone(), playlist_id.clone(), target.clone()));
            if let Err(error) = self.start_download(job_id.clone(), target.clone(), helper, query) {
                self.pending_download_saves.lock().unwrap_or_else(|e| e.into_inner()).remove(&job_id.0);
                self.save_in_flight.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
                return Err(error);
            }
            self.publish(self.event(Generation(self.next_generation().0), Event::SaveState {
                state: SaveState {
                    key: key.clone(),
                    playlist_id: playlist_id.clone(),
                    state: SaveStateKind::Queued,
                    already_saved: false,
                    path: Some(target.display().to_string()),
                    job_id: Some(job_id),
                    error: None,
                },
            }));
            return Ok(serde_json::json!({
                "key": key.0,
                "playlist_id": playlist_id.0,
                "state": "queued",
            }));
        };
        self.publish(self.event(Generation(self.next_generation().0), Event::SaveState {
            state: SaveState {
                key: key.clone(),
                playlist_id: playlist_id.clone(),
                state: SaveStateKind::Saving,
                already_saved: false,
                path: None,
                job_id: None,
                error: None,
            },
        }));
        let state = self.clone();
        let ffmpeg = self.preview_ffmpeg.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let id_for_task = key.clone();
        let playlist_for_task = playlist_id.clone();
        let filename_for_task = filename;
        let destination_for_task = destination;
        let generation = self.next_generation().0;
        let _ = self.submit(self.generation(format!("save-{}", key.0), Generation(generation)), move |_token| {
            let outcome =
                remux_and_promote(&staged, &filename_for_task, Some(&destination_for_task), ffmpeg.as_deref());
            // Release the in-flight guard only AFTER promotion settles, before
            // publishing so a SaveState event still implies a fresh save can
            // start. Releasing at worker START would let two racing saves pass
            // the dedupe check and promote the same recommendation twice.
            state.save_in_flight.lock().unwrap_or_else(|e| e.into_inner()).remove(&id_for_task);
            match outcome {
                Ok(receipt) => {
                    state.publish(state.event(Generation(generation), Event::SaveState {
                        state: SaveState {
                            key: id_for_task.clone(),
                            playlist_id: playlist_for_task.clone(),
                            state: SaveStateKind::Saved,
                            already_saved: receipt.already_saved,
                            path: Some(receipt.path.display().to_string()),
                            job_id: None,
                            error: None,
                        },
                    }));
                    // The promoted M4A now lives in the playlist folder, but the
                    // published snapshot's ordered_track_ids is built by the
                    // scanner. Without this refresh the save reported "Added to
                    // playlist" while the song stayed invisible until an
                    // unrelated scan (the unstaged/download path already
                    // rescans on job completion).
                    if !receipt.already_saved && !state.scan_roots().is_empty() {
                        let _ = state.schedule_mutation_scan();
                    }
                }
                Err(error) => {
                    state.publish(state.event(Generation(generation), Event::SaveState {
                        state: SaveState {
                            key: id_for_task.clone(),
                            playlist_id: playlist_for_task,
                            state: SaveStateKind::Failed,
                            already_saved: false,
                            path: None,
                            job_id: None,
                            error: Some(format!("{error:?}")),
                        },
                    }));
                }
            }
        });
        Ok(serde_json::json!({
            "key": key.0,
            "playlist_id": playlist_id.0,
            "state": "saving",
        }))
    }
    pub fn root_statuses(&self) -> Vec<RootStatus> {
        self.root_statuses.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn set_scan_roots(&self, roots: Vec<PathBuf>) {
        *self.roots.lock().unwrap_or_else(|e| e.into_inner()) = roots;
    }
    pub fn set_bundle_helper(&self, helper: Option<PathBuf>) {
        *self.bundle_helper.lock().unwrap_or_else(|e| e.into_inner()) = helper;
    }
    /// Locate the bundled yt-dlp binary beside the download helper. Used for
    /// best-effort metadata probes; absence is not fatal.
    fn discover_ytdlp(&self) -> Result<PathBuf, String> {
        let helper = self
            .bundle_helper
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| "bundled download helper is not configured".to_string())?;
        let managed_dir = helper.parent().map(PathBuf::from);
        crate::process::executables::discover(
            crate::process::executables::ExecutableKind::YtDlp,
            managed_dir.as_deref(),
        )
        .map_err(|error| error.to_string())
    }
    /// Test/embedding seam: marks a track as the current playback position
    /// without opening the audio device. Mirrors the real play path so the
    /// Discover advance/handoff wiring is exercised identically.
    pub fn set_playing(&self, track_id: TrackId, context: Vec<TrackId>) {
        self.playback.lock().unwrap_or_else(|e| e.into_inner()).play_with_context(track_id.clone(), context);
        self.playback.lock().unwrap_or_else(|e| e.into_inner()).record_started(track_id.clone(), false);
        self.discover_track_started(&track_id);
        self.publish_queue_snapshot();
    }
    pub fn scan_roots(&self) -> Vec<PathBuf> {
        self.roots.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn favorite_paths(&self) -> Vec<String> {
        self.favorites
            .lock()
            .unwrap()
            .paths()
            .map(str::to_owned)
            .collect()
    }
    pub fn set_favorite_path(&self, path: impl AsRef<std::path::Path>) {
        self.favorites.lock().unwrap_or_else(|e| e.into_inner()).set(path, true);
    }
    pub fn prune_favorites(&self) {
        let paths = self
            .track_paths
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        self.favorites.lock().unwrap_or_else(|e| e.into_inner()).project(paths);
    }
    pub fn pending_events(&self) -> Vec<EventEnvelope> {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    /// Test and integration accessor for the buffered event queue.
    pub fn events(&self) -> Vec<EventEnvelope> {
        self.pending_events()
    }
    pub fn acknowledge_event(&self, event_id: &str) {
        self.events
            .lock()
            .unwrap()
            .retain(|event| event.event_id != event_id);
    }
    pub fn delivery_failure(&self) -> Option<ContractError> {
        self.delivery_failure.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    /// Drain is single-consumer: events are claimed while holding the queue lock and
    /// failed deliveries are returned atomically instead of being duplicated.
    pub fn drain_events<F>(&self, deliver: F) -> Result<bool, String>
    where
        F: Fn(&EventEnvelope) -> Result<(), String>,
    {
        {
            let mut draining = self.event_draining.lock().unwrap_or_else(|e| e.into_inner());
            if *draining {
                return Ok(false);
            }
            *draining = true;
        }
        struct DrainGuard<'a> {
            draining: &'a Mutex<bool>,
            in_flight: &'a Mutex<usize>,
        }
        impl Drop for DrainGuard<'_> {
            fn drop(&mut self) {
                *self.in_flight.lock().unwrap_or_else(|e| e.into_inner()) = 0;
                *self.draining.lock().unwrap_or_else(|e| e.into_inner()) = false;
            }
        }
        let _guard = DrainGuard {
            draining: &self.event_draining,
            in_flight: &self.event_in_flight,
        };
        let _publication = self.publication.lock().unwrap_or_else(|e| e.into_inner());
        let claimed = { std::mem::take(&mut *self.events.lock().unwrap_or_else(|e| e.into_inner())) };
        *self.event_in_flight.lock().unwrap_or_else(|e| e.into_inner()) = claimed.len();
        let mut failed_at = None;
        let mut error = None;
        for (index, event) in claimed.iter().enumerate() {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| deliver(event)));
            match outcome {
                Ok(Ok(())) => {}
                Ok(Err(message)) => {
                    failed_at = Some(index);
                    error = Some(message);
                    break;
                }
                Err(_) => {
                    failed_at = Some(index);
                    error = Some("event delivery panicked".into());
                    break;
                }
            }
        }
        let failed = failed_at.map_or_else(Vec::new, |index| claimed[index..].to_vec());
        if !failed.is_empty() {
            self.events.lock().unwrap_or_else(|e| e.into_inner()).splice(0..0, failed);
        }
        error.map_or(Ok(true), Err)
    }
    pub fn event(&self, generation: Generation, event: Event) -> EventEnvelope {
        let _publication = self.publication.lock().unwrap_or_else(|e| e.into_inner());
        self.event_locked(generation, event)
    }

    /// Releases an allocated ID when an event is abandoned before publication.
    pub fn discard_event(&self, event_id: &str) {
        let Some(id) = event_id
            .strip_prefix("event-")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return;
        };
        let _publication = self.publication.lock().unwrap_or_else(|e| e.into_inner());
        if self.allocated_events.lock().unwrap_or_else(|e| e.into_inner()).remove(&id) {
            self.unpublished.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
            self.skipped_publications.lock().unwrap_or_else(|e| e.into_inner()).insert(id);
            self.publish_ready_locked();
        }
    }

    fn event_locked(&self, generation: Generation, event: Event) -> EventEnvelope {
        let id = self
            .next_event
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current
                    .checked_add(1)
                    .filter(|next| *next <= MAX_SAFE_EVENT_ID)
            })
            .expect("event ID space exhausted")
            + 1;
        self.allocated_events.lock().unwrap_or_else(|e| e.into_inner()).insert(id);
        EventEnvelope {
            version: CONTRACT_VERSION,
            event_id: format!("event-{id}"),
            generation,
            job_id: None,
            session_id: None,
            event,
        }
    }
    pub fn set_event_sink(
        &self,
        sink: StdArc<dyn Fn(EventEnvelope) -> Result<(), String> + Send + Sync>,
    ) {
        *self.event_sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink);
        self.replayed_events.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    /// Replay the latest state/control envelopes after a listener has started.
    pub fn replay_latest(&self) {
        let Some(sink) = self.event_sink.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
            return;
        };
        {
            let _publication = self.publication.lock().unwrap_or_else(|e| e.into_inner());
            for envelope in [
                self.latest_snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone(),
                self.latest_control.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            ]
            .into_iter()
            .flatten()
            {
                let mut events = self.events.lock().unwrap_or_else(|e| e.into_inner());
                let mut replayed = self.replayed_events.lock().unwrap_or_else(|e| e.into_inner());
                if !events
                    .iter()
                    .any(|pending| pending.event_id == envelope.event_id)
                    && !replayed.contains(&envelope.event_id)
                {
                    let event_id = envelope.event_id.clone();
                    events.push(envelope);
                    events.sort_by_key(|event| {
                        event
                            .event_id
                            .strip_prefix("event-")
                            .and_then(|id| id.parse::<u64>().ok())
                            .unwrap_or(u64::MAX)
                    });
                    replayed.insert(event_id);
                } else if events
                    .iter()
                    .any(|pending| pending.event_id == envelope.event_id)
                {
                    replayed.insert(envelope.event_id.clone());
                }
            }
        }
        let _ = self.drain_events(|event| sink(event.clone()));
    }
    pub fn stop_event_delivery(&self) {
        self.event_retry_stopped.store(true, Ordering::Release);
    }
    pub fn event_delivery_stopped(&self) -> bool {
        self.event_retry_stopped.load(Ordering::Acquire)
    }
    pub fn set_provider_metadata(&self, path: PathBuf, metadata: TrackMetadata) {
        self.provider_metadata
            .lock()
            .unwrap()
            .insert(path, metadata);
    }
    pub fn publish(&self, envelope: EventEnvelope) {
        let _publication = self.publication.lock().unwrap_or_else(|e| e.into_inner());
        let id = envelope
            .event_id
            .strip_prefix("event-")
            .and_then(|value| value.parse::<u64>().ok())
            .expect("internal event IDs are numeric");
        self.unpublished.lock().unwrap_or_else(|e| e.into_inner()).insert(id, envelope);
        self.publish_ready_locked();
    }

    fn publish_ready_locked(&self) {
        loop {
            let next = self.next_publish.load(Ordering::Acquire);
            if self.skipped_publications.lock().unwrap_or_else(|e| e.into_inner()).remove(&next) {
                self.next_publish.fetch_add(1, Ordering::Release);
                continue;
            }
            let Some(envelope) = self.unpublished.lock().unwrap_or_else(|e| e.into_inner()).remove(&next) else {
                break;
            };
            self.allocated_events.lock().unwrap_or_else(|e| e.into_inner()).remove(&next);
            self.next_publish.fetch_add(1, Ordering::Release);
            self.publish_one_locked(envelope);
        }
    }

    fn publish_one_locked(&self, envelope: EventEnvelope) {
        if matches!(&envelope.event, Event::LibrarySnapshot { .. }) {
            *self.latest_snapshot.lock().unwrap_or_else(|e| e.into_inner()) = Some(envelope.clone());
        }
        if matches!(&envelope.event, Event::Shutdown { .. }) {
            *self.latest_control.lock().unwrap_or_else(|e| e.into_inner()) = Some(envelope.clone());
        }
        let has_delivery_work = !self.events.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
            || *self.event_in_flight.lock().unwrap_or_else(|e| e.into_inner()) > 0
            || *self.event_draining.lock().unwrap_or_else(|e| e.into_inner());
        if !has_delivery_work {
            if let Some(sink) = self.event_sink.lock().unwrap_or_else(|e| e.into_inner()).clone() {
                let delivery = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    sink(envelope.clone())
                }));
                if !matches!(delivery, Ok(Ok(()))) {
                    self.queue_event_locked(envelope);
                    let failure = ContractError {
                        code: ErrorCode::Internal,
                        message: "backend event delivery failed".into(),
                        retryable: true,
                    };
                    if let Ok(failure_id) = self.next_delivery_error.fetch_update(
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                        |current| {
                            current
                                .checked_add(1)
                                .filter(|next| *next <= MAX_SAFE_EVENT_ID)
                        },
                    ) {
                        self.queue_event_locked(EventEnvelope {
                            version: CONTRACT_VERSION,
                            event_id: format!("delivery-error-{}", failure_id + 1),
                            generation: Generation(0),
                            job_id: None,
                            session_id: None,
                            event: Event::Error { error: failure },
                        });
                    } else {
                        *self.delivery_failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(failure);
                    }
                }
            } else {
                self.queue_event_locked(envelope);
            }
        } else {
            self.queue_event_locked(envelope);
        }
    }

    fn queue_event_locked(&self, envelope: EventEnvelope) {
        let mut events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        events.push(envelope);
        events.sort_by_key(|event| {
            event
                .event_id
                .strip_prefix("event-")
                .and_then(|id| id.parse::<u64>().ok())
                .unwrap_or(u64::MAX)
        });
    }
    pub fn next_generation(&self) -> Generation {
        Generation(self.next_generation.fetch_add(1, Ordering::Relaxed) + 1)
    }
    pub fn scan_async(&self) -> Result<Generation, SpawnError> {
        self.scan_async_with(|roots, token| {
            if token.is_cancelled() {
                return ScanOutcome {
                    playlists: BTreeMap::new(),
                    roots: Vec::new(),
                    errors: Vec::new(),
                };
            }
            crate::library::scanner::scan_library_with_cancel(roots, &OsFileSystem, || token.is_cancelled())
        })
    }

    /// Schedule a real task-registry scan while keeping refresh requests
    /// deterministic: one active scan and one deferred follow-up.
    pub fn scan_async_with<F>(&self, work: F) -> Result<Generation, SpawnError>
    where
        F: Fn(&[PathBuf], CancellationToken) -> ScanOutcome + Send + Sync + 'static,
    {
        let work = Arc::new(work);
        {
            let mut scan = self.scan.lock().unwrap_or_else(|e| e.into_inner());
            if scan.active {
                scan.pending = Some(work);
                return Ok(Generation(self.next_generation.load(Ordering::Acquire)));
            }
            scan.active = true;
        }
        self.start_scan(work)
    }

    fn start_scan(&self, work: ScanWork) -> Result<Generation, SpawnError> {
        let generation = self.next_generation();
        let spec = self.generation("library-scan", generation);
        let state = self.clone();
        let roots = self.roots.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let result = self.submit(spec.clone(), move |token| {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let outcome = work(&roots, token.clone());
                if !token.is_cancelled() {
                    state.publish_scan_result_if_current(&spec, generation, outcome);
                }
            }));
            match outcome {
                Ok(()) => state.finish_scan(token.is_cancelled() || state.is_closing()),
                Err(panic) => {
                    state.finish_scan(true);
                    std::panic::resume_unwind(panic);
                }
            }
        });
        if result.is_err() {
            let mut scan = self.scan.lock().unwrap_or_else(|e| e.into_inner());
            scan.active = false;
            scan.pending = None;
        }
        result.map(|_| generation)
    }

    fn finish_scan(&self, discard_pending: bool) {
        let pending = {
            let mut scan = self.scan.lock().unwrap_or_else(|e| e.into_inner());
            if discard_pending || self.is_closing() {
                scan.active = false;
                scan.pending = None;
                None
            } else {
                scan.pending.take().or_else(|| {
                    scan.active = false;
                    None
                })
            }
        };
        if let Some(work) = pending {
            if self.start_scan(work).is_err() {
                let mut scan = self.scan.lock().unwrap_or_else(|e| e.into_inner());
                scan.active = false;
                scan.pending = None;
            }
        }
    }

    pub fn publish_scan_result_if_current(
        &self,
        spec: &TaskSpec,
        generation: Generation,
        outcome: ScanOutcome,
    ) -> bool {
        if spec.generation != generation || !self.publish_allowed(spec) {
            return false;
        }
        self.publish_scan_outcome(generation, outcome);
        true
    }

    fn publish_scan_outcome(&self, generation: Generation, outcome: ScanOutcome) {
        let statuses: Vec<_> = outcome
            .roots
            .iter()
            .map(|(_, status)| match status {
                crate::library::scanner::RootStatus::Scanned => RootStatus::Scanned,
                crate::library::scanner::RootStatus::Missing => RootStatus::Missing,
                crate::library::scanner::RootStatus::Inaccessible => RootStatus::Inaccessible,
            })
            .collect();
        *self.root_statuses.lock().unwrap_or_else(|e| e.into_inner()) = statuses.clone();
        // A scan that could not read ANY root (transient unmount, network
        // volume asleep, permission hiccup) must never blank a non-empty
        // library. Keep the previous snapshot and surface the condition as an
        // error instead of publishing empty state.
        let all_roots_unreadable = !statuses.is_empty()
            && statuses.iter().all(|status| !matches!(status, RootStatus::Scanned));
        if all_roots_unreadable {
            let had_tracks = !self.library.lock().unwrap_or_else(|e| e.into_inner()).tracks.is_empty();
            if had_tracks {
                let mut error = self.event(
                    generation,
                    Event::Error {
                        error: ContractError {
                            code: ErrorCode::Internal,
                            message: "library root(s) unavailable; keeping previous library".into(),
                            retryable: true,
                        },
                    },
                );
                error.job_id = Some(JobId("library-scan".into()));
                self.publish(error);
                return;
            }
        }
        for (root_index, status) in statuses.iter().copied().enumerate() {
            let mut event = self.event(generation, Event::LibraryRootStatus { root_index, status });
            event.job_id = Some(JobId("library-scan".into()));
            self.publish(event);
            if !matches!(status, RootStatus::Scanned) {
                let mut error = self.event(
                    generation,
                    Event::Error {
                        error: ContractError {
                            code: ErrorCode::Internal,
                            message: "library root could not be scanned".into(),
                            retryable: true,
                        },
                    },
                );
                error.job_id = Some(JobId("library-scan".into()));
                self.publish(error);
            }
        }
        for issue in outcome.errors {
            let scope = issue
                .playlist
                .map_or_else(|| "root".into(), |playlist| format!("playlist {playlist}"));
            let root_path = outcome
                .roots
                .get(issue.root_index)
                .map(|(path, _)| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            let detail = issue.message.replace(&root_path, "<root>");
            let mut error = self.event(
                generation,
                Event::Error {
                    error: ContractError {
                        code: ErrorCode::Internal,
                        message: format!(
                            "library scan failed for root {root_path} ({scope}): {detail}"
                        ),
                        retryable: true,
                    },
                },
            );
            error.job_id = Some(JobId("library-scan".into()));
            self.publish(error);
        }
        let snapshot = self.replace_library(generation, outcome.playlists);
        let mut event = self.event(generation, Event::LibrarySnapshot { snapshot });
        event.job_id = Some(JobId("library-scan".into()));
        self.publish(event);
    }
    pub fn replace_library(
        &self,
        generation: Generation,
        playlists: BTreeMap<String, Vec<PathBuf>>,
    ) -> LibrarySnapshot {
        // Build the ENTIRE id→path map in a local, then swap it in with ONE
        // lock write. Clearing the shared map first left a rebuild-sized
        // window where play commands for rows still on screen resolved
        // against an EMPTY map and failed with "track is not available
        // locally" — the flaky search-click rejection.
        let playlist_urls = self.scan_roots().first()
            .and_then(|root| crate::library::mutations::PlaylistService::new(root).playlist_urls().ok())
            .unwrap_or_default();
        let mut rebuilt = BTreeMap::<TrackId, PathBuf>::new();
        let mut paths = BTreeMap::<PathBuf, TrackId>::new();
        let mut seen_ids = std::collections::HashSet::<String>::new();
        let mut tracks = Vec::new();
        let mut playlist_data = Vec::new();
        for (name, files) in playlists {
            let playlist_id = crate::contracts::types::PlaylistId(format!("playlist:{}", name));
            let mut ids = Vec::new();
            // Playlists are folders under the library root; the folder is
            // the destination directory for explicit save-to-playlist.
            let directory = files
                .first()
                .and_then(|first| first.parent().map(|p| p.to_path_buf()))
                .filter(|dir| files.iter().all(|path| path.parent() == Some(dir.as_path())));
            for path in files {
                if let Some(existing) = paths.get(&path) {
                    ids.push(existing.clone());
                    continue;
                }
                let metadata = self
                    .provider_metadata
                    .lock()
                    .unwrap()
                    .get(&path)
                    .cloned()
                    .unwrap_or_else(|| {
                        crate::domain::identity::filename_metadata(&path.to_string_lossy())
                    });
                let candidate = TrackId(crate::domain::identity::track_id(&metadata));
                // Filename-only metadata cannot distinguish "Unsorted/song.m4a"
                // from "Favorites/song.m4a" (or repeated downloads of one
                // source): both derive the SAME metadata id, which previously
                // collapsed them into one logical track — duplicated rows,
                // queues that always play the first copy. Keep the FIRST
                // occurrence canonical (persisted favorite ids survive);
                // discriminate every later collision by path hash.
                let id = if seen_ids.contains(candidate.0.as_str()) {
                    TrackId(format!(
                        "{}#{}",
                        candidate.0,
                        crate::domain::identity::path_discriminator(&path)
                    ))
                } else {
                    candidate
                };
                seen_ids.insert(id.0.clone());
                paths.insert(path.clone(), id.clone());
                rebuilt.insert(id.clone(), path.clone());
                let favorite = self.favorites.lock().unwrap_or_else(|e| e.into_inner()).contains(&path);
                tracks.push(Track {
                    id: id.clone(),
                    path: None,
                    title: metadata.title,
                    artist: (!metadata.artist.is_empty()).then_some(metadata.artist),
                    album: None,
                    duration_ms: (metadata.duration_ms != 0)
                        .then_some(metadata.duration_ms),
                    artwork_url: None,
                    favorite,
                    provider: metadata.provider.map(|provider| {
                        crate::contracts::types::ProviderId {
                            provider: provider.provider,
                            id: provider.id,
                        }
                    }),
                    added_ms: std::fs::metadata(&path)
                        .and_then(|metadata| metadata.modified())
                        .ok()
                        .and_then(|modified| {
                            modified
                                .duration_since(std::time::UNIX_EPOCH)
                                .ok()
                                .map(|elapsed| elapsed.as_millis().min(u64::MAX as u128) as u64)
                        }),
                });
                ids.push(id);
            }
            playlist_data.push(Playlist {
                id: playlist_id,
                name: name.clone(),
                path: directory.map(|dir| dir.display().to_string()),
                url: playlist_urls.get(&name).cloned(),
                ordered_track_ids: ids,
            });
        }
        let snapshot = LibrarySnapshot {
            version: CONTRACT_VERSION,
            generation,
            tracks,
            playlists: playlist_data,
        };
        *self.library.lock().unwrap_or_else(|e| e.into_inner()) = snapshot.clone();
        // Single atomic swap — see the comment at the top of this function.
        *self.track_paths.lock().unwrap_or_else(|e| e.into_inner()) = rebuilt;
        let scanned = self
            .track_paths
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        self.favorites.lock().unwrap_or_else(|e| e.into_inner()).project(scanned);
        snapshot
    }
    pub fn submit<T, F>(&self, spec: TaskSpec, work: F) -> Result<TaskHandle<T>, SpawnError>
    where
        T: Send + 'static,
        F: FnOnce(CancellationToken) -> T + Send + 'static,
    {
        self.tasks.spawn(spec, work)
    }
    pub fn generation(&self, key: impl Into<String>, generation: Generation) -> TaskSpec {
        TaskSpec::new(key, generation, None)
    }
    pub fn session_task(
        &self,
        key: impl Into<String>,
        generation: Generation,
        session: SessionId,
    ) -> TaskSpec {
        TaskSpec::new(key, generation, Some(session))
    }
    pub fn publish_allowed(&self, spec: &TaskSpec) -> bool {
        self.tasks.is_current(spec)
    }
    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport {
        self.tasks.shutdown(deadline)
    }
    pub fn is_closing(&self) -> bool {
        self.tasks.is_closing()
    }

    pub fn dispatch(&self, envelope: CommandEnvelope) -> Result<Value, ContractError> {
        if envelope.request_id.trim().is_empty() {
            return Err(ContractError::invalid("request_id must not be empty"));
        }
        let admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        if !matches!(&envelope.command, Command::Shutdown { .. }) && self.is_closing() {
            return Err(ContractError {
                code: ErrorCode::ShuttingDown,
                message: "backend is shutting down".into(),
                retryable: false,
            });
        }
        match envelope.command {
            Command::GetLibrary => {
                let snapshot = self.library();
                self.publish(self.event(
                    snapshot.generation,
                    Event::LibrarySnapshot {
                        snapshot: snapshot.clone(),
                    },
                ));
                serde_json::to_value(snapshot).map_err(|e| ContractError {
                    code: ErrorCode::Internal,
                    message: e.to_string(),
                    retryable: false,
                })
            }
            Command::ScanLibrary => {
                let generation = self.scan_async().map_err(|e| match e {
                    SpawnError::AtCapacity => ContractError {
                        code: ErrorCode::Busy,
                        message: "task capacity is full".into(),
                        retryable: true,
                    },
                    SpawnError::ShuttingDown => ContractError {
                        code: ErrorCode::ShuttingDown,
                        message: "backend is shutting down".into(),
                        retryable: false,
                    },
                })?;
                Ok(serde_json::json!({ "generation": generation.0, "job_id": "library-scan" }))
            }
            Command::LoadConfig { home } => self.load_config(PathBuf::from(home)),
            Command::RegisterRoot { root } => {
                let root = PathBuf::from(root);
                if !root.is_absolute() || root.to_string_lossy().chars().any(char::is_control) {
                    return Err(ContractError::invalid("library root must be an absolute directory"));
                }
                self.set_scan_roots(vec![root.clone()]);
                Ok(serde_json::json!({ "root": root.display().to_string() }))
            }
            Command::PromoteAudioOutput { temporary, destination } => {
                self.promote_audio_output(
                    PathBuf::from(temporary),
                    PathBuf::from(destination),
                )
            }
            Command::SyncMarkers { folder } => {
                let folder = validate_library_path(self, &PathBuf::from(&folder), false)?;
                let markers = Markers::load_checked(folder).map_err(|message| ContractError {
                    code: if message.contains("missing") {
                        ErrorCode::NotFound
                    } else {
                        ErrorCode::InvalidRequest
                    },
                    message,
                    retryable: false,
                })?;
                Ok(serde_json::json!({
                    "successful": markers.successful.len(),
                    "permanent_failures": markers.failed.len()
                }))
            }
            Command::SearchDownloader { query, try_soundcloud, limit } => {
                let query = query.trim().to_owned();
                let sources = crate::download::workflows::search_sources(&query, try_soundcloud)
                    .map_err(|error| ContractError::invalid(format!("invalid downloader search: {error:?}")))?;
                let limit = limit.clamp(1, crate::contracts::types::MAX_DOWNLOADER_SEARCH_LIMIT);
                let helper = self.bundle_helper.lock().unwrap_or_else(|e| e.into_inner()).clone();
                // Without the bundled download helper there is nothing to
                // search with: report Unavailable so the UI surfaces the exact
                // reason instead of silently showing "no matches".
                let Some(helper) = helper else {
                    return Err(ContractError {
                        code: ErrorCode::Unavailable,
                        message: "downloader search is unavailable until the bundled download helper is configured".into(),
                        retryable: false,
                    });
                };
                // Search shells out to the bundled yt-dlp binary directly (the
                // helper script is a download wrapper, not a search tool). The
                // binary lives beside the helper in the managed resources dir.
                let managed_dir = helper.parent().map(PathBuf::from);
                let ytdlp = crate::process::executables::discover(
                    crate::process::executables::ExecutableKind::YtDlp,
                    managed_dir.as_deref(),
                );
                let ytdlp = match ytdlp {
                    Ok(path) => path,
                    Err(error) => {
                        return Err(ContractError {
                            code: ErrorCode::Unavailable,
                            message: format!("downloader search is unavailable: {error}"),
                            retryable: false,
                        });
                    }
                };
                // The bundled yt-dlp is a large self-extracting executable and
                // can spend most of the old 30-second window merely starting.
                // YouTube's public results page gives us the same metadata in
                // one request; retain yt-dlp below as a compatibility fallback.
                if helper.file_name().and_then(|name| name.to_str()) == Some("luno-download-helper") {
                    match crate::download::search::search_youtube_web(&query, limit) {
                        Ok(found) if !found.is_empty() => return Ok(serde_json::to_value(found).unwrap_or(Value::Null)),
                        Ok(_) => eprintln!("[luno-downloader] fast YouTube search returned no parsable results; trying yt-dlp"),
                        Err(error) => eprintln!("[luno-downloader] fast YouTube search unavailable: {error}; trying yt-dlp"),
                    }
                }
                // Run provider searches in parallel so combined latency is the
                // max of the two rather than the sum.
                let mut results = Vec::new();
                let mut failures = Vec::new();
                let provider_count = sources.len();
                std::thread::scope(|scope| {
                    let mut handles = Vec::new();
                    for (provider, source_query) in sources {
                        let provider_name = format!("{provider:?}").to_lowercase();
                        let user_query = source_query
                            .split_once(':')
                            .map(|(_, rest)| rest.to_string())
                            .unwrap_or_else(|| source_query.clone());
                        let ytdlp = ytdlp.clone();
                        let limit = limit;
                        handles.push(scope.spawn(move || {
                            crate::download::search::search_provider(&ytdlp, &provider_name, &user_query, limit)
                        }));
                    }
                    for handle in handles {
                        match handle.join() {
                            Ok(Ok(found)) => results.extend(found),
                            Ok(Err(error)) => failures.push(error),
                            Err(_) => failures.push("downloader search worker stopped unexpectedly".into()),
                        }
                    }
                });
                if results.is_empty() && failures.len() == provider_count {
                    return Err(ContractError {
                        code: ErrorCode::Unavailable,
                        message: format!("music search failed: {}", failures.join("; ")),
                        retryable: true,
                    });
                }
                Ok(serde_json::to_value(results).unwrap_or(Value::Null))
            }
            Command::Discover { artist, title, limit } => {
                let limit = limit.clamp(1, crate::contracts::types::MAX_DISCOVER_LIMIT);
                let mut discovery = self.discovery.lock().unwrap_or_else(|e| e.into_inner());
                match discovery.get_similar(&artist, &title, limit, &self.library_recommendations()) {
                    DiscoveryResult::Success(items) => serde_json::to_value(items).map_err(|error| {
                        ContractError::invalid(format!("discovery serialization: {error}"))
                    }),
                    DiscoveryResult::Error(DiscoveryError::MissingApiKey) => Err(ContractError {
                        code: ErrorCode::Unavailable,
                        message: "discovery is unavailable until a Last.fm API key is configured".into(),
                        retryable: false,
                    }),
                    DiscoveryResult::Error(DiscoveryError::Request(message)) => Err(ContractError {
                        code: ErrorCode::Internal,
                        message,
                        retryable: true,
                    }),
                }
            }
            Command::PreviewTap { key, seeds } => self.preview_tap(&seeds, &key),
            Command::PreviewPrefetch { seeds } => self.preview_prefetch(&seeds),
            Command::PreviewNext => self.preview_step(true),
            Command::PreviewPrevious => self.preview_step(false),
            Command::PreviewCancel => self.preview_cancel(),
            Command::DiscoverEnable { seeds } => self.discover_enable(&seeds),
            Command::DiscoverDisable => self.discover_disable(),
            Command::DiscoverAdvance => self.discover_advance(),
            Command::DiscoverSelect { key } => self.discover_select(&key),
            Command::Save { key, playlist_id } => self.save_preview(&key, &playlist_id),
            Command::ValidateDirectUrl { url, destination } => {
                crate::download::workflows::validate_http_url(&url)
                    .map_err(|_| ContractError::invalid("URL must use http or https"))?;
                let destination = PathBuf::from(destination);
                if !destination.is_absolute() {
                    return Err(ContractError::invalid("destination must be an absolute path"));
                }
                validate_library_path(self, &destination, true)?;
                // Best-effort metadata resolution so direct-URL/queue downloads
                // can be named "Artist - Title" instead of the opaque job id.
                // A probe failure degrades the filename, never the download.
                let mut title = None;
                let mut artist = None;
                let mut thumbnail_url = None;
                let mut duration_secs = None;
                match self.discover_ytdlp() {
                    Ok(ytdlp) => match crate::download::search::probe_media(&ytdlp, url.trim()) {
                        Ok(probe) => {
                            title = (!probe.title.is_empty()).then_some(probe.title);
                            artist = probe.artist;
                            thumbnail_url = probe.thumbnail_url;
                            duration_secs = probe.duration;
                        }
                        Err(error) => eprintln!("[luno-downloader] URL metadata probe failed: {error}"),
                    },
                    Err(error) => eprintln!("[luno-downloader] URL metadata probe unavailable: {error}"),
                }
                Ok(serde_json::json!({
                    "url": url.trim(),
                    "destination": destination.display().to_string(),
                    "title": title,
                    "artist": artist,
                    "thumbnail_url": thumbnail_url,
                    "duration_secs": duration_secs,
                }))
            }
            Command::PreviewCsv { csv } => {
                let tracks = crate::download::csv::parse(csv.as_bytes())
                    .map_err(|error| ContractError::invalid(format!("invalid CSV: {error:?}")))?;
                Ok(serde_json::json!({ "rows": tracks.iter().enumerate().map(|(index, track)| serde_json::json!({
                    "id": format!("csv:{}", index + 1), "title": track.title, "artist": track.artist,
                    "album": track.album, "url": track.url
                })).collect::<Vec<_>>() }))
            }
            Command::QueueUrls { urls } => {
                let mut accepted = Vec::new();
                let mut rejected = Vec::new();
                for url in urls {
                    match crate::download::workflows::validate_http_url(&url) {
                        Ok(url) => accepted.push(url.to_owned()),
                        Err(_) => rejected.push(serde_json::json!({ "url": url.trim(), "reason": "invalid HTTP/HTTPS URL" })),
                    }
                }
                Ok(serde_json::json!({ "accepted": accepted, "rejected": rejected }))
            }
            Command::SelectLibraryRoot { root } => {
                let root = PathBuf::from(root);
                if !root.is_absolute() || root.to_string_lossy().chars().any(char::is_control) { return Err(ContractError::invalid("library root must be an absolute directory")); }
                if !root.is_dir() { return Err(ContractError { code: ErrorCode::NotFound, message: "library root does not exist".into(), retryable: false }); }
                let root = std::fs::canonicalize(root).map_err(|_| ContractError::invalid("library root could not be canonicalized"))?;
                self.set_scan_roots(vec![root.clone()]);
                Ok(serde_json::json!({ "root": root.display().to_string() }))
            }
            Command::CreatePlaylist { name } => self.create_playlist(&name),
            Command::SetPlaylistUrl { playlist_id, url } => self.set_playlist_url(&playlist_id, url.as_deref()),
            Command::DeletePlaylist { playlist_id } => self.delete_playlist(&playlist_id),
            Command::AddTracksToPlaylist { track_ids, playlist_id } => self.add_tracks_to_playlist(&track_ids, &playlist_id),
            Command::DeleteTracks { track_ids } => self.delete_tracks(&track_ids),
            Command::EditTrackMetadata { track_id, artist, title } => self.edit_track_metadata(track_id, &artist, &title),
            Command::StartDownload { job_id, destination, helper, source } => self.start_download(job_id, PathBuf::from(destination), PathBuf::from(helper), source),
            Command::RetryDownload { job_id } => {
                let (destination, helper, source) = self.download_requests.lock().unwrap_or_else(|e| e.into_inner()).get(&job_id.0).cloned().ok_or_else(|| ContractError { code: ErrorCode::NotFound, message: "download request not found".into(), retryable: false })?;
                self.start_download(job_id, destination, helper, source)
            }
            Command::BundleHelperPath => match self.bundle_helper.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                Some(helper) => Ok(serde_json::json!({ "helper": helper.display().to_string() })),
                None => Err(ContractError { code: ErrorCode::Unavailable, message: "bundled download helper is unavailable".into(), retryable: false }),
            },
            Command::LoadSettings { home } => self.load_config(PathBuf::from(home)),
            Command::ResolveRuntimeHome => {
                let home = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .ok_or_else(|| ContractError::invalid("runtime home is unavailable"))?;
                validate_home(&home)?;
                Ok(serde_json::json!({ "home": home.display().to_string() }))
            }
            Command::SaveSettings { home, settings } => self.save_settings(PathBuf::from(home), settings),
            Command::TransferPreview { manifest, destination } => self.transfer_preview(&manifest, PathBuf::from(destination)),
            Command::TransferExport => self.transfer_export(),
            Command::TransferApply { manifest, destination, approvals } => self.transfer_apply(&manifest, PathBuf::from(destination), approvals),
            Command::ToggleFavorite { track_id } => self.toggle_favorite(track_id),
            Command::RecordRecent { track_id } => {
                if !self.track_paths.lock().unwrap_or_else(|e| e.into_inner()).contains_key(&track_id) {
                    return Err(ContractError {
                        code: ErrorCode::NotFound,
                        message: "track is not available locally".into(),
                        retryable: false,
                    });
                }
                self.record_recent(track_id);
                Ok(Value::Null)
            }
            Command::GetRecent => serde_json::to_value(self.recent.lock().unwrap_or_else(|e| e.into_inner()).clone())
                .map_err(|_| ContractError {
                    code: ErrorCode::Internal,
                    message: "recent history unavailable".into(),
                    retryable: false,
                }),
            Command::ClearRecent => {
                self.recent.lock().unwrap_or_else(|e| e.into_inner()).clear();
                Ok(Value::Null)
            }
            Command::PlayNext { track_ids } => self.queue_insert(track_ids, true),
            Command::AddToQueue { track_ids } => self.queue_insert(track_ids, false),
            Command::RemoveFromQueue { index } => self.queue_remove(index),
            Command::ReorderQueue { from, to } => self.queue_reorder(from, to),
            Command::ClearQueue => {
                self.playback.lock().unwrap_or_else(|e| e.into_inner()).clear();
                #[cfg(target_os = "macos")]
                {
                    if let Ok(mut backend) = self.audio.lock() { backend.stop(); }
                    if let Some(clock) = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() { clock.stop(); }
                    *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) = crate::contracts::types::PlaybackStatus::Stopped;
                    *self.current_track.lock().unwrap_or_else(|e| e.into_inner()) = None;
                }
                self.publish_queue_snapshot();
                Ok(Value::Null)
            }
            Command::ToggleShuffle => {
                self.playback.lock().unwrap_or_else(|e| e.into_inner()).toggle_shuffle();
                self.publish_queue_snapshot();
                Ok(Value::Null)
            }
            Command::SetRepeat { repeat } => {
                self.playback.lock().unwrap_or_else(|e| e.into_inner()).set_repeat(match repeat {
                    ContractRepeatMode::Off => crate::playback::queue::RepeatMode::Off,
                    ContractRepeatMode::One => crate::playback::queue::RepeatMode::One,
                    ContractRepeatMode::All => crate::playback::queue::RepeatMode::All,
                });
                self.publish_queue_snapshot();
                Ok(Value::Null)
            }
            Command::CancelJob { job_id } => {
                if self.downloads.cancel_job(&job_id.0) {
                    Ok(Value::Null)
                } else if self.tasks.cancel_key(&job_id.0) {
                    Ok(Value::Null)
                } else {
                    Err(ContractError {
                        code: ErrorCode::NotFound,
                        message: "job not found".into(),
                        retryable: false,
                    })
                }
            }
            #[cfg(not(target_os = "macos"))]
            Command::Play { .. } | Command::PlayWithContext { .. } | Command::Resume | Command::Next | Command::Previous | Command::Pause | Command::Seek { .. } | Command::SetVolume { .. } => Err(ContractError {
                code: ErrorCode::Unavailable,
                message: "audio backend unavailable on this platform".into(),
                retryable: false,
            }),
            #[cfg(target_os = "macos")]
            Command::Play { track_id } => self.play_track(track_id),
            #[cfg(target_os = "macos")]
            Command::PlayWithContext { track_id, context } => self.play_with_context(track_id, context),
            #[cfg(target_os = "macos")]
            Command::Next => self.next_track(),
            #[cfg(target_os = "macos")]
            Command::Previous => self.previous_track(),
            #[cfg(target_os = "macos")]
            Command::Pause => self.pause_track(),
            #[cfg(target_os = "macos")]
            Command::Resume => self.resume_track(),
            #[cfg(target_os = "macos")]
            Command::Seek { position_ms } => self.seek_track(position_ms),
            #[cfg(target_os = "macos")]
            Command::SetVolume { volume } => self.set_volume(volume),
            Command::VideoSource { track_id } => self.video_stage_track(track_id),
            Command::VideoStage { track_id } => self.video_stage_track(track_id),
            Command::SetVideoMode { enabled } => self.set_video_mode(enabled),
            Command::VideoPlay => self.video_mutate(|video| video.status = crate::contracts::events::VideoStatus::Playing),
            Command::VideoPause => self.video_mutate(|video| video.status = crate::contracts::events::VideoStatus::Paused),
            Command::VideoSeek { position_ms } => self.video_mutate(|video| video.position_ms = position_ms as u64),
            Command::VideoResize { .. } => Ok(Value::Null),
            Command::VideoFullscreen { fullscreen } => self.video_mutate(|video| video.fullscreen = fullscreen),
            Command::Shutdown { reason } => {
                let report =
                    self.shutdown_events_locked(reason, Duration::from_millis(500), &admission);
                if report.failed {
                    Err(ContractError {
                        code: ErrorCode::Internal,
                        message: "shutdown failed because a worker panicked".into(),
                        retryable: false,
                    })
                } else if report.timed_out {
                    Err(ContractError {
                        code: ErrorCode::Internal,
                        message: "shutdown timed out; tasks or events remain undelivered".into(),
                        retryable: false,
                    })
                } else {
                    Ok(Value::Null)
                }
            }
        }
    }

    fn load_config(&self, home: PathBuf) -> Result<Value, ContractError> {
        validate_home(&home)?;
        // Remember where settings live so backend-originated changes
        // (video-mode toggle) can persist immediately.
        *self.settings_home.lock().unwrap_or_else(|e| e.into_inner()) = Some(home.clone());
        let path = home.join(".luno_config.json");
        let config = match crate::persistence::legacy::read_config(&path) {
            Ok(config) => config,
            Err(error) if error.contains("No such file") || error.contains("not found") => {
                return Err(ContractError {
                    code: ErrorCode::NotFound,
                    message: "configuration is missing".into(),
                    retryable: false,
                });
            }
            Err(_) => {
                return Err(ContractError {
                    code: ErrorCode::InvalidRequest,
                    message: "configuration is corrupt".into(),
                    retryable: false,
                });
            }
        };
        let root = config.music_root.map(|value| value.trim().to_owned());
        if root.as_deref().is_some_and(str::is_empty) {
            return Err(ContractError {
                code: ErrorCode::InvalidRequest,
                message: "configuration music root is empty".into(),
                retryable: false,
            });
        }
        let root = root.map(|root| {
            let root = PathBuf::from(root);
            if root.is_absolute() { root } else { home.join(&root) }
        });
        if let Some(root) = &root {
            if !root.is_absolute() || root.to_string_lossy().chars().any(char::is_control) || !root.is_dir() {
                return Err(ContractError::invalid("configuration music root is unsafe or unavailable"));
            }
        }
        self.set_scan_roots(root.clone().into_iter().collect());
        // Apply the persisted master volume to engine state so the first track
        // after startup plays at the saved level before the user touches the
        // slider. The backend itself is a no-op until a track is loaded; the
        // stored value is what `start_selected_track` reapplies on load.
        #[cfg(target_os = "macos")]
        if let Some(volume) = config.volume {
            let volume = volume.clamp(0.0, 1.0) as f32;
            *self.volume.lock().unwrap_or_else(|e| e.into_inner()) = volume;
            if let Ok(mut backend) = self.audio.lock() {
                backend.set_volume(volume);
            }
        }
        if let Some(paths) = config.favorite_songs {
            let mut favorites = self.favorites.lock().unwrap_or_else(|e| e.into_inner());
            for path in paths {
                favorites.set(path, true);
            }
        }
        // Song-page video mode: seed the runtime flag so auto-staging and
        // queue prefetch resume exactly where the last session left them.
        let video_mode = config.video_mode.unwrap_or(false);
        *self.video_mode_enabled.lock().unwrap_or_else(|e| e.into_inner()) = video_mode;
        // Media keys: default ON — Luno must own the hardware keys unless the
        // user explicitly opted out (Settings ▸ Playback).
        self.media_keys_enabled.store(config.global_keys.unwrap_or(true), Ordering::Release);
        #[cfg(target_os = "macos")]
        if self.media_keys_enabled.load(Ordering::Acquire) {
            crate::media_keys::MediaKeys::claim_now_playing();
        } else {
            crate::media_keys::clear_now_playing();
        }
        Ok(serde_json::json!({
            "music_root": root.map(|path| path.display().to_string()),
            "auto_discovery": config.auto_discovery,
            "volume": config.volume,
            "sleep_timer": config.sleep_timer,
            "global_keys": config.global_keys,
            "video_mode": video_mode,
            "favorite_songs": self.favorites.lock().unwrap_or_else(|e| e.into_inner()).paths().map(str::to_owned).collect::<Vec<_>>(),
        }))
    }

    fn save_settings(&self, home: PathBuf, settings: crate::contracts::types::SettingsInput) -> Result<Value, ContractError> {
        validate_home(&home)?;
        if home.as_os_str().is_empty()
            || settings.favorite_songs.iter().any(|path| path.is_empty() || path.contains('\0') || path.chars().any(char::is_control))
            || settings.volume.is_some_and(|volume| !volume.is_finite() || !(0.0..=1.0).contains(&volume))
        {
            return Err(ContractError::invalid("settings contain an unsafe path"));
        }
        if let Some(root) = settings.music_root.as_deref() {
            let root = PathBuf::from(root);
            if !root.is_absolute() || !root.is_dir() {
                return Err(ContractError::invalid("music root must be an existing absolute directory"));
            }
        }
        // MERGE semantics: overlay only the fields this caller actually knows
        // about onto the existing file. Whole-file replacement here let any
        // partial writer (video-mode toggle, settings panel) silently wipe
        // unrelated preferences — and let a second app instance clobber
        // video_mode back to false while a download was in flight.
        let mut merged: serde_json::Value = std::fs::read(&home.join(".luno_config.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        if !merged.is_object() {
            merged = serde_json::json!({});
        }
        let incoming = serde_json::json!({
            "music_root": settings.music_root,
            "auto_discovery": settings.auto_discovery,
            "volume": settings.volume,
            "sleep_timer": settings.sleep_timer,
            "global_keys": settings.global_keys,
            "video_mode": settings.video_mode,
            "favorite_songs": settings.favorite_songs,
        });
        if let (Some(target), Some(source)) = (merged.as_object_mut(), incoming.as_object()) {
            for (key, value) in source {
                if !value.is_null() {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
        let bytes = serde_json::to_vec_pretty(&merged).map_err(|_| ContractError { code: ErrorCode::Internal, message: "settings could not be encoded".into(), retryable: false })?;
        let path = home.join(".luno_config.json");
        crate::persistence::atomic_write::write_atomic(&path, &bytes).map_err(|_| ContractError { code: ErrorCode::Internal, message: "settings could not be persisted".into(), retryable: true })?;
        // Live-apply the media-keys toggle so hardware keys follow immediately.
        if let Some(enabled) = settings.global_keys {
            self.media_keys_enabled.store(enabled, Ordering::Release);
            #[cfg(target_os = "macos")]
            if enabled {
                crate::media_keys::MediaKeys::claim_now_playing();
            } else {
                crate::media_keys::clear_now_playing();
                *self.now_playing_track.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
        }
        Ok(serde_json::json!({ "saved": true }))
    }

    fn start_download(&self, job_id: JobId, destination: PathBuf, helper: PathBuf, source: String) -> Result<Value, ContractError> {
        let root = self.scan_roots().first().cloned().ok_or_else(|| ContractError::invalid("no configured library root"))?;
        let root = std::fs::canonicalize(root).map_err(|_| ContractError { code: ErrorCode::NotFound, message: "configured library root is unavailable".into(), retryable: false })?;
        let parent = destination.parent().ok_or_else(|| ContractError::invalid("destination must name a file"))?;
        let parent = if parent.exists() {
            std::fs::canonicalize(parent).map_err(|_| ContractError::invalid("destination directory is unavailable"))?
        } else {
            let ancestor = parent
                .ancestors()
                .find(|candidate| candidate.exists())
                .unwrap_or(std::path::Path::new("/"));
            let ancestor = std::fs::canonicalize(ancestor).map_err(|_| ContractError::invalid("destination directory is unavailable"))?;
            if !ancestor.starts_with(&root) {
                return Err(ContractError::invalid("download helper or destination is unsafe"));
            }
            std::fs::create_dir_all(parent).map_err(|_| ContractError::invalid("destination directory is unavailable"))?;
            std::fs::canonicalize(parent).map_err(|_| ContractError::invalid("destination directory is unavailable"))?
        };
        if !parent.starts_with(&root) || !helper.is_absolute() || !helper.is_file() {
            return Err(ContractError::invalid("download helper or destination is unsafe"));
        }
        let source = source.replace('\r', " ").replace('\n', " ").trim().to_owned();
        self.download_requests.lock().unwrap_or_else(|e| e.into_inner()).insert(job_id.0.clone(), (destination.clone(), helper.clone(), source.clone()));
        let generation = self.next_generation();
        let job = job_id.clone();
        let coordinator = self.downloads.clone();
        let state = self.clone();
        let helper_for_worker = helper.clone();
        let source_for_worker = source.clone();
        let receiver = coordinator.events_for(&job.0);
        let worker_home = root.join(".luno-gate4-worker-home");
        std::fs::create_dir_all(&worker_home).map_err(|_| ContractError::invalid("download worker home is unavailable"))?;
        let start_result = coordinator.start(crate::download::coordinator::DownloadRequest { job_id: job.0.clone(), destination }, Box::new(move |temporary, token, progress| {
            if token.is_cancelled() { return Err("cancelled".into()); }
            crate::download::workflows::run_download_helper(&helper_for_worker, temporary, &source_for_worker, &worker_home, token, progress)
        }));
        if let Err(error) = start_result {
            coordinator.remove_events_for(&job.0);
            return Err(ContractError { code: match error { crate::download::coordinator::StartError::Busy => ErrorCode::Busy, crate::download::coordinator::StartError::ShuttingDown => ErrorCode::ShuttingDown, crate::download::coordinator::StartError::InvalidRequest => ErrorCode::InvalidRequest }, message: "download could not start".into(), retryable: matches!(error, crate::download::coordinator::StartError::Busy) });
        }
        let spec = self.generation(format!("download-{}", job.0), generation);
        let worker_job = job.clone();
        self.submit(spec, move |task_token| {
            while !state.is_closing() {
                if task_token.is_cancelled() { break; }
                let Ok(event) = receiver.recv_timeout(Duration::from_millis(50)) else {
                    continue;
                };
                use crate::download::coordinator::DownloadEvent;
                let (state_value, progress, error, phase) = match &event {
                    DownloadEvent::Started { .. } => (crate::contracts::types::JobState::Running, None, None, None),
                    DownloadEvent::Progress { fraction, .. } => (crate::contracts::types::JobState::Running, Some(*fraction), None, None),
                    DownloadEvent::Finalizing { .. } => (crate::contracts::types::JobState::Running, Some(0.97), None, Some(DownloadPhase::Finalizing)),
                    DownloadEvent::Cancelling { .. } => (crate::contracts::types::JobState::Cancelling, None, None, None),
                    DownloadEvent::Succeeded { .. } => (crate::contracts::types::JobState::Succeeded, Some(1.0), None, None),
                    DownloadEvent::Failed { error, .. } => {
                        (crate::contracts::types::JobState::Failed, None, Some(ContractError { code: ErrorCode::Unavailable, message: sanitize_download_error(error), retryable: true }), None)
                    },
                    DownloadEvent::Cancelled { .. } => (crate::contracts::types::JobState::Cancelled, None, None, None),
                };
                let mut envelope = state.event(generation, Event::JobUpdated { job: crate::contracts::types::DownloadJob { id: worker_job.clone(), kind: crate::contracts::types::DownloadKind::Single, state: state_value, track_id: None, progress, phase, generation } });
                envelope.job_id = Some(worker_job.clone());
                state.publish(envelope);
                if let Some(error) = error { state.publish(state.event(generation, Event::Error { error })); }
                if matches!(event, DownloadEvent::Succeeded { .. } | DownloadEvent::Failed { .. } | DownloadEvent::Cancelled { .. }) {
                    if let Some((key, playlist_id, path)) = state.pending_download_saves.lock().unwrap_or_else(|e| e.into_inner()).remove(&worker_job.0) {
                        state.save_in_flight.lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
                        let (save_state, save_error) = match &event {
                            DownloadEvent::Succeeded { .. } => (SaveStateKind::Saved, None),
                            DownloadEvent::Failed { error, .. } => (SaveStateKind::Failed, Some(sanitize_download_error(error))),
                            DownloadEvent::Cancelled { .. } => (SaveStateKind::Failed, Some("download cancelled".into())),
                            _ => unreachable!(),
                        };
                        state.publish(state.event(generation, Event::SaveState { state: SaveState {
                            key,
                            playlist_id,
                            state: save_state,
                            already_saved: false,
                            path: matches!(save_state, SaveStateKind::Saved).then(|| path.display().to_string()),
                            job_id: Some(worker_job.clone()),
                            error: save_error,
                        }}));
                    }
                }
                if matches!(event, DownloadEvent::Succeeded { .. }) {
                    // A completed download wrote a new audio file into the
                    // library; refresh the snapshot so the song appears in the
                    // app immediately (the legacy Python app called
                    // `_refresh_library()` on completion). Scans are
                    // generation-guarded and coalesce, so repeated downloads
                    // queue at most one follow-up scan.
                    let _ = state.scan_async();
                }
                if matches!(event, DownloadEvent::Succeeded { .. } | DownloadEvent::Failed { .. } | DownloadEvent::Cancelled { .. }) { break; }
            }
        }).map_err(|_| ContractError { code: ErrorCode::Internal, message: "download worker could not start".into(), retryable: true })?;
        Ok(serde_json::json!({ "job_id": job.0, "generation": generation.0 }))
    }

    fn transfer_export(&self) -> Result<Value, ContractError> {
        let library = self.library();
        let mut assigned = BTreeSet::new();
        let playlists = library.playlists.iter().map(|playlist| {
            for id in &playlist.ordered_track_ids { assigned.insert(id.clone()); }
            crate::transfer::codec::Playlist { name: playlist.name.clone(), description: String::new(), track_refs: playlist.ordered_track_ids.iter().map(|id| id.0.clone()).collect() }
        }).collect::<Vec<_>>();
        let tracks = library.tracks.iter().map(|track| crate::transfer::codec::Track {
            r#ref: track.id.0.clone(), title: track.title.clone(), artist: track.artist.clone().unwrap_or_default(), album: track.album.clone().unwrap_or_default(), duration_ms: track.duration_ms.unwrap_or_default(), source: track.provider.as_ref().map(|source| crate::transfer::codec::Source { provider: source.provider.clone(), id: source.id.clone() }), ambiguity_confirmation_required: track.provider.is_none(),
        }).collect::<Vec<_>>();
        let unassigned_track_refs = library.tracks.iter().filter(|track| !assigned.contains(&track.id)).map(|track| track.id.0.clone()).collect();
        let manifest = crate::transfer::codec::Manifest { format: crate::transfer::codec::MANIFEST_FORMAT.into(), manifest_version: crate::transfer::codec::MANIFEST_VERSION, scope: "library".into(), created_at: "1970-01-01T00:00:00Z".into(), tracks, playlists, unassigned_track_refs };
        let manifest = crate::transfer::codec::encode(&manifest).map_err(|error| ContractError::invalid(error.to_string()))?;
        Ok(serde_json::json!({ "manifest": manifest }))
    }

    fn transfer_index(&self) -> crate::transfer::provider_index::ProviderIndex {
        let metadata = self.provider_metadata.lock().unwrap_or_else(|e| e.into_inner());
        crate::transfer::provider_index::ProviderIndex::new(self.track_paths.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(id, path)| crate::transfer::provider_index::IndexedTrack { path: path.clone(), metadata: metadata.get(path).cloned().unwrap_or_else(|| crate::domain::identity::filename_metadata(&id.0)) }))
    }

    fn transfer_preview(&self, manifest: &str, destination: PathBuf) -> Result<Value, ContractError> {
        self.validate_transfer_destination(&destination)?;
        let manifest = crate::transfer::codec::decode(manifest).map_err(|error| ContractError::invalid(error.to_string()))?;
        let plan = crate::transfer::planner::plan_import(&manifest, &self.transfer_index(), destination);
        let matched = plan.tracks.iter().filter(|track| matches!(track.match_plan, crate::transfer::planner::MatchPlan::Matched(_))).count();
        let ambiguous = plan.tracks.iter().filter(|track| matches!(track.match_plan, crate::transfer::planner::MatchPlan::Ambiguous(_))).count();
        let missing = plan.tracks.iter().filter(|track| matches!(track.match_plan, crate::transfer::planner::MatchPlan::Unresolved)).count();
        let provider_matches = manifest.tracks.iter().filter(|track| track.source.is_some()).count().min(matched);
        Ok(serde_json::json!({ "matched": matched, "ambiguous": ambiguous, "missing": missing, "provider_matches": provider_matches, "unresolved_provider_handoff": missing, "no_deletions": plan.no_deletions }))
    }

    fn transfer_apply(&self, manifest: &str, destination: PathBuf, approvals: std::collections::BTreeMap<String, String>) -> Result<Value, ContractError> {
        self.validate_transfer_destination(&destination)?;
        let manifest = crate::transfer::codec::decode(manifest).map_err(|error| ContractError::invalid(error.to_string()))?;
        let plan = crate::transfer::planner::plan_import(&manifest, &self.transfer_index(), &destination);
        let approvals = approvals.into_iter().map(|(key, value)| (key, PathBuf::from(value))).collect();
        let result = crate::transfer::planner::apply_import_plan(&plan, &approvals);
        Ok(serde_json::json!({ "copied": result.copied.len(), "failures": result.failures.len(), "no_deletions": plan.no_deletions }))
    }

    fn playlist_mutation_root(&self) -> Result<PathBuf, ContractError> {
        let root = self.scan_roots().first().cloned().ok_or_else(|| ContractError::invalid("no configured library root"))?;
        std::fs::canonicalize(root).map_err(|_| ContractError { code: ErrorCode::NotFound, message: "configured library root is unavailable".into(), retryable: false })
    }

    fn playlist_name(&self, playlist_id: &PlaylistId) -> Result<String, ContractError> {
        self.library.lock().unwrap_or_else(|e| e.into_inner()).playlists.iter()
            .find(|playlist| &playlist.id == playlist_id).map(|playlist| playlist.name.clone())
            .ok_or_else(|| ContractError { code: ErrorCode::NotFound, message: "playlist not found".into(), retryable: false })
    }

    fn schedule_mutation_scan(&self) -> Result<u64, ContractError> {
        self.scan_async().map(|generation| generation.0).map_err(|error| match error {
            SpawnError::AtCapacity => ContractError { code: ErrorCode::Busy, message: "playlist changed, but the library refresh queue is full".into(), retryable: true },
            SpawnError::ShuttingDown => ContractError { code: ErrorCode::ShuttingDown, message: "backend is shutting down".into(), retryable: false },
        })
    }

    fn create_playlist(&self, name: &str) -> Result<Value, ContractError> {
        let service = crate::library::mutations::PlaylistService::new(self.playlist_mutation_root()?); let mut index = crate::library::index::LibraryIndex::default();
        let result = service.create(name, &mut index).map_err(playlist_mutation_error)?; let generation = self.schedule_mutation_scan()?;
        Ok(serde_json::json!({ "name": name.trim(), "path": result.value.display().to_string(), "generation": generation }))
    }

    fn set_playlist_url(&self, playlist_id: &PlaylistId, url: Option<&str>) -> Result<Value, ContractError> {
        let name = self.playlist_name(playlist_id)?;
        let normalized = match url.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) => Some(crate::download::sync::playlist_url(value).ok_or_else(|| ContractError::invalid("enter a valid YouTube playlist URL"))?),
            None => None,
        };
        let service = crate::library::mutations::PlaylistService::new(self.playlist_mutation_root()?);
        let mut index = crate::library::index::LibraryIndex::default();
        service.set_url(&name, normalized.as_deref(), &mut index).map_err(playlist_mutation_error)?;
        let generation = self.schedule_mutation_scan()?;
        Ok(serde_json::json!({ "playlist_id": playlist_id, "url": normalized, "generation": generation }))
    }

    fn delete_playlist(&self, playlist_id: &PlaylistId) -> Result<Value, ContractError> {
        let name = self.playlist_name(playlist_id)?; let service = crate::library::mutations::PlaylistService::new(self.playlist_mutation_root()?); let mut index = crate::library::index::LibraryIndex::default();
        service.delete(&name, &mut index).map_err(playlist_mutation_error)?; let generation = self.schedule_mutation_scan()?;
        Ok(serde_json::json!({ "deleted": name, "generation": generation }))
    }

    fn add_tracks_to_playlist(&self, track_ids: &[TrackId], playlist_id: &PlaylistId) -> Result<Value, ContractError> {
        if track_ids.is_empty() { return Err(ContractError::invalid("choose at least one track")); }
        // Create-and-add can arrive before the empty folder's scan publishes;
        // only the non-destructive add path may resolve that stable id early.
        let name = self.playlist_name(playlist_id).or_else(|_| playlist_id.0.strip_prefix("playlist:").map(str::to_owned).ok_or_else(|| ContractError { code: ErrorCode::NotFound, message: "playlist not found".into(), retryable: false }))?;
        let paths = { let map = self.track_paths.lock().unwrap_or_else(|e| e.into_inner()); track_ids.iter().map(|id| map.get(id).cloned().ok_or_else(|| ContractError { code: ErrorCode::NotFound, message: "one or more tracks are no longer in the library".into(), retryable: true })).collect::<Result<Vec<_>, _>>()? };
        let service = crate::library::mutations::PlaylistService::new(self.playlist_mutation_root()?); let mut index = crate::library::index::LibraryIndex::default();
        let result = service.add_files(&paths, &name, &mut index).map_err(playlist_mutation_error)?; let generation = self.schedule_mutation_scan()?;
        Ok(serde_json::json!({ "playlist": name, "copied": result.value.copied, "failed": result.value.failed, "generation": generation }))
    }

    fn delete_tracks(&self, track_ids: &[TrackId]) -> Result<Value, ContractError> {
        if track_ids.is_empty() { return Err(ContractError::invalid("choose at least one track")); }
        let paths = { let map = self.track_paths.lock().unwrap_or_else(|e| e.into_inner()); track_ids.iter().map(|id| map.get(id).cloned().ok_or_else(|| ContractError { code: ErrorCode::NotFound, message: "one or more tracks are no longer in the library".into(), retryable: true })).collect::<Result<Vec<_>, _>>()? };
        for path in &paths {
            validate_library_path(self, path, true)?;
            std::fs::remove_file(path).map_err(|error| ContractError { code: ErrorCode::Internal, message: format!("could not delete '{}': {error}", path.file_name().and_then(|name| name.to_str()).unwrap_or("track")), retryable: true })?;
        }
        // Optimistic snapshot so the rows disappear immediately in every view.
        // The follow-up scan reconciles playlist membership from disk; without
        // this the UI waited on a slow scan and reads as "delete did nothing".
        {
            let removed: std::collections::HashSet<&TrackId> = track_ids.iter().collect();
            let mut library = self.library.lock().unwrap_or_else(|e| e.into_inner());
            library.tracks.retain(|track| !removed.contains(&track.id));
            for playlist in library.playlists.iter_mut() {
                playlist.ordered_track_ids.retain(|id| !removed.contains(id));
            }
            let generation = self.next_generation();
            library.generation = generation;
            let snapshot = library.clone();
            drop(library);
            self.publish(self.event(snapshot.generation, Event::LibrarySnapshot { snapshot }));
        }
        let generation = self.schedule_mutation_scan()?;
        Ok(serde_json::json!({ "deleted": paths.len(), "generation": generation }))
    }

    /// Rename a library file so its filename metadata matches the user-edited
    /// artist/title. The scanner derives identity from `Artist - Title.ext`, so
    /// this is the only durable way to correct a mis-named download.
    fn edit_track_metadata(
        &self,
        track_id: TrackId,
        artist: &str,
        title: &str,
    ) -> Result<Value, ContractError> {
        let title = sanitize_track_stem(title);
        if title.is_empty() {
            return Err(ContractError::invalid("title cannot be empty"));
        }
        let artist = sanitize_track_stem(artist);
        let path = self
            .track_paths
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&track_id)
            .cloned()
            .ok_or_else(|| ContractError {
                code: ErrorCode::NotFound,
                message: "track is not available locally".into(),
                retryable: false,
            })?;
        let path = validate_library_path(self, &path, true)?;
        let parent = path.parent().ok_or_else(|| ContractError::invalid("track file has no parent directory"))?;
        let stem = if artist.is_empty() { title.clone() } else { format!("{artist} - {title}") };
        let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("");
        let new_name = if extension.is_empty() { stem.clone() } else { format!("{stem}.{extension}") };
        let new_path = parent.join(&new_name);
        if new_path.as_os_str().is_empty() || new_path.to_string_lossy().chars().any(char::is_control) {
            return Err(ContractError::invalid("new track name is unsafe"));
        }
        // A case-only rename resolves to the same file on macOS' default
        // case-insensitive filesystem, so it must not be rejected as a collision.
        let case_only = new_path.to_string_lossy().to_lowercase() == path.to_string_lossy().to_lowercase();
        if new_path != path {
            if new_path.exists() && !case_only {
                return Err(ContractError::invalid("a file with that name already exists"));
            }
            std::fs::rename(&path, &new_path).map_err(|error| ContractError {
                code: ErrorCode::Internal,
                message: format!("could not rename track: {error}"),
                retryable: true,
            })?;
            let mut favorites = self.favorites.lock().unwrap_or_else(|e| e.into_inner());
            if favorites.contains(&path) {
                favorites.set(&path, false);
                favorites.set(&new_path, true);
            }
            self.provider_metadata.lock().unwrap_or_else(|e| e.into_inner()).remove(&path);
        }
        // Optimistic snapshot so the row reflects the new name immediately; the
        // follow-up scan swaps the filename-derived id and reconciles membership.
        {
            let mut library = self.library.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(track) = library.tracks.iter_mut().find(|track| track.id == track_id) {
                track.title = title.clone();
                track.artist = (!artist.is_empty()).then_some(artist.clone());
            }
            let generation = self.next_generation();
            library.generation = generation;
            let snapshot = library.clone();
            drop(library);
            self.publish(self.event(snapshot.generation, Event::LibrarySnapshot { snapshot }));
        }
        let generation = self.schedule_mutation_scan()?;
        Ok(serde_json::json!({
            "path": new_path.display().to_string(),
            "title": title,
            "artist": artist,
            "generation": generation,
        }))
    }

    fn validate_transfer_destination(&self, destination: &PathBuf) -> Result<(), ContractError> {
        let roots = self.scan_roots();
        let root = roots.first().ok_or_else(|| ContractError::invalid("no configured library root"))?;
        let root = std::fs::canonicalize(root).map_err(|_| ContractError { code: ErrorCode::NotFound, message: "configured library root is unavailable".into(), retryable: false })?;
        let destination = std::fs::canonicalize(destination).map_err(|_| ContractError::invalid("transfer destination is unavailable"))?;
        if destination.starts_with(root) { Ok(()) } else { Err(ContractError::invalid("transfer destination is outside the configured library")) }
    }

    fn promote_audio_output(
        &self,
        temporary: PathBuf,
        destination: PathBuf,
    ) -> Result<Value, ContractError> {
        if temporary.as_os_str().is_empty() || destination.as_os_str().is_empty() {
            return Err(ContractError::invalid("promotion paths must not be empty"));
        }
        let configured_root = self
            .scan_roots()
            .first()
            .cloned()
            .ok_or_else(|| ContractError::invalid("no configured library root"))?;
        let root = std::fs::canonicalize(&configured_root).map_err(|_| ContractError {
            code: ErrorCode::NotFound,
            message: "configured library root is unavailable".into(),
            retryable: false,
        })?;
        let parent = destination.parent().ok_or_else(|| {
            ContractError::invalid("promotion destination must name a file")
        })?;
        let canonical_parent = std::fs::canonicalize(parent).map_err(|_| {
            ContractError::invalid("promotion destination directory is unavailable")
        })?;
        if !canonical_parent.starts_with(&root)
            || destination.file_name().is_none_or(|name| name.is_empty())
        {
            return Err(ContractError::invalid(
                "promotion destination is outside the configured library",
            ));
        }
        promote_output(
            &temporary,
            &destination,
            Validation::audio(),
            CollisionPolicy::Reject,
            None,
        )
        .map(|receipt| serde_json::json!({ "path": receipt.path, "bytes": receipt.bytes }))
        .map_err(Self::promotion_error)
    }

fn promotion_error(error: PromotionError) -> ContractError {
        match error {
            PromotionError::Cancelled => ContractError {
                code: ErrorCode::Cancelled,
                message: "audio promotion was cancelled".into(),
                retryable: false,
            },
            PromotionError::Collision(_) => ContractError {
                code: ErrorCode::Busy,
                message: "promotion destination already exists".into(),
                retryable: false,
            },
            PromotionError::InvalidOutput(message) => ContractError::invalid(message),
            PromotionError::Io(error) => ContractError {
                code: if error.kind() == std::io::ErrorKind::NotFound {
                    ErrorCode::NotFound
                } else {
                    ErrorCode::Internal
                },
                message: "audio promotion could not complete".into(),
                retryable: error.kind() != std::io::ErrorKind::NotFound,
            },
        }
    }

    fn toggle_favorite(&self, track_id: TrackId) -> Result<Value, ContractError> {
        let path = self
            .track_paths
            .lock()
            .unwrap()
            .get(&track_id)
            .cloned()
            .ok_or_else(|| ContractError {
                code: ErrorCode::NotFound,
                message: "track is not available locally".into(),
                retryable: false,
            })?;
        let favorite = {
            let mut favorites = self.favorites.lock().unwrap_or_else(|e| e.into_inner());
            let next = !favorites.contains(&path);
            favorites.set(&path, next);
            next
        };
        // Publish the mutated snapshot so every view (playlist rows, search
        // results, queue, footer) re-renders the star immediately. The old
        // code mutated in-memory state but emitted no event, so the UI never
        // updated until the next rescan.
        let snapshot = {
            let mut library = self.library.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(track) = library.tracks.iter_mut().find(|track| track.id == track_id) {
                track.favorite = favorite;
            }
            let generation = self.next_generation();
            library.generation = generation;
            library.clone()
        };
        self.publish(self.event(snapshot.generation, Event::LibrarySnapshot { snapshot }));
        // Persist so the choice survives restarts. Merge semantics keep other
        // prefs intact; failures are non-fatal (in-memory state already won).
        if let Some(home) = self.settings_home.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            let settings = crate::contracts::types::SettingsInput {
                music_root: None,
                auto_discovery: None,
                volume: None,
                sleep_timer: None,
                global_keys: None,
                favorite_songs: self.favorites.lock().unwrap_or_else(|e| e.into_inner()).paths().map(str::to_owned).collect(),
                video_mode: None,
            };
            let _ = self.save_settings(home, settings);
        }
        Ok(serde_json::json!({ "favorite": favorite }))
    }

    pub fn record_recent(&self, track_id: TrackId) {
        let mut recent = self.recent.lock().unwrap_or_else(|e| e.into_inner());
        recent.retain(|id| id != &track_id);
        recent.insert(0, track_id);
        recent.truncate(100);
    }

    #[cfg(target_os = "macos")]
    fn playback_error(error: BackendError) -> ContractError {
        let code = match error {
            BackendError::InvalidSeek(_) => ErrorCode::InvalidRequest,
            BackendError::NotLoaded => ErrorCode::NotFound,
            BackendError::UnsupportedFormat(_)
            | BackendError::Io(_)
            | BackendError::Decode(_)
            | BackendError::Device(_)
            | BackendError::Cancelled => ErrorCode::Unavailable,
        };
        ContractError {
            code,
            message: error.to_string(),
            retryable: matches!(code, ErrorCode::Unavailable),
        }
    }

    #[cfg(target_os = "macos")]
    fn recover_from_backend_error(&self, error: BackendError) {
        let contract = Self::playback_error(error);
        self.publish(self.event(self.library().generation, Event::Error { error: contract }));
        let action = self.recovery.lock().unwrap_or_else(|e| e.into_inner()).record_failure();
        if matches!(action, crate::playback::recovery::RecoveryAction::Stop) {
            *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) = crate::contracts::types::PlaybackStatus::Failed;
            self.publish_queue_snapshot();
        } else {
            let _ = self.next_track();
        }
    }

    fn playback_snapshot(
        &self,
        status: crate::contracts::types::PlaybackStatus,
        position: Duration,
        duration: Option<Duration>,
    ) -> crate::contracts::types::PlaybackSnapshot {
        let queue = self.playback.lock().unwrap_or_else(|e| e.into_inner());
        crate::contracts::types::PlaybackSnapshot {
            queue: queue.queue().to_vec(),
            current_index: queue.current_index().map(|index| index as u32),
            shuffle: queue.shuffle(),
            repeat: match queue.repeat() {
                crate::playback::queue::RepeatMode::Off => ContractRepeatMode::Off,
                crate::playback::queue::RepeatMode::One => ContractRepeatMode::One,
                crate::playback::queue::RepeatMode::All => ContractRepeatMode::All,
            },
            position_ms: position.as_millis().min(u64::MAX as u128) as u64,
            duration_ms: duration.map(|value| value.as_millis().min(u64::MAX as u128) as u64),
            status,
            generation: self.next_generation(),
        }
    }

    fn publish_playback(
        &self,
        snapshot: crate::contracts::types::PlaybackSnapshot,
        track: Option<TrackId>,
    ) {
        let snapshot_generation = snapshot.generation;
        // Update the OS Now Playing session in lockstep with the backend
        // snapshot so media keys keep routing to Luno during playback.
        #[cfg(target_os = "macos")]
        self.publish_now_playing(&snapshot, track.as_ref());
        self.publish(self.event(snapshot_generation, Event::PlaybackSnapshot { snapshot }));
        if let Some(track_id) = track {
            // TrackStarted rides the same generation as its snapshot so the
            // frontend's stale-event filter never drops it (playback domain
            // generation outruns the library generation during heartbeats).
            self.publish(self.event(snapshot_generation, Event::TrackStarted { track_id }));
        }
    }

    /// Publish truthful Now Playing metadata + playback state. Modern macOS
    /// consults `MPNowPlayingInfoCenter.playbackState` when deciding which
    /// process owns the hardware media keys; a launch-time claim without it
    /// silently loses routing back to other players.
    #[cfg(target_os = "macos")]
    fn publish_now_playing(
        &self,
        snapshot: &crate::contracts::types::PlaybackSnapshot,
        track: Option<&TrackId>,
    ) {
        if !self.media_keys_enabled.load(Ordering::Acquire) {
            crate::media_keys::clear_now_playing();
            return;
        }
        if let Some(track_id) = track {
            let metadata = self
                .library
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .tracks
                .iter()
                .find(|track| &track.id == track_id)
                .map(|track| (track.title.clone(), track.artist.clone().unwrap_or_default(), track.album.clone().unwrap_or_default()));
            if let Some(metadata) = metadata {
                *self.now_playing_track.lock().unwrap_or_else(|e| e.into_inner()) = Some(metadata);
            }
        }
        let (title, artist, album) = self
            .now_playing_track
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_default();
        crate::media_keys::publish_now_playing(&crate::media_keys::NowPlaying {
            title,
            artist,
            album,
            duration_secs: snapshot.duration_ms.map(|ms| ms as f64 / 1000.0).unwrap_or(0.0),
            elapsed_secs: snapshot.position_ms as f64 / 1000.0,
            playing: snapshot.status == crate::contracts::types::PlaybackStatus::Playing,
        });
    }

    fn publish_queue_snapshot(&self) {
        let status = {
            #[cfg(target_os = "macos")]
            { *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) }
            #[cfg(not(target_os = "macos"))]
            { crate::contracts::types::PlaybackStatus::Stopped }
        };
        let snapshot = self.playback_snapshot(status, Duration::ZERO, None);
        self.publish_playback(snapshot, None);
    }

    fn queue_insert(&self, track_ids: Vec<TrackId>, next: bool) -> Result<Value, ContractError> {
        if next { self.playback.lock().unwrap_or_else(|e| e.into_inner()).play_next(track_ids); }
        else { self.playback.lock().unwrap_or_else(|e| e.into_inner()).add_to_queue(track_ids); }
        self.publish_queue_snapshot();
        Ok(Value::Null)
    }

    fn queue_remove(&self, index: usize) -> Result<Value, ContractError> {
        let was_current = {
            let mut queue = self.playback.lock().unwrap_or_else(|e| e.into_inner());
            let was_current = queue.current_index() == Some(index);
            if !queue.remove(index) {
                return Err(ContractError::invalid("queue index is out of range"));
            }
            was_current
        };
        // Removing the PLAYING row re-points current_index at the track that
        // shifted into its slot; without restarting audio the old track keeps
        // sounding while the published snapshot names a different one. Align
        // the output with the (new) current track, or stop when the queue emptied.
        #[cfg(target_os = "macos")]
        if was_current {
            let has_current = self.playback.lock().unwrap_or_else(|e| e.into_inner()).current().is_some();
            if has_current {
                // Even if the replacement track cannot start, the removal must
                // still be published so the panel drops the removed row.
                return self.start_current_track().inspect_err(|_| self.publish_queue_snapshot());
            }
            if let Ok(mut backend) = self.audio.lock() {
                let _ = backend.stop();
            }
            if let Some(clock) = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                clock.stop();
            }
            *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) =
                crate::contracts::types::PlaybackStatus::Stopped;
        }
        #[cfg(not(target_os = "macos"))]
        let _ = was_current;
        self.publish_queue_snapshot();
        Ok(Value::Null)
    }

    fn queue_reorder(&self, from: usize, to: usize) -> Result<Value, ContractError> {
        if !self.playback.lock().unwrap_or_else(|e| e.into_inner()).reorder(from, to) {
            return Err(ContractError::invalid("queue index is out of range"));
        }
        self.publish_queue_snapshot();
        Ok(Value::Null)
    }

    #[cfg(target_os = "macos")]
    fn play_track(&self, track_id: TrackId) -> Result<Value, ContractError> {
        self.play_with_context(track_id.clone(), vec![track_id])
    }

    #[cfg(target_os = "macos")]
    fn play_with_context(&self, track_id: TrackId, context: Vec<TrackId>) -> Result<Value, ContractError> {
        eprintln!("[luno-play] request {track_id:?} context_len={}", context.len());
        if context.is_empty() || !context.iter().any(|id| id == &track_id) {
            eprintln!("[luno-play] rejected: context must contain the selected track");
            return Err(ContractError::invalid("playback context must contain the selected track"));
        }
        // RESOLVE + HEAL THE WHOLE REQUEST UNDER ONE LOCK: a click that races
        // a rescan carries ids minted from the PREVIOUS snapshot, and when
        // duplicate-stem files let scan order decide which copy keeps the
        // plain id (vs `#<path-hash>`), EVERY entry can flip — not just the
        // clicked one. Healing ONLY the clicked track left the stored queue
        // full of dangling ids: auto-advance skipped, the queue panel and
        // Now Playing resolved nothing (the song the user just started
        // "disappeared"), and repeat clicks kept failing. Heal all of it:
        // effective track id, every context entry, then validate.
        let (track_id, context, path) = {
            let map = self.track_paths.lock().unwrap_or_else(|e| e.into_inner());
            let resolve = |id: &TrackId| -> Option<TrackId> {
                if map.contains_key(id) {
                    return Some(id.clone());
                }
                heal_track_id(&map, id).inspect(|healed| {
                    eprintln!("[luno-play] healed stale id {id:?} -> {healed:?}");
                })
            };
            let Some(effective) = resolve(&track_id).or_else(|| {
                self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|mode| mode.path_for(&track_id).map(|_| track_id.clone()))
            }) else {
                eprintln!("[luno-play] rejected: track not found locally: {track_id:?}");
                return Err(ContractError {
                    code: ErrorCode::NotFound,
                    message: "track is not available locally".into(),
                    retryable: false,
                });
            };
            let mut healed_context: Vec<TrackId> = Vec::with_capacity(context.len());
            let mut seen = std::collections::HashSet::new();
            for id in context {
                if let Some(healed) = resolve(&id) {
                    if seen.insert(healed.0.clone()) {
                        healed_context.push(healed);
                    }
                }
            }
            if !healed_context.iter().any(|id| id == &effective) {
                healed_context.insert(0, effective.clone());
            }
            let Some(path) = map.get(&effective).cloned().or_else(|| {
                self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|mode| mode.path_for(&effective).cloned())
            }) else {
                eprintln!("[luno-play] rejected: healed track has no file: {effective:?}");
                return Err(ContractError {
                    code: ErrorCode::NotFound,
                    message: "track is not available locally".into(),
                    retryable: false,
                });
            };
            (effective, healed_context, path)
        };
        eprintln!("[luno-play] resolved -> {} queue_len={}", path.display(), context.len());
        // A library selection is an explicit ownership transfer. End either
        // temporary presentation before replacing the queue so stale workers
        // cannot keep publishing against the new physical context.
        if self.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|session| session.is_active()) {
            let _ = self.leave_preview_for_library();
        }
        if self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|mode| mode.is_active()) {
            self.end_discover(false);
        }
        self.tasks.cancel_key("preview-heartbeat");
        self.playback.lock().unwrap_or_else(|e| e.into_inner()).play_with_context(track_id.clone(), context);
        // NOTE: video mode is NOT kicked here on purpose — `start_selected_track`
        // kicks it once the track actually starts playing (and auto-advance funnels
        // through the same path). Kicking in BOTH places spawned two prefetch passes
        // per switch (the second superseding the first mid-search), doubling yt-dlp
        // churn for zero benefit.
        // Acknowledge the selection immediately. Decoding runs on a runtime
        // task (`playback-load`) so the UI thread never blocks on audio
        // loading; a newer selection cancels the previous task by key.
        let ack = self.playback_snapshot(
            *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()),
            Duration::ZERO,
            None,
        );
        self.publish_playback(ack.clone(), Some(track_id.clone()));
        let load_state = self.clone();
        let generation = ack.generation;
        self.submit(self.generation("playback-load", generation), move |token| {
            load_state.start_selected_track(track_id, path, token);
        })
        .map_err(|_| ContractError {
            code: ErrorCode::Busy,
            message: "playback worker is unavailable".into(),
            retryable: true,
        })?;
        serde_json::to_value(ack).map_err(|e| ContractError {
            code: ErrorCode::Internal,
            message: e.to_string(),
            retryable: false,
        })
    }

    /// Decode and start a selected track on the runtime task thread. Decoding
    /// never holds the audio lock, so pause/seek/next commands stay responsive
    /// while a large file loads; a superseded request (cancelled token) stops
    /// decoding at the next packet and never starts playback.
    #[cfg(target_os = "macos")]
    fn start_selected_track(
        &self,
        track_id: TrackId,
        path: PathBuf,
        token: crate::runtime::task_registry::CancellationToken,
    ) {
        if token.is_cancelled() || self.is_closing() {
            eprintln!("[luno-play] load cancelled for {track_id:?}");
            return;
        }
        // Snapshot the intended volume before locking audio so `set_volume`
        // (volume-then-audio lock order) can never deadlock against this task.
        let desired_volume = *self.volume.lock().unwrap_or_else(|e| e.into_inner());
        // Do not synchronously probe the file here. The streaming producer
        // already has to open the container, and the old duplicate pre-probe
        // was the entire multi-second first-click delay on cold/cloud files.
        // Use scan/provider duration when known; otherwise the producer fills
        // it after opening while the output stream is already live.
        let known_duration = self.library.lock().unwrap_or_else(|e| e.into_inner())
            .tracks.iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.duration_ms)
            .map(std::time::Duration::from_millis);
        if token.is_cancelled() || self.is_closing() {
            return;
        }
        let info = {
            let mut backend = match self.audio.lock() {
                Ok(backend) => backend,
                Err(_) => {
                    eprintln!("[luno-play] audio lock poisoned");
                    return;
                }
            };
            match backend.load_streaming_with_duration(&path, known_duration) {
                Ok(info) => {
                    // A fresh load replaces the output state; reapply the
                    // intended volume so the new track starts at the level the
                    // user set (covers tracks chosen before any playback).
                    backend.set_volume(desired_volume);
                    info
                }
                Err(error) => {
                    drop(backend);
                    eprintln!("[luno-play] load_streaming failed: {error:?}");
                    self.recover_from_backend_error(error);
                    return;
                }
            }
        };
        if token.is_cancelled() || self.is_closing() {
            return;
        }
        {
            let mut backend = match self.audio.lock() {
                Ok(backend) => backend,
                Err(_) => return,
            };
            if let Err(error) = backend.play() {
                drop(backend);
                eprintln!("[luno-play] backend.play failed: {error:?}");
                self.recover_from_backend_error(error);
                return;
            }
        }
        eprintln!("[luno-play] playing {track_id:?} ({}ms)", info.duration.map(|d| d.as_millis()).unwrap_or(0));
        self.playback.lock().unwrap_or_else(|e| e.into_inner()).record_started(track_id.clone(), false);
        self.discover_track_started(&track_id);
        self.record_recent(track_id.clone());
        // Auto-advance reached a new track: keep video mode in lockstep
        // (stage the new current, prefetch its successors).
        if self.video_mode_enabled() {
            self.kick_video_pipeline();
        }
        *self.clock.lock().unwrap_or_else(|e| e.into_inner()) = Some(PlaybackClock::new(info.duration));
        if let Some(clock) = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() { clock.start(0.0); }
        self.recovery.lock().unwrap_or_else(|e| e.into_inner()).record_success();
        *self.current_track.lock().unwrap_or_else(|e| e.into_inner()) = Some(track_id.clone());
        *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) = crate::contracts::types::PlaybackStatus::Playing;
        let snapshot = self.playback_snapshot(
            crate::contracts::types::PlaybackStatus::Playing,
            Duration::ZERO,
            info.duration,
        );
        self.publish_playback(snapshot.clone(), Some(track_id));
        let heartbeat_state = self.clone();
        let generation = snapshot.generation;
        let _ = self.submit(self.generation("playback-heartbeat", generation), move |token| {
            // One transient tick failure must NEVER kill the heartbeat: a dead
            // heartbeat froze every snapshot (scrubber, status, queue) until
            // restart, which read as "the app stopped responding".
            let mut consecutive_errors = 0u32;
            while !token.is_cancelled() && !heartbeat_state.is_closing() {
                std::thread::sleep(crate::playback::clock::HEARTBEAT);
                if token.is_cancelled() || heartbeat_state.is_closing() { break; }
                match heartbeat_state.heartbeat_tick() {
                    Ok(()) => consecutive_errors = 0,
                    Err(error) => {
                        eprintln!("[luno-play] heartbeat tick failed: {error:?}");
                        consecutive_errors += 1;
                        if consecutive_errors >= 10 { break; }
                    }
                }
            }
        });
    }

    #[cfg(target_os = "macos")]
    /// Starts audio for the track ALREADY current in the queue WITHOUT
    /// touching queue structure. next/previous/auto-advance walk the existing
    /// (possibly shuffled) order; routing those through `play_with_context`
    /// made every skip RE-SHUFFLE the whole queue current-first under shuffle,
    /// which pinned previous() at index 0 ("can't go back a couple songs"),
    /// mutated the shuffle order on every press, and clobbered the original
    /// order so toggling shuffle off restored the shuffled sequence.
    #[cfg(target_os = "macos")]
    fn start_current_track(&self) -> Result<Value, ContractError> {
        let Some(track_id) = self.playback.lock().unwrap_or_else(|e| e.into_inner()).current().cloned() else {
            return Ok(Value::Null);
        };
        // Same resolve+heal contract as play_with_context: a skip that races a
        // rescan must not resurrect the dangling-id family.
        let path = {
            let map = self.track_paths.lock().unwrap_or_else(|e| e.into_inner());
            match map.get(&track_id) {
                Some(path) => Some(path.clone()),
                None => heal_track_id(&map, &track_id).and_then(|healed| {
                    eprintln!("[luno-play] healed stale id {track_id:?} -> {healed:?}");
                    map.get(&healed).cloned()
                }).or_else(|| self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|mode| mode.path_for(&track_id).cloned())),
            }
        };
        let Some(path) = path else {
            // Discover mode projects recommendation ids before their asset is
            // staged. Selecting one prepares it and HOLDS playback on that
            // selection; the discover driver starts it once the asset lands.
            // Failing here instead fell back to a library track ("Next plays my
            // library").
            let discover_generation = self
                .discover
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .filter(|mode| mode.is_active() && mode.recommendations().contains(&track_id))
                .map(|mode| mode.generation());
            let Some(generation) = discover_generation else {
                eprintln!("[luno-play] rejected: track not found locally: {track_id:?}");
                return Err(ContractError {
                    code: ErrorCode::NotFound,
                    message: "track is not available locally".into(),
                    retryable: false,
                });
            };
            if let Ok(mut backend) = self.audio.lock() { let _ = backend.stop(); }
            self.tasks.cancel_key("playback-heartbeat");
            self.tasks.cancel_key("preview-heartbeat");
            if let Some(clock) = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() { clock.stop(); }
            *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) = crate::contracts::types::PlaybackStatus::Stopped;
            self.ensure_discover_prepared(generation, &track_id);
            let ack = self.playback_snapshot(
                crate::contracts::types::PlaybackStatus::Stopped,
                Duration::ZERO,
                None,
            );
            // No TrackStarted: the recommendation has not actually played yet,
            // so it must not enter recent history or count as a play.
            self.publish_playback(ack.clone(), None);
            return serde_json::to_value(ack).map_err(|e| ContractError {
                code: ErrorCode::Internal,
                message: e.to_string(),
                retryable: false,
            });
        };
        self.tasks.cancel_key("preview-heartbeat");
        let ack = self.playback_snapshot(
            *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()),
            Duration::ZERO,
            None,
        );
        self.publish_playback(ack.clone(), Some(track_id.clone()));
        let load_state = self.clone();
        let generation = ack.generation;
        self.submit(self.generation("playback-load", generation), move |token| {
            load_state.start_selected_track(track_id, path, token);
        })
        .map_err(|_| ContractError {
            code: ErrorCode::Busy,
            message: "playback worker is unavailable".into(),
            retryable: true,
        })?;
        serde_json::to_value(ack).map_err(|e| ContractError {
            code: ErrorCode::Internal,
            message: e.to_string(),
            retryable: false,
        })
    }

    #[cfg(target_os = "macos")]
    fn leave_preview_for_library(&self) -> Option<Handoff> {
        let generation = self.next_generation().0;
        let handoff = self
            .preview_session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
            .filter(|session| session.is_active())
            .map(|session| session.invalidate(generation));
        self.cancel_preview_tasks();
        // Completed recommendation files belong to the active seed, not to
        // the focused presentation mode. Keep them available for replay,
        // discovery handoff, and Save; a new seed or shutdown owns cleanup.
        if let Ok(mut backend) = self.audio.lock() { let _ = backend.stop(); }
        self.publish_preview_snapshot(generation);
        handoff
    }

    #[cfg(target_os = "macos")]
    fn next_track(&self) -> Result<Value, ContractError> {
        let handoff = if self.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|session| session.is_active())
            && !self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|mode| mode.is_active())
        {
            self.leave_preview_for_library()
        } else { None };
        if let Some(handoff) = handoff {
            if let Some(index) = handoff.index {
                self.playback.lock().unwrap_or_else(|e| e.into_inner()).replace(handoff.queue, index);
            }
        }
        let advanced = self.playback.lock().unwrap_or_else(|e| e.into_inner()).next().cloned();
        if advanced.is_some() {
            self.start_current_track()
        } else {
            // The queue is exhausted. An active Discover session hands the
            // block back: restore the captured queue at the seed position so
            // the library is intact when playback resumes.
            self.end_discover(true);
            if let Ok(mut backend) = self.audio.lock() { backend.stop(); }
            if let Some(clock) = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() { clock.stop(); }
            *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) = crate::contracts::types::PlaybackStatus::Stopped;
            self.publish_queue_snapshot();
            Ok(Value::Null)
        }
    }

    #[cfg(target_os = "macos")]
    fn previous_track(&self) -> Result<Value, ContractError> {
        let handoff = if self.preview_session.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|session| session.is_active())
            && !self.discover.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|mode| mode.is_active())
        {
            self.leave_preview_for_library()
        } else { None };
        if let Some(handoff) = handoff {
            if let Some(index) = handoff.index {
                self.playback.lock().unwrap_or_else(|e| e.into_inner()).replace(handoff.queue, index);
            }
        }
        let went_back = self.playback.lock().unwrap_or_else(|e| e.into_inner()).previous().cloned();
        if went_back.is_some() {
            self.start_current_track()
        } else {
            // At the start of the physical queue, Previous restores/restarts
            // the seed. Leaving a preview must never stop output silently.
            self.start_current_track()
        }
    }

    #[cfg(target_os = "macos")]
    fn pause_track(&self) -> Result<Value, ContractError> {
        let mut backend = self.audio.lock().map_err(|_| ContractError {
            code: ErrorCode::Internal,
            message: "audio backend lock failed".into(),
            retryable: false,
        })?;
        let was_paused = *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) == crate::contracts::types::PlaybackStatus::Paused;
        if was_paused { backend.play().map_err(Self::playback_error)?; }
        else { backend.pause().map_err(Self::playback_error)?; }
        // Publish the new status IMMEDIATELY: the heartbeat thread samples
        // playback_status + video_current independently, and if it ran between
        // "audio paused" and "status written" it saw (Playing, Paused) — which
        // its resume arm interprets as "audio restarted" — and flipped the
        // freshly-paused video back to Playing (2026-08-24).
        *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) = if was_paused { crate::contracts::types::PlaybackStatus::Playing } else { crate::contracts::types::PlaybackStatus::Paused };
        let position = match backend.position() {
            Ok(position) => position,
            Err(error) => { drop(backend); self.recover_from_backend_error(error); return Ok(Value::Null); }
        };
        let duration = match backend.duration() {
            Ok(duration) => duration,
            Err(error) => { drop(backend); self.recover_from_backend_error(error); return Ok(Value::Null); }
        };
        // Freeze/unfreeze the video in the SAME breath as the audio so the
        // user never sees a lag between pressing pause and the video stopping
        // (waiting for the 400ms heartbeat made the video feel detached).
        // The video_current lock MUST be scoped so it is RELEASED before
        // publish_video_state() runs — it re-acquires the same mutex, and a
        // publish under the held guard deadlocks the command thread (which
        // froze pause/resume and every later command; 2026-08-23).
        let mut video_followed = false;
        {
            let mut video_guard = self.video_current.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(video) = video_guard.as_mut() {
                if matches!(video.status, crate::contracts::events::VideoStatus::Playing | crate::contracts::events::VideoStatus::Ready) && !was_paused {
                    video.status = crate::contracts::events::VideoStatus::Paused;
                    video.position_ms = position.as_millis() as u64;
                    video_followed = true;
                } else if matches!(video.status, crate::contracts::events::VideoStatus::Paused) && was_paused {
                    // This click is a RESUME (the button toggles).
                    video.status = crate::contracts::events::VideoStatus::Playing;
                    video.position_ms = position.as_millis() as u64;
                    video_followed = true;
                }
            }
        }
        if video_followed {
            self.publish_video_state();
        }
        if was_paused {
            if let Some(clock) = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() { clock.resume(0.0); }
        } else if let Some(clock) = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            clock.pause(0.0, Some(position));
        }
        let snapshot = self.playback_snapshot(
            *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()),
            position,
            duration,
        );
        self.publish_playback(snapshot.clone(), None);
        serde_json::to_value(snapshot).map_err(|e| ContractError {
            code: ErrorCode::Internal,
            message: e.to_string(),
            retryable: false,
        })
    }

    /// Resumes a paused track WITHOUT reloading it — unlike `play_with_context`,
    /// which restarts from zero. A no-op when nothing is paused.
    #[cfg(target_os = "macos")]
    fn resume_track(&self) -> Result<Value, ContractError> {
        if *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) != crate::contracts::types::PlaybackStatus::Paused {
            return Ok(Value::Null);
        }
        // CATCH-UP KICK (2026-08-25): resuming after a long pause must find
        // the video ALREADY staged — "ready to go", not loading on resume. If
        // the current track's slot never resolved (dead Loading corpse,
        // missed kick, Unavailable retry), one idempotent pipeline pass runs
        // here, off the UI thread, before/while the audio resumes.
        if self.video_mode_enabled() {
            let current = self.playback.lock().unwrap_or_else(|e| e.into_inner()).current().cloned();
            if let Some(track) = current {
                if !self.current_video_live(&track) {
                    self.kick_video_pipeline();
                }
            }
        }
        let mut backend = self.audio.lock().map_err(|_| ContractError {
            code: ErrorCode::Internal,
            message: "audio backend lock failed".into(),
            retryable: false,
        })?;
        backend.play().map_err(Self::playback_error)?;
        let position = backend.position().unwrap_or_default();
        let duration = backend.duration().ok().flatten();
        drop(backend);
        // Resume the video in the same breath as the audio — the footer's
        // resume path goes through resume_track, which previously never
        // touched the video and left it frozen while the song played.
        let mut video_resumed = false;
        {
            let mut video_guard = self.video_current.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(video) = video_guard.as_mut() {
                if matches!(video.status, crate::contracts::events::VideoStatus::Paused) {
                    video.status = crate::contracts::events::VideoStatus::Playing;
                    video.position_ms = position.as_millis() as u64;
                    video_resumed = true;
                }
            }
        }
        // Publish OUTSIDE the guard: publish_video_state re-acquires the
        // same mutex — publishing under the held guard deadlocks (2026-08-23).
        if video_resumed {
            self.publish_video_state();
        }
        if let Some(clock) = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() { clock.resume(0.0); }
        *self.playback_status.lock().unwrap_or_else(|e| e.into_inner()) = crate::contracts::types::PlaybackStatus::Playing;
        let snapshot = self.playback_snapshot(
            crate::contracts::types::PlaybackStatus::Playing,
            position,
            duration,
        );
        self.publish_playback(snapshot.clone(), None);
        serde_json::to_value(snapshot).map_err(|e| ContractError {
            code: ErrorCode::Internal,
            message: e.to_string(),
            retryable: false,
        })
    }

    #[cfg(target_os = "macos")]
    fn seek_track(&self, position_ms: u64) -> Result<Value, ContractError> {
        let mut backend = self.audio.lock().map_err(|_| ContractError {
            code: ErrorCode::Internal,
            message: "audio backend lock failed".into(),
            retryable: false,
        })?;
        backend
            .seek(Duration::from_millis(position_ms))
            .map_err(Self::playback_error)?;
        let position = backend.position().map_err(Self::playback_error)?;
        let duration = backend.duration().map_err(Self::playback_error)?;
        if let Some(clock) = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            clock.seek(position, 0.0);
        }
        // The video must land on the same page instantly: push the new audio
        // position into the video state and republish. The frontend clamps to
        // the staged video's duration and self-corrects residual drift.
        {
            let mut video_guard = self.video_current.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(video) = video_guard.as_mut() {
                if matches!(video.status, crate::contracts::events::VideoStatus::Playing | crate::contracts::events::VideoStatus::Paused) {
                    video.position_ms = position.as_millis() as u64;
                    drop(video_guard);
                    // Publish AFTER releasing video_current (deadlock rule).
                    self.publish_video_state();
                }
            }
        }
        let status = *self.playback_status.lock().unwrap_or_else(|e| e.into_inner());
        let snapshot = self.playback_snapshot(status, position, duration);
        self.publish_playback(snapshot.clone(), None);
        serde_json::to_value(snapshot).map_err(|e| ContractError {
            code: ErrorCode::Internal,
            message: e.to_string(),
            retryable: false,
        })
    }

    /// Hardware media-key entry point (MPRemoteCommandCenter handlers land
    /// here through main.rs). Status-aware: toggle pauses a playing song,
    /// resumes a paused one, and starts the current track when stopped.
    /// The Settings ▸ Playback "media keys" switch is honored live.
    #[cfg(target_os = "macos")]
    pub fn handle_media_key(&self, key: crate::media_keys::MediaKey) {
        use crate::contracts::{events::Command, types::PlaybackStatus};
        if !self.media_keys_enabled.load(Ordering::Acquire) {
            return;
        }
        static MEDIA_KEY_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let status = *self.playback_status.lock().unwrap_or_else(|e| e.into_inner());
        let current = self.playback.lock().unwrap_or_else(|e| e.into_inner()).current().cloned();
        let command = match key {
            MediaKey::Toggle => match status {
                PlaybackStatus::Playing => Command::Pause,
                PlaybackStatus::Paused => Command::Resume,
                _ => match current {
                    Some(track_id) => Command::Play { track_id },
                    None => return,
                },
            },
            MediaKey::Play => match status {
                PlaybackStatus::Playing => return,
                PlaybackStatus::Paused => Command::Resume,
                _ => match current {
                    Some(track_id) => Command::Play { track_id },
                    None => return,
                },
            },
            MediaKey::Pause => {
                if status == PlaybackStatus::Playing { Command::Pause } else { return; }
            }
            MediaKey::Next => Command::Next,
            MediaKey::Previous => Command::Previous,
        };
        let seq = MEDIA_KEY_SEQ.fetch_add(1, Ordering::Relaxed);
        let envelope = crate::contracts::events::CommandEnvelope {
            version: 1,
            request_id: format!("media-key-{}-{seq}", std::process::id()),
            command,
        };
        if let Err(error) = self.dispatch(envelope) {
            eprintln!("[luno-media] key dispatch failed: {error:?}");
        }
    }

    #[cfg(target_os = "macos")]
    fn set_volume(&self, volume: f32) -> Result<Value, ContractError> {
        let volume = volume.clamp(0.0, 1.0);
        // Record the intended volume first (guards are dropped before the
        // audio lock is taken below, keeping a single lock order), then apply
        // it to the live backend so current playback responds immediately.
        *self.volume.lock().unwrap_or_else(|e| e.into_inner()) = volume;
        if let Ok(mut backend) = self.audio.lock() {
            backend.set_volume(volume);
        }
        Ok(Value::Null)
    }

    #[cfg(target_os = "macos")]
    fn heartbeat_tick(&self) -> Result<(), ContractError> {
        let mut backend = self.audio.lock().map_err(|_| ContractError { code: ErrorCode::Internal, message: "audio backend lock failed".into(), retryable: false })?;
        let position = match backend.position() {
            Ok(position) => position,
            Err(error) => {
                drop(backend);
                self.recover_from_backend_error(error);
                return Err(ContractError { code: ErrorCode::Unavailable, message: "audio position is unavailable".into(), retryable: true });
            }
        };
        let duration = match backend.duration() {
            Ok(duration) => duration,
            Err(error) => {
                drop(backend);
                self.recover_from_backend_error(error);
                return Err(ContractError { code: ErrorCode::Unavailable, message: "audio duration is unavailable".into(), retryable: true });
            }
        };
        let status = *self.playback_status.lock().unwrap_or_else(|e| e.into_inner());
        let elapsed = position;
        // ---- Video follow: auto-play/pause + COARSE clock sync --------------
        // Event-driven, not poll-driven: publish ONLY on play/pause transitions
        // or a genuine >2s drift. Streaming an update every heartbeat tick made
        // the <video> micro-seek constantly (stop-start stutter) — between real
        // corrections the element plays freely in real time alongside the song.
        let elapsed_ms = elapsed.as_millis() as u64;
        let mut video_needs_publish = false;
        {
            let mut video = self.video_current.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(state) = video.as_mut() {
                match (status, state.status) {
                    (crate::contracts::types::PlaybackStatus::Playing, crate::contracts::events::VideoStatus::Ready) => {
                        state.status = crate::contracts::events::VideoStatus::Playing;
                        state.position_ms = elapsed_ms;
                        video_needs_publish = true;
                    }
                    (crate::contracts::types::PlaybackStatus::Playing, crate::contracts::events::VideoStatus::Playing) => {
                        // Publish EVERY tick during normal playback so the UI
                        // always holds a fresh audio position for the video:
                        // scrubs must propagate and mode re-enables must mount
                        // at the true song position. The earlier stutter came
                        // from the frontend re-seeking on every stale update;
                        // VideoStage now ignores stale-by-arrival positions
                        // while actively playing and only acts on real drift.
                        state.position_ms = elapsed_ms;
                        video_needs_publish = true;
                    }
                    (crate::contracts::types::PlaybackStatus::Playing, crate::contracts::events::VideoStatus::Paused) => {
                        // Resume: audio restarted after a pause — follow it.
                        state.status = crate::contracts::events::VideoStatus::Playing;
                        state.position_ms = elapsed_ms;
                        video_needs_publish = true;
                    }
                    (_, crate::contracts::events::VideoStatus::Playing) => {
                        // Audio paused/stopped/died — the video must follow.
                        state.status = crate::contracts::events::VideoStatus::Paused;
                        video_needs_publish = true;
                    }
                    _ => {}
                }
            }
        }
        if video_needs_publish {
            self.publish_video_state();
        }
        // ----------------------------------------------------------------------
        // ---- Stall watchdog (mixer-idle completion parity) -------------------
        // While Playing, a healthy stream advances `position` every tick. A
        // silently-dead output stream freezes it. First: nudge the backend
        // (rebuilds a flagged stream). If it is STILL frozen one window later,
        // treat the track as finished and advance — exactly what the Tk
        // engine's mixer-idle detection did.
        enum StallAction { None, Rebuild, Advance }
        let stall_action = if status == crate::contracts::types::PlaybackStatus::Playing {
            const REBUILD_AFTER_TICKS: u32 = 10; // ~4 s frozen
            const ADVANCE_AFTER_TICKS: u32 = 20; // ~8 s frozen
            let mut stall = self.playback_stall.lock().unwrap_or_else(|e| e.into_inner());
            match *stall {
                Some((last, ticks)) if last == position => {
                    let next = ticks + 1;
                    let action = if next == REBUILD_AFTER_TICKS {
                        eprintln!("[luno-play] position stalled at {:?}; rebuilding output stream", position);
                        StallAction::Rebuild
                    } else if next >= ADVANCE_AFTER_TICKS {
                        eprintln!("[luno-play] still stalled after rebuild; advancing to next track");
                        StallAction::Advance
                    } else {
                        StallAction::None
                    };
                    // Reset on Advance so the NEXT track's load window cannot
                    // inherit this counter and trigger an immediate skip.
                    if matches!(action, StallAction::Advance) { *stall = None; }
                    else { *stall = Some((position, next)); }
                    action
                }
                _ => {
                    *stall = Some((position, 0));
                    StallAction::None
                }
            }
        } else {
            *self.playback_stall.lock().unwrap_or_else(|e| e.into_inner()) = None;
            StallAction::None
        };
        match stall_action {
            StallAction::Rebuild => { let _ = backend.play(); }
            StallAction::Advance => {
                drop(backend);
                let _ = self.next_track()?;
                return Ok(());
            }
            StallAction::None => {}
        }
        // ----------------------------------------------------------------------
        let due = self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|clock| clock.completion_due(elapsed, true, elapsed));
        if due && self.clock.lock().unwrap_or_else(|e| e.into_inner()).as_mut().is_some_and(|clock| clock.dispatch_completion()) {
            drop(backend);
            let _ = self.next_track()?;
        } else {
            let snapshot = self.playback_snapshot(status, position, duration);
            self.publish_playback(snapshot, None);
        }
        Ok(())
    }

    pub fn shutdown_events(
        &self,
        reason: crate::contracts::events::ShutdownReason,
        deadline: Duration,
    ) -> ShutdownReport {
        let admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        self.shutdown_events_locked(reason, deadline, &admission)
    }

    fn shutdown_events_locked(
        &self,
        reason: crate::contracts::events::ShutdownReason,
        deadline: Duration,
        _admission: &std::sync::MutexGuard<'_, ()>,
    ) -> ShutdownReport {
        let mut stored_result = self.shutdown_result.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(report) = *stored_result {
            return report;
        }
        self.tasks.begin_shutdown();
        #[cfg(target_os = "macos")]
        if let Ok(mut backend) = self.audio.lock() {
            backend.shutdown();
        }
        self.publish(self.event(
            Generation(0),
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    version: CONTRACT_VERSION,
                    state: ShutdownState::Requested,
                    reason,
                },
            },
        ));
        self.publish(self.event(
            Generation(0),
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    version: CONTRACT_VERSION,
                    state: ShutdownState::Draining,
                    reason,
                },
            },
        ));
        let started = std::time::Instant::now();
        let report = self.shutdown(deadline);
        let remaining = deadline.saturating_sub(started.elapsed());
        let process_ok = self.supervisor.shutdown(remaining);
        let remaining = deadline.saturating_sub(started.elapsed());
        let download_report = self.downloads.shutdown(remaining);
        let remaining = deadline.saturating_sub(started.elapsed());
        let pending_before_terminal = self.retry_event_delivery(remaining);
        let terminal_timed_out = report.timed_out
            || !process_ok
            || download_report.timed_out
            || pending_before_terminal > 0;
        let failed = report.failed;
        let state = if failed {
            ShutdownState::Failed
        } else if terminal_timed_out {
            ShutdownState::TimedOut
        } else {
            ShutdownState::Complete
        };
        self.publish(self.event(
            Generation(0),
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    version: CONTRACT_VERSION,
                    state,
                    reason,
                },
            },
        ));
        if failed {
            self.publish(self.event(
                Generation(0),
                Event::Error {
                    error: ContractError {
                        code: ErrorCode::Internal,
                        message: "shutdown failed because a worker panicked".into(),
                        retryable: false,
                    },
                },
            ));
        } else if terminal_timed_out {
            self.publish(self.event(
                Generation(0),
                Event::Error {
                    error: ContractError {
                        code: ErrorCode::Internal,
                        message: "shutdown timed out; workers remain active".into(),
                        retryable: false,
                    },
                },
            ));
        }
        let remaining = deadline.saturating_sub(started.elapsed());
        let undelivered = self.retry_event_delivery(remaining);
        // The terminal drain is the final authority for the shutdown report.
        let result = ShutdownReport {
            joined: report.joined,
            timed_out: !failed && (terminal_timed_out || undelivered > 0),
            failed,
            undelivered,
        };
        *stored_result = Some(result);
        result
    }

    fn retry_event_delivery(&self, deadline: Duration) -> usize {
        let started = std::time::Instant::now();
        loop {
            let pending = self.pending_events().len();
            let in_flight = *self.event_in_flight.lock().unwrap_or_else(|e| e.into_inner());
            let active = *self.event_draining.lock().unwrap_or_else(|e| e.into_inner());
            let remaining_events = pending + in_flight.max(usize::from(active));
            let Some(sink) = self.event_sink.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
                return remaining_events;
            };
            let drained = self.drain_events(|event| sink(event.clone()));
            let pending = self.pending_events().len();
            let in_flight = *self.event_in_flight.lock().unwrap_or_else(|e| e.into_inner());
            let active = *self.event_draining.lock().unwrap_or_else(|e| e.into_inner());
            let remaining = pending + in_flight.max(usize::from(active));
            if remaining == 0 && !matches!(drained, Ok(false)) {
                return 0;
            }
            if started.elapsed() >= deadline {
                return remaining;
            }
            std::thread::yield_now();
        }
    }

    pub fn run_process(
        &self,
        spec: crate::process::supervisor::ProcessSpec,
    ) -> Result<crate::process::supervisor::ProcessOutput, crate::process::supervisor::SuperviseError>
    {
        let _admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        self.supervisor.run_admitted(spec, &self.tasks)
    }
    pub fn process_count(&self) -> usize {
        self.supervisor.registry.active_count()
    }
}

fn validate_home(home: &PathBuf) -> Result<(), ContractError> {
    if !home.is_absolute() || home.as_os_str().is_empty() || home.to_string_lossy().chars().any(char::is_control) || !home.is_dir() || std::fs::canonicalize(home).is_err() {
        return Err(ContractError::invalid("settings home must be an absolute safe directory"));
    }
    Ok(())
}

fn playlist_mutation_error(error: crate::library::mutations::MutationError) -> ContractError {
    use crate::library::mutations::MutationError;
    let code = match &error { MutationError::Io { .. } | MutationError::PartialCopy(_) => ErrorCode::Internal, MutationError::NotFound(_) => ErrorCode::NotFound, _ => ErrorCode::InvalidRequest };
    ContractError { code, message: error.to_string(), retryable: matches!(code, ErrorCode::Internal) }
}

fn validate_library_path(state: &AppState, path: &Path, allow_file: bool) -> Result<PathBuf, ContractError> {
    let root = state.scan_roots().first().cloned().ok_or_else(|| ContractError::invalid("no configured library root"))?;
    let root = std::fs::canonicalize(root).map_err(|_| ContractError { code: ErrorCode::NotFound, message: "configured library root is unavailable".into(), retryable: false })?;
    if !path.is_absolute() || path.to_string_lossy().chars().any(char::is_control) { return Err(ContractError::invalid("path must be absolute and safe")); }
    let candidate = if path.exists() { std::fs::canonicalize(path) } else { path.parent().map(std::fs::canonicalize).unwrap_or_else(|| Err(std::io::Error::other("missing parent"))) };
    let candidate = candidate.map_err(|_| ContractError::invalid("path is unavailable"))?;
    if !candidate.starts_with(&root) || (!allow_file && !path.is_dir()) { return Err(ContractError::invalid("path is outside the configured library")); }
    Ok(candidate)
}

/// Sanitize a user-supplied artist/title component into a safe filename stem.
/// Mirrors the frontend download sanitizer so an edited name and a downloaded
/// name are treated identically by the scanner.
fn sanitize_track_stem(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() && !"<>:\"/\\|?*".contains(*character))
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .trim()
        .to_owned()
}

fn sanitize_download_error(error: &str) -> String {
    let first = error.lines().next().unwrap_or("download failed");
    if first.contains('/') || first.contains('\\') { "download worker failed".into() } else { first.chars().take(160).collect() }
}
