use std::path::PathBuf;
use luno_desktop_lib::contracts::TrackId;
use luno_desktop_lib::preview::session::{PreviewSession, SessionAction, AHEAD_WINDOW};

fn id(value: &str) -> TrackId { TrackId::new(value).unwrap() }
fn recs(n: usize) -> Vec<TrackId> { (0..n).map(|i| id(&format!("r{i}"))).collect() }

#[test]
fn tap_starts_on_demand_and_keeps_five_ahead() {
    let r = recs(10);
    let mut s = PreviewSession::start(1, vec![id("seed"), id("next")], 0, r.clone(), &r[2]).unwrap();
    assert_eq!(s.window().len(), AHEAD_WINDOW + 1);
    assert_eq!(s.prepared_asset(1, r[2].clone(), PathBuf::from("r2.mp3")), SessionAction::Play(r[2].clone()));
    assert_eq!(s.refill_target(), 8);
}

#[test]
fn first_failure_restores_physical_playback() {
    let r = recs(3);
    let mut s = PreviewSession::start(4, vec![id("seed"), id("next")], 0, r.clone(), &r[0]).unwrap();
    assert_eq!(s.failed_asset(4, &r[0]), SessionAction::Ended);
    assert!(!s.is_active());
    assert_eq!(s.handoff().queue, vec![id("seed"), id("next")]);
}

#[test]
fn stale_generation_cannot_publish_after_invalidation() {
    let r = recs(1);
    let mut s = PreviewSession::start(1, vec![id("seed")], 0, r.clone(), &r[0]).unwrap();
    s.invalidate(2);
    assert_eq!(s.prepared_asset(1, r[0].clone(), PathBuf::from("late.mp3")), SessionAction::Ended);
}

#[test]
fn next_leaves_and_previous_returns_to_captured_queue() {
    let r = recs(1);
    let mut s = PreviewSession::start(1, vec![id("seed"), id("next")], 0, r.clone(), &r[0]).unwrap();
    s.prepared_asset(1, r[0].clone(), PathBuf::from("r0.mp3"));
    assert!(matches!(s.next(), SessionAction::LeaveToNext { index: Some(1), .. }));
    let mut s = PreviewSession::start(1, vec![id("seed"), id("next")], 0, r.clone(), &r[0]).unwrap();
    s.prepared_asset(1, r[0].clone(), PathBuf::from("r0.mp3"));
    assert!(matches!(s.previous(), SessionAction::ReturnToSeed { index: 0, .. }));
}

#[test]
fn shutdown_invalidates_and_preserves_queue() {
    let r = recs(1);
    let mut s = PreviewSession::start(9, vec![id("seed")], 0, r, &id("r0")).unwrap();
    let handoff = s.shutdown();
    assert!(!s.is_active());
    assert_eq!(handoff.queue, vec![id("seed")]);
}
