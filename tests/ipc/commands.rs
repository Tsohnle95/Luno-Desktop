use luno_desktop_lib::contracts::CommandEnvelope;
use luno_desktop_lib::contracts::events::{Command, Event, ShutdownState};
use luno_desktop_lib::contracts::types::{JobId, TrackId};
use luno_desktop_lib::state::AppState;
use luno_desktop_lib::runtime::task_registry::TaskSpec;
use luno_desktop_lib::runtime::task_registry::SpawnError;
use luno_desktop_lib::contracts::events::ShutdownReason;
use std::fs;
use std::thread;
use std::time::Duration;
use std::time::Instant;

fn envelope(command: Command) -> CommandEnvelope {
    CommandEnvelope { version: 1, request_id: "test-request".into(), command }
}

fn wait_for<F: Fn(&[luno_desktop_lib::contracts::events::EventEnvelope]) -> bool>(state: &AppState, predicate: F) -> Vec<luno_desktop_lib::contracts::events::EventEnvelope> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut collected = Vec::new();
    loop {
        collected.extend(state.events());
        if predicate(&collected) { return collected; }
        assert!(Instant::now() < deadline, "timed out waiting for backend event");
        thread::yield_now();
    }
}

#[test]
fn malformed_envelopes_reject_version_and_unknown_fields() {
    let bad_version = serde_json::from_str::<CommandEnvelope>(r#"{"version":2,"request_id":"x","command":{"type":"pause"}}"#);
    let extra = serde_json::from_str::<CommandEnvelope>(r#"{"version":1,"request_id":"x","extra":true,"command":{"type":"pause"}}"#);
    assert!(bad_version.is_err());
    assert!(extra.is_err());
}

#[test]
fn scan_is_async_and_publishes_a_generation_tagged_snapshot() {
    let state = AppState::new(2);
    let root = std::env::temp_dir().join(format!("luno-ipc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Song.mp3"), b"not decoded by the scanner").unwrap();
    state.set_scan_roots(vec![root.clone()]);
    state.dispatch(envelope(Command::ScanLibrary)).unwrap();
    let events = wait_for(&state, |events| events.iter().any(|e| matches!(&e.event, Event::LibrarySnapshot { .. }) && e.generation.0 == 1));
    assert!(events.iter().any(|e| matches!(&e.event, Event::LibrarySnapshot { .. }) && e.generation.0 == 1));
    assert!(state.library().tracks.iter().all(|track| track.path.is_none()));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cancellation_and_playback_errors_are_structured() {
    let state = AppState::new(1);
    let error = state.dispatch(envelope(Command::Play { track_id: TrackId("missing".into()) })).unwrap_err();
    assert_eq!(error.code, luno_desktop_lib::contracts::types::ErrorCode::Internal);
    let error = state.dispatch(envelope(Command::CancelJob { job_id: JobId("missing".into()) })).unwrap_err();
    assert_eq!(error.code, luno_desktop_lib::contracts::types::ErrorCode::NotFound);
}

#[test]
fn shutdown_emits_ordered_states_and_rejects_late_commands() {
    let state = AppState::new(1);
    state.dispatch(envelope(Command::Shutdown { reason: luno_desktop_lib::contracts::events::ShutdownReason::UserRequested })).unwrap();
    let states: Vec<_> = state.events().into_iter().filter_map(|event| match event.event { Event::Shutdown { envelope } => Some(envelope.state), _ => None }).collect();
    assert_eq!(states, vec![ShutdownState::Requested, ShutdownState::Draining, ShutdownState::Complete]);
    let error = state.dispatch(envelope(Command::Pause)).unwrap_err();
    assert_eq!(error.code, luno_desktop_lib::contracts::types::ErrorCode::ShuttingDown);
}

#[test]
fn event_ids_are_unique() {
    let state = AppState::new(1);
    state.dispatch(envelope(Command::GetLibrary)).unwrap();
    state.dispatch(envelope(Command::GetLibrary)).unwrap();
    let events = state.events();
    assert_ne!(events[0].event_id, events[1].event_id);
}

#[test]
fn shutdown_timeout_never_emits_complete() {
    let state = AppState::new(1);
    let release = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_release = release.clone();
    state.submit(TaskSpec::new("slow", luno_desktop_lib::contracts::types::Generation(1), None), move |_| {
        while !worker_release.load(std::sync::atomic::Ordering::Acquire) { thread::yield_now(); }
    }).unwrap();
    let report = state.shutdown_events(ShutdownReason::System, Duration::from_millis(1));
    assert!(report.timed_out);
    let states: Vec<_> = state.events().into_iter().filter_map(|event| match event.event { Event::Shutdown { envelope } => Some(envelope.state), _ => None }).collect();
    assert_eq!(states, vec![ShutdownState::Requested, ShutdownState::Draining, ShutdownState::TimedOut]);
    release.store(true, std::sync::atomic::Ordering::Release);
    state.tasks.shutdown(Duration::from_secs(1));
}

#[test]
fn scan_errors_are_structured_and_paths_are_private() {
    let state = AppState::new(1);
    let missing = std::env::temp_dir().join(format!("luno-missing-{}", std::process::id()));
    state.set_scan_roots(vec![missing.clone()]);
    state.dispatch(envelope(Command::ScanLibrary)).unwrap();
    let events = wait_for(&state, |events| events.iter().any(|e| matches!(e.event, Event::Error { .. })));
    assert!(events.iter().any(|e| matches!(e.event, Event::Error { .. })));
    let snapshot = state.library();
    assert!(snapshot.tracks.iter().all(|track| track.path.is_none()));
}

#[test]
fn identity_and_identifier_validation_do_not_use_empty_or_absolute_ids() {
    let metadata = luno_desktop_lib::domain::identity::filename_metadata("/private/music/Artist - Song.mp3");
    let id = luno_desktop_lib::domain::identity::track_id(&metadata);
    assert!(!id.contains("/"));
    assert!(TrackId::new("").is_err());
}

#[test]
fn event_sink_failures_are_buffered_as_structured_errors() {
    let state = AppState::new(1);
    state.set_event_sink(std::sync::Arc::new(|_| Err("test sink failure".into())));
    state.publish(state.event(luno_desktop_lib::contracts::types::Generation(0), Event::LibraryRootStatus { root_index: 0, status: luno_desktop_lib::contracts::events::RootStatus::Scanned }));
    let events = state.events();
    assert!(events.iter().any(|event| matches!(event.event, Event::LibraryRootStatus { .. })));
    assert!(events.iter().any(|event| matches!(event.event, Event::Error { .. })));
}

#[test]
fn scanned_provider_metadata_is_preserved_in_snapshot() {
    let state = AppState::new(1);
    let root = std::env::temp_dir().join(format!("luno-provider-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let path = root.join("Song.mp3");
    fs::write(&path, b"audio").unwrap();
    state.set_provider_metadata(path, luno_desktop_lib::domain::identity::TrackMetadata { title: "Song".into(), artist: "Artist".into(), duration_ms: 1234, provider: Some(luno_desktop_lib::domain::identity::ProviderIdentity { provider: "youtube".into(), id: "abc".into() }) });
    state.set_scan_roots(vec![root.clone()]);
    state.dispatch(envelope(Command::ScanLibrary)).unwrap();
    wait_for(&state, |events| events.iter().any(|e| matches!(e.event, Event::LibrarySnapshot { .. })));
    let track = &state.library().tracks[0];
    assert_eq!(track.provider.as_ref().map(|provider| provider.id.as_str()), Some("abc"));
    assert_eq!(track.duration_ms, Some(1234));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn capacity_rejection_does_not_cancel_superseded_task() {
    let state = AppState::new(1);
    let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let marker = completed.clone();
    let started = std::sync::Arc::new(std::sync::Barrier::new(2));
    let worker_started = started.clone();
    let release = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_release = release.clone();
    let old = state.submit(TaskSpec::new("same", luno_desktop_lib::contracts::types::Generation(1), None), move |token| {
        worker_started.wait();
        while !worker_release.load(std::sync::atomic::Ordering::Acquire) { thread::yield_now(); }
        if !token.is_cancelled() { marker.store(true, std::sync::atomic::Ordering::Release); }
    }).unwrap();
    started.wait();
    let replacement = state.submit(TaskSpec::new("same", luno_desktop_lib::contracts::types::Generation(2), None), |_| ());
    assert!(matches!(replacement, Err(SpawnError::AtCapacity)));
    release.store(true, std::sync::atomic::Ordering::Release);
    assert!(old.recv().is_ok());
    assert!(completed.load(std::sync::atomic::Ordering::Acquire));
}
