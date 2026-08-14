use luno_desktop_lib::contracts::CommandEnvelope;
use luno_desktop_lib::contracts::events::{Command, Event, ShutdownState};
use luno_desktop_lib::contracts::types::{JobId, TrackId};
use luno_desktop_lib::state::AppState;
use std::fs;
use std::thread;
use std::time::Duration;

fn envelope(command: Command) -> CommandEnvelope {
    CommandEnvelope { version: 1, request_id: "test-request".into(), command }
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
    thread::sleep(Duration::from_millis(20));
    let events = state.events();
    assert!(events.iter().any(|e| matches!(&e.event, Event::LibrarySnapshot { .. }) && e.generation.0 == 1));
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
