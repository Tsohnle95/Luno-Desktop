use crate::contracts::{
    events::{Command, CommandEnvelope, Event},
    types::CONTRACT_VERSION,
    TrackId,
};
use crate::playback::backend::{AudioBackend, BackendError, FakeBackend};
use crate::playback::clock::{heartbeat_should_continue, ClockSource, PlaybackClock};
use crate::playback::queue::{QueueState, RepeatMode};
use crate::playback::recovery::{FailureRecovery, RecoveryAction, MAX_CONSECUTIVE_FAILURES};
use crate::preview::discover_mode::{DiscoverAction, DiscoverMode, ReadyAsset};
use crate::preview::session::{PreviewSession, SessionAction, AHEAD_WINDOW};
use crate::state::AppState;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

fn id(value: &str) -> TrackId {
    TrackId(value.into())
}

fn dispatch(state: &AppState, command: Command) {
    state
        .dispatch(CommandEnvelope {
            version: CONTRACT_VERSION,
            request_id: format!("gate3-{}", rand_id()),
            command,
        })
        .unwrap();
}

fn rand_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn fixture_file(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("luno-gate3-{label}-{}", std::process::id()));
    let _ = fs::create_dir_all(&root);
    let path = root.join("track.wav");
    let sample_rate = 44_100u32;
    let channels = 2u16;
    let bits_per_sample = 16u16;
    let frames = 441u32;
    let data_size = frames * channels as u32 * (bits_per_sample as u32 / 8);
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
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for frame in 0..frames {
        let sample = (((frame * 440) % sample_rate) as i16).to_le_bytes();
        wav.extend_from_slice(&sample);
        wav.extend_from_slice(&sample);
    }
    fs::write(&path, wav).unwrap();
    path
}

#[test]
fn synthetic_fixture_is_pcm_riff_wave_without_a_device() {
    let path = fixture_file("shape");
    let bytes = fs::read(path).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(&bytes[12..16], b"fmt ");
    assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 1);
    assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
    assert_eq!(
        u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        44_100
    );
    assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 16);
    assert_eq!(&bytes[36..40], b"data");
}

#[test]
fn ref_play_context_and_queue_up_next_preserve_order() {
    let mut queue = QueueState::new();
    queue.play_with_context(
        id("track-b"),
        vec![id("track-a"), id("track-b"), id("track-c")],
    );
    queue.play_next([id("track-x")]);
    queue.add_to_queue([id("track-y")]);
    assert_eq!(
        queue.queue(),
        &[
            id("track-a"),
            id("track-b"),
            id("track-x"),
            id("track-y"),
            id("track-c")
        ]
    );
    assert_eq!(queue.current(), Some(&id("track-b")));
    assert_eq!(queue.user_queue_count(), 2);
}

#[test]
fn target_app_state_publishes_authoritative_queue_snapshots_for_mutations() {
    let state = AppState::new(4);
    dispatch(
        &state,
        Command::PlayNext {
            track_ids: vec![id("track-a")],
        },
    );
    dispatch(
        &state,
        Command::AddToQueue {
            track_ids: vec![id("track-b")],
        },
    );
    dispatch(&state, Command::ReorderQueue { from: 1, to: 0 });
    let snapshots: Vec<_> = state
        .events()
        .into_iter()
        .filter_map(|event| match event.event {
            Event::PlaybackSnapshot { snapshot } => Some(snapshot),
            _ => None,
        })
        .collect();
    assert_eq!(
        snapshots.last().unwrap().queue,
        vec![id("track-b"), id("track-a")]
    );
    assert_eq!(snapshots.last().unwrap().current_index, Some(1));
    dispatch(&state, Command::ToggleShuffle);
    dispatch(
        &state,
        Command::SetRepeat {
            repeat: crate::contracts::events::RepeatMode::All,
        },
    );
    let latest = state
        .events()
        .into_iter()
        .rev()
        .find_map(|event| match event.event {
            Event::PlaybackSnapshot { snapshot } => Some(snapshot),
            _ => None,
        })
        .unwrap();
    assert!(latest.shuffle);
    assert_eq!(latest.repeat, crate::contracts::events::RepeatMode::All);
}

#[test]
fn ref_shuffle_repeat_keeps_current_and_restores_context() {
    let mut queue = QueueState::new();
    queue.play_with_context(id("b"), ["a", "b", "c", "d"].into_iter().map(id).collect());
    queue.toggle_shuffle();
    assert_eq!(queue.current(), Some(&id("b")));
    assert_eq!(queue.queue()[0], id("b"));
    queue.toggle_shuffle();
    assert_eq!(queue.queue(), &[id("a"), id("b"), id("c"), id("d")]);
    queue.set_repeat(RepeatMode::One);
    assert_eq!(queue.next(), Some(&id("b")));
    queue.set_repeat(RepeatMode::All);
    assert_eq!(queue.next(), Some(&id("c")));
    assert_eq!(queue.next(), Some(&id("d")));
    assert_eq!(queue.next(), Some(&id("a")));
    assert_eq!(queue.current(), Some(&id("a")));
}

#[test]
fn ref_clock_seek_pause_and_completion_are_clamped_and_latched() {
    let mut clock = PlaybackClock::new(Some(Duration::from_secs(120)));
    clock.start(10.0);
    assert_eq!(
        clock.position(12.0, false, None).position,
        Duration::from_secs(2)
    );
    clock.seek(Duration::from_secs(200), 12.0);
    assert_eq!(
        clock.position(12.0, false, None).position,
        Duration::from_secs(120)
    );
    clock.pause(12.0, Some(Duration::from_secs(60)));
    assert_eq!(
        clock
            .position(99.0, true, Some(Duration::from_secs(90)))
            .position,
        Duration::from_secs(60)
    );
    clock.resume(20.0);
    assert_eq!(
        clock.position(21.0, false, None).position,
        Duration::from_secs(61)
    );
    assert!(clock.completion_due(Duration::from_secs(120), true, Duration::ZERO));
    assert!(clock.dispatch_completion());
    assert!(!clock.dispatch_completion());
    assert_eq!(
        clock
            .position(21.0, true, Some(Duration::from_secs(61)))
            .source,
        ClockSource::Backend
    );
}

#[test]
fn ref_failure_skip_stops_after_observed_threshold() {
    let mut recovery = FailureRecovery::new();
    for _ in 0..MAX_CONSECUTIVE_FAILURES {
        assert_eq!(recovery.record_failure(), RecoveryAction::Skip);
    }
    assert_eq!(recovery.record_failure(), RecoveryAction::Stop);
    assert!(recovery.stopped());
    recovery.record_success();
    assert_eq!(recovery.record_failure(), RecoveryAction::Stop);
}

#[test]
fn ref_queue_mutations_keep_current_index_and_history_policy() {
    let mut queue = QueueState::new();
    queue.play_with_context(id("b"), ["a", "b", "c"].into_iter().map(id).collect());
    queue.record_started(id("b"), false);
    queue.record_started(id("preview"), true);
    assert_eq!(queue.history(), &[id("b")]);
    assert!(queue.remove(0));
    assert_eq!(queue.current_index(), Some(0));
    assert!(queue.reorder(0, 1));
    assert_eq!(queue.current_index(), Some(1));
}

#[test]
fn ref_backend_missing_unsupported_and_device_independent_fixture_states() {
    let path = fixture_file("backend");
    let mut backend = FakeBackend::with_track(path.clone(), Some(Duration::from_secs(2)));
    let info = backend.load(&path).unwrap();
    assert_eq!(info.duration, Some(Duration::from_secs(2)));
    backend.play().unwrap();
    backend.seek(Duration::from_secs(9)).unwrap();
    assert_eq!(backend.position().unwrap(), Duration::from_secs(2));
    assert!(matches!(
        backend.load(&path.with_extension("txt")),
        Err(BackendError::UnsupportedFormat(_))
    ));
    assert!(matches!(
        backend.load(&path.with_file_name("missing.wav")),
        Err(BackendError::Io(_))
    ));
}

#[test]
fn ref_preview_session_isolated_and_stale_completion_cannot_resurrect() {
    let physical = vec![id("seed"), id("successor")];
    let recs: Vec<_> = (0..7).map(|i| id(&format!("rec-{i}"))).collect();
    let tapped = recs[2].clone();
    let mut session = PreviewSession::start(4, physical.clone(), 0, recs, &tapped).unwrap();
    assert_eq!(session.window().len(), AHEAD_WINDOW);
    assert_eq!(
        session.prepared_asset(
            4,
            tapped.clone(),
            PathBuf::from("fixture://preview/rec-2.mp3")
        ),
        SessionAction::Play(tapped)
    );
    assert!(matches!(
        session.failed_asset(4, &id("rec-3")),
        SessionAction::Wait(_)
    ));
    assert_eq!(session.invalidate(5).queue, physical);
    assert_eq!(
        session.prepared_asset(4, id("rec-4"), PathBuf::from("fixture://stale")),
        SessionAction::Ended
    );
}

#[test]
fn ref_discover_handoff_restores_physical_queue_and_keeps_assets_temporary() {
    let physical = vec![id("seed"), id("next")];
    let mut mode = DiscoverMode::new(1, physical.clone(), 0, vec![id("rec")]).unwrap();
    assert_eq!(
        mode.ready_asset(
            1,
            ReadyAsset {
                id: id("rec"),
                path: PathBuf::from("fixture://preview/rec.mp3")
            }
        ),
        DiscoverAction::Noop
    );
    assert!(matches!(mode.enable(), DiscoverAction::QueueChanged { .. }));
    let projected = mode.projected_queue();
    assert_ne!(projected, physical);
    assert!(
        matches!(mode.handoff(&physical, None, &id("seed")), DiscoverAction::Restore { queue, .. } if queue == physical)
    );
    assert!(heartbeat_should_continue(false));
    assert!(!heartbeat_should_continue(true));
}
