use crate::contracts::types::Generation;
use crate::contracts::TrackId;
use crate::discovery::artwork::{design_index, render_design, ArtworkService, MAX_CACHE};
use crate::discovery::service::{
    DiscoveryError, DiscoveryResult, DiscoveryService, Recommendation,
};
use crate::library::scanner::{scan_library, OsFileSystem};
use crate::preview::assets::{AssetError, PreviewAssetStore};
use crate::preview::discover_mode::{DiscoverAction, DiscoverMode, ReadyAsset};
use crate::preview::save::{promote_to_playlist, PlaylistDestination, SaveError};
use crate::preview::session::{PreviewSession, SessionAction, AHEAD_WINDOW};
use crate::runtime::shutdown::shutdown;
use crate::runtime::task_registry::{TaskRegistry, TaskSpec};
use crate::video::player::{
    Decoder, DecoderFactory, VideoEvent, VideoPlayer, VideoSize, VideoStatus,
};
use crate::video::source::{lookup, VideoCandidate};
use crate::video::staging::{Slot, StageError, StageState, VideoStage};
use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

struct Fixture(PathBuf);
impl std::ops::Deref for Fixture {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture(label: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!("luno-gate5-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    Fixture(root)
}
fn id(value: &str) -> TrackId {
    TrackId(value.into())
}

fn fake_server(responses: Vec<serde_json::Value>, status: u16) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        for value in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let body = serde_json::to_string(&value).unwrap();
            let reason = if status == 200 { "OK" } else { "Error" };
            write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    format!("http://{address}/2.0/")
}
fn recommendation(artist: &str, title: &str) -> Recommendation {
    Recommendation {
        artist: artist.into(),
        title: title.into(),
        match_score: 1.0,
        artwork_url: None,
    }
}

#[test]
fn ref_discovery_fake_provider_covers_fallback_filter_cache_and_states() {
    let mut missing = DiscoveryService::with_base_url(None, "http://127.0.0.1:1/");
    assert!(matches!(
        missing.get_similar("artist-a", "track-a", 5, &[]),
        DiscoveryResult::Error(DiscoveryError::MissingApiKey)
    ));
    let mut empty = DiscoveryService::with_base_url(Some("key".into()), "http://127.0.0.1:1/");
    assert_eq!(
        empty.get_similar("", "", 5, &[]),
        DiscoveryResult::Success(vec![])
    );

    let direct = fake_server(
        vec![
            json!({"similartracks": {"track": [
                {"name":"direct-a","artist":{"name":"artist-direct"},"match":0.9,"image":[{"size":"small","#text":"http://img.test/small.jpg"},{"size":"mega","#text":"http://img.test/mega.jpg"}]},
                {"name":"direct-b","artist":{"name":"artist-direct"},"match":0.8}
            ]}}),
            json!({"similartracks": {"track": [
                {"name":"limit-five","artist":{"name":"artist-direct"},"match":0.7}
            ]}}),
            json!({"similartracks": {"track": []}}),
            json!({"toptracks": {"track": []}}),
        ],
        200,
    );
    let mut direct_service = DiscoveryService::with_base_url(Some("fixture-key".into()), direct);
    assert!(matches!(
        direct_service.get_similar("artist-direct", "seed", 3, &[]),
        DiscoveryResult::Success(items)
            if items.len() == 2
                && items[0].artwork_url.as_deref() == Some("https://img.test/mega.jpg")
    ));
    assert!(matches!(
        direct_service.get_similar("artist-direct", "seed", 5, &[]),
        DiscoveryResult::Success(items)
            if items.len() == 1 && items[0].title == "limit-five"
    ));
    assert_eq!(
        direct_service.get_similar("artist-direct", "empty", 5, &[]),
        DiscoveryResult::Success(vec![])
    );

    let base = fake_server(
        vec![
            json!({"similartracks": {"track": []}}),
            json!({"toptracks": {"track": [
                {"name":"track-b","artist":{"name":"artist-b"},"match":0.7},
                {"name":"track-b","artist":{"name":"artist-b"},"match":0.6},
                {"name":"track-a","artist":{"name":"artist-a"},"match":0.5}
            ]}}),
        ],
        200,
    );
    let mut service = DiscoveryService::with_base_url(Some("fixture-key".into()), base);
    let result = service.get_similar(
        "artist-a",
        "track-a",
        5,
        &[recommendation("artist-a", "track-a")],
    );
    assert!(
        matches!(result, DiscoveryResult::Success(items) if items.len() == 1 && items[0].artist == "artist-b" && items[0].match_score == 0.8)
    );
    let cached = service.get_similar("artist-a", "track-a", 5, &[]);
    assert!(
        matches!(cached, DiscoveryResult::Success(items) if items.len() == 2 && items.iter().any(|item| item.artist == "artist-a") && items.iter().all(|item| item.match_score == 0.8))
    );

    let error_url = fake_server(vec![json!({"error": 6, "message": "invalid key"})], 500);
    let mut error = DiscoveryService::with_base_url(Some("bad".into()), error_url);
    assert!(matches!(
        error.get_similar("artist-a", "track-a", 5, &[]),
        DiscoveryResult::Error(DiscoveryError::Request(_))
    ));
}

#[test]
fn ref_artwork_fake_provider_is_cached_bounded_and_deterministic() {
    let response = json!({"data":[{"album":{"cover_xl":"http://img.test/cover.jpg"}}]});
    let base = fake_server(vec![response; MAX_CACHE + 2], 200);
    let mut service = ArtworkService::with_base_url(base);
    assert_eq!(
        service.find_artwork("artist-a", "track-a").as_deref(),
        Some("https://img.test/cover.jpg")
    );
    for index in 0..MAX_CACHE {
        assert!(service
            .find_artwork(&format!("artist-{index}"), "track-a")
            .is_some());
    }
    assert!(service.find_artwork("artist-a", "track-a").is_some());
    assert_eq!(
        service.find_artwork("artist-a", "track-a").as_deref(),
        Some("https://img.test/cover.jpg")
    );
    assert_eq!(
        design_index("Artist A", "Track A"),
        design_index("Artist A", "Track A")
    );
    assert_eq!(
        render_design(design_index("Artist A", "Track A"), 96).size,
        96
    );
    assert_eq!(MAX_CACHE, 128);
}

#[test]
fn ref_preview_asset_isolated_atomic_and_generation_guarded() {
    let root = fixture("preview");
    let library = root.join("library");
    let preview = root.join("preview");
    fs::create_dir_all(&library).unwrap();
    let mut store = PreviewAssetStore::at(&preview).unwrap();
    let generation = store.begin_generation();
    let cancel = AtomicBool::new(false);
    let path = store
        .download("artist-a|track-a", generation, &cancel, |writer| {
            writer
                .write_all(b"ID3 synthetic preview")
                .map_err(|e| e.to_string())
        })
        .unwrap();
    assert!(path.starts_with(&preview));
    assert!(!path.starts_with(&library));
    assert!(store
        .download("empty", generation, &cancel, |_writer| Ok(()))
        .is_err());
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        store.download("cancelled", generation, &cancelled, |_writer| Ok(())),
        Err(AssetError::Cancelled)
    ));
    store.begin_generation();
    assert!(matches!(store.ready_path("../escape"), Ok(_))); // key is safely mapped, not traversed
    assert!(matches!(
        store.download("stale", generation, &cancel, |_writer| Ok(())),
        Err(AssetError::StaleGeneration)
    ));
    store.cleanup_session(None).unwrap();
    assert!(!path.exists());
}

#[test]
fn ref_preview_session_and_discover_mode_cover_windows_handoff_and_intervention() {
    let recs: Vec<_> = (0..7).map(|n| id(&format!("rec-{n}"))).collect();
    let mut session =
        PreviewSession::start(1, vec![id("seed"), id("next")], 0, recs.clone(), &recs[2]).unwrap();
    assert_eq!(
        session.window().len(),
        (AHEAD_WINDOW + 1).min(recs.len() - 2)
    );
    assert_eq!(
        session.prepared_asset(1, recs[2].clone(), PathBuf::from("fixture://rec-2.mp3")),
        SessionAction::Play(recs[2].clone())
    );
    assert!(matches!(
        session.failed_asset(1, &recs[3]),
        SessionAction::Wait(_)
    ));
    assert_eq!(
        session.prepared_asset(0, recs[3].clone(), PathBuf::from("fixture://stale")),
        SessionAction::Ended
    );
    assert_eq!(session.invalidate(2).queue, vec![id("seed"), id("next")]);

    let mut navigation =
        PreviewSession::start(3, vec![id("seed"), id("next")], 0, recs.clone(), &recs[2]).unwrap();
    assert_eq!(
        navigation.prepared_asset(3, recs[2].clone(), PathBuf::from("fixture://rec-2")),
        SessionAction::Play(recs[2].clone())
    );
    assert_eq!(navigation.next(), SessionAction::Wait(recs[3].clone()));
    assert_eq!(
        navigation.prepared_asset(3, recs[3].clone(), PathBuf::from("fixture://rec-3")),
        SessionAction::Wait(recs[3].clone())
    );
    assert_eq!(navigation.previous(), SessionAction::Play(recs[2].clone()));
    assert_eq!(navigation.next(), SessionAction::Play(recs[3].clone()));
    assert_eq!(navigation.previous(), SessionAction::Play(recs[2].clone()));
    assert_eq!(
        navigation.previous(),
        SessionAction::ReturnToSeed {
            queue: vec![id("seed"), id("next")],
            index: 0
        }
    );

    let mut mode =
        DiscoverMode::new(1, vec![id("seed"), id("next")], 0, recs[..3].to_vec()).unwrap();
    for (n, path) in ["a.mp3", "b.mp3"].iter().enumerate() {
        assert!(matches!(
            mode.ready_asset(
                1,
                ReadyAsset {
                    id: recs[n].clone(),
                    path: PathBuf::from(*path)
                }
            ),
            DiscoverAction::Noop
        ));
    }
    assert!(matches!(mode.enable(), DiscoverAction::QueueChanged { .. }));
    assert_eq!(
        mode.advance(&recs[0]),
        DiscoverAction::Prepare {
            id: recs[2].clone()
        }
    );
    // The rolling window is projected when the session enables, so a completion
    // only fills in the asset table — it does not re-shape the queue.
    assert_eq!(
        mode.ready_asset(
            1,
            ReadyAsset {
                id: recs[2].clone(),
                path: PathBuf::from("c.mp3")
            }
        ),
        DiscoverAction::Noop
    );
    assert!(matches!(
        mode.disable(2),
        DiscoverAction::Restore { queue, index: 0 }
            if queue == vec![id("seed"), id("next")]
    ));
    let replacement = vec![id("user-owned")];
    assert_eq!(mode.handoff(&replacement), DiscoverAction::Noop);
    assert!(!mode.is_active());
}

#[test]
fn ref_preview_save_requires_destination_validates_duplicate_and_promotes_once() {
    let root = fixture("save");
    let staged = root.join("staged.mp3");
    fs::write(&staged, b"ID3 synthetic preview").unwrap();
    assert!(matches!(
        promote_to_playlist(&staged, "track.mp3", None),
        Err(SaveError::MissingDestination)
    ));
    let destination = PlaylistDestination {
        playlist_id: "playlist-a".into(),
        directory: root.join("playlist-a"),
    };
    let library_root = root.join("library");
    fs::create_dir_all(&library_root).unwrap();
    let before = scan_library(&[library_root.clone()], &OsFileSystem);
    assert!(before
        .playlists
        .values()
        .flatten()
        .all(|path| path != &staged));
    let destination = PlaylistDestination {
        directory: library_root.join("playlist-a"),
        ..destination
    };
    assert!(matches!(
        promote_to_playlist(&staged, "../escape.mp3", Some(&destination)),
        Err(SaveError::InvalidDestination)
    ));
    let first = promote_to_playlist(&staged, "track.mp3", Some(&destination)).unwrap();
    assert!(!first.already_saved && first.path.exists() && staged.exists());
    let after = scan_library(&[library_root], &OsFileSystem);
    assert!(after
        .playlists
        .get("playlist-a")
        .is_some_and(|paths| paths == &vec![first.path.clone()]));
    let second = promote_to_playlist(&staged, "track.mp3", Some(&destination)).unwrap();
    assert!(second.already_saved);
    fs::write(destination.directory.join("bad.mp3"), b"not audio").unwrap();
    assert!(matches!(
        promote_to_playlist(&staged, "bad.mp3", Some(&destination)),
        Err(SaveError::Promotion(_))
    ));
}

#[test]
fn ref_video_source_and_staging_use_canonical_relevant_isolated_assets() {
    let results = lookup(Some("artist-a"), "track-a", |query| {
        if query.contains("artist-a") {
            vec![
                VideoCandidate {
                    id: "wrong".into(),
                    title: "unrelated".into(),
                    artist: Some("other".into()),
                    url: "".into(),
                },
                VideoCandidate {
                    id: "".into(),
                    title: "artist-a - track-a".into(),
                    artist: None,
                    url: "https://youtu.be/video-1?x=1".into(),
                },
            ]
        } else {
            vec![]
        }
    });
    assert_eq!(results[0].id, "video-1");
    assert_eq!(results[0].url, "https://www.youtube.com/watch?v=video-1");
    let root = fixture("video");
    let library = root.join("library");
    fs::create_dir_all(&library).unwrap();
    let mut stage = VideoStage::at(root.join("video-cache"), Some(&library)).unwrap();
    let generation = stage.begin_generation();
    let cancel = AtomicBool::new(false);
    let current = stage
        .stage(Slot::Current, "video-1", generation, &cancel, |out| {
            out.write_all(b"....ftypmp42").map_err(|e| e.to_string())
        })
        .unwrap();
    assert_eq!(current.state, StageState::Ready);
    assert!(stage
        .adopt(Slot::Current, generation, current.path.as_ref().unwrap())
        .is_ok());
    assert!(matches!(
        stage.adopt(Slot::Current, generation, &library.join("bad.mp4")),
        Err(StageError::InvalidPath)
    ));
    let next = stage.unavailable(Slot::Next, generation);
    assert_eq!(next.state, StageState::Unavailable);
    let next_ready = stage
        .stage(Slot::Next, "video-2", generation, &cancel, |out| {
            out.write_all(b"....ftypmp42").map_err(|e| e.to_string())
        })
        .unwrap();
    assert_eq!(next_ready.slot, Slot::Next);
    assert_eq!(next_ready.state, StageState::Ready);
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        stage.stage(Slot::Next, "video-cancel", generation, &cancelled, |_out| {
            Ok(())
        }),
        Err(StageError::Cancelled)
    ));
    let stale_generation = stage.begin_generation();
    assert!(matches!(
        stage.stage(Slot::Next, "video-stale", generation, &cancel, |_out| {
            Ok(())
        }),
        Err(StageError::StaleGeneration)
    ));
    assert_eq!(stale_generation, generation + 1);
    stage.cleanup(&[]).unwrap();
    assert!(fs::read_dir(stage.root()).unwrap().next().is_none());
}

/// Regression for the live failure where the ONLY strict candidate was a
/// licensed upload YouTube refuses to every downloader client ("not available
/// on this app"), leaving staging with zero alternates. Relaxed matches must
/// follow strict ones as fallback material while word-soup results stay out.
#[test]
fn ref_video_lookup_offers_relaxed_fallbacks_after_strict_matches() {
    fn candidate(id: &str, title: &str, uploader: Option<&str>) -> VideoCandidate {
        VideoCandidate { id: id.into(), title: title.into(), artist: uploader.map(str::to_string), url: String::new() }
    }
    // Real-world shape of the "Playtime [full EP]" failure.
    let results = lookup(Some("L'indécis"), "Playtime [full EP]", |query| match query {
        q if q.contains(" - ") => vec![
            candidate("exact-licensed", "L'indécis - Playtime [full EP]", Some("Chillhop Music")),
            // Word soup sharing two title words but no artist anywhere.
            candidate("soup-1", "🔴 LIVE: Playtime with Friends (Full Episodes)", Some("Non-stop cartoons")),
            candidate("soup-2", "POPPY PLAYTIME ALL CHAPTERS FULL Game", None),
            // Partial overlap WITH artist evidence -> relaxed fallback.
            candidate("artist-mix", "PLAYTIME EP Release Party - 2H Mix", Some("L'indécis")),
        ],
        _ => vec![
            candidate("reupload", "L'Indécis - Playtime (Full Album)", Some("The Mellow Mix")),
            candidate("low-overlap", "L'indécis - Full Performance (Live Session)", Some("Chillhop Music")),
        ],
    });
    let ids: Vec<&str> = results.iter().map(|source| source.id.as_str()).collect();
    assert_eq!(
        ids,
        // Strict first; relaxed ties keep query-form encounter order.
        vec!["exact-licensed", "artist-mix", "reupload"],
        "strict first, then relaxed best-overlap; word soup and low overlap rejected"
    );
}

#[derive(Default)]
struct FakeDecoder {
    starts: Vec<Duration>,
    stops: usize,
    clears: usize,
}
impl Decoder for FakeDecoder {
    fn start(&mut self, position: Duration, _size: VideoSize) -> Result<(), String> {
        self.starts.push(position);
        Ok(())
    }
    fn stop(&mut self) {
        self.stops += 1;
    }
    fn clear_buffer(&mut self) {
        self.clears += 1;
    }
}
struct FakeFactory {
    fail: bool,
    creates: usize,
}
impl DecoderFactory for FakeFactory {
    type Decoder = FakeDecoder;
    fn create(
        &mut self,
        _generation: crate::video::player::VideoGeneration,
    ) -> Result<Self::Decoder, String> {
        self.creates += 1;
        if self.fail {
            Err("decoder unavailable".into())
        } else {
            Ok(FakeDecoder::default())
        }
    }
}

#[test]
fn ref_video_player_follows_audio_clock_and_cleans_fullscreen_or_fallback() {
    let mut player = VideoPlayer::new(
        FakeFactory {
            fail: false,
            creates: 0,
        },
        VideoSize {
            width: 640,
            height: 360,
        },
    );
    let generation = player.load(Duration::from_secs(2));
    assert!(player.start(generation, Duration::from_secs(2)));
    player.sync(Duration::from_secs(3));
    player.pause(Duration::from_secs(4));
    player.resume(Duration::from_secs(4));
    player.seek(Duration::from_secs(6));
    player.resize(
        VideoSize {
            width: 800,
            height: 450,
        },
        Duration::from_secs(6),
    );
    player.set_fullscreen(true);
    player.decoder_eof();
    assert_eq!(player.status(), VideoStatus::Ended);
    assert!(player
        .take_events()
        .iter()
        .any(|event| matches!(event, VideoEvent::Resize(_))));
    player.shutdown();
    assert!(!player.is_fullscreen());
    let mut fallback = VideoPlayer::new(
        FakeFactory {
            fail: true,
            creates: 0,
        },
        VideoSize {
            width: 1,
            height: 1,
        },
    );
    fallback.load(Duration::ZERO);
    assert_eq!(fallback.status(), VideoStatus::Fallback);
}

#[test]
fn ref_shutdown_cancels_owned_work_without_late_policy_delivery() {
    let registry = Arc::new(TaskRegistry::new(2));
    let worker = registry
        .spawn(TaskSpec::new("preview", Generation(1), None), |token| {
            while !token.is_cancelled() {
                thread::sleep(Duration::from_millis(1));
            }
            token.is_cancelled()
        })
        .unwrap();
    let report = shutdown(&registry);
    assert!(!report.timed_out);
    assert!(worker.recv().unwrap());
    assert!(registry.is_closing());
    assert_eq!(registry.active_count(), 0);
    let _ = AtomicBool::new(false).load(Ordering::Relaxed);
}
