use std::time::Duration;
use luno_desktop_lib::playback::clock::{heartbeat_should_continue, ClockSource, PlaybackClock, STARTUP_GRACE};
use luno_desktop_lib::playback::recovery::{FailureRecovery, RecoveryAction};

#[test]
fn backend_clock_has_precedence_and_clamps() {
    let mut clock = PlaybackClock::new(Some(Duration::from_secs(30)));
    clock.start(10.0);
    let reading = clock.position(20.0, true, Some(Duration::from_secs(12)));
    assert_eq!(reading, (luno_desktop_lib::playback::clock::ClockReading { position: Duration::from_secs(12), source: ClockSource::Backend }));
    assert_eq!(clock.position(20.0, true, Some(Duration::from_secs(40))).position, Duration::from_secs(30));
}

#[test]
fn fallback_pause_resume_and_seek_while_paused_are_stable() {
    let mut clock = PlaybackClock::new(Some(Duration::from_secs(100)));
    clock.start(0.0);
    assert_eq!(clock.position(5.0, false, None).position, Duration::from_secs(5));
    clock.pause(5.0, None);
    clock.seek(Duration::from_secs(75), 20.0);
    assert_eq!(clock.position(50.0, false, None).position, Duration::from_secs(75));
    clock.resume(50.0);
    assert_eq!(clock.position(52.0, false, None).position, Duration::from_secs(77));
}

#[test]
fn completion_handles_known_unknown_duration_and_latches_once() {
    let mut known = PlaybackClock::new(Some(Duration::from_secs(10))); known.start(0.0);
    assert!(known.completion_due(Duration::from_secs(10), true, Duration::from_secs(10)));
    assert!(known.dispatch_completion()); assert!(!known.dispatch_completion());
    let mut unknown = PlaybackClock::new(None); unknown.start(0.0);
    assert!(!unknown.completion_due(Duration::from_millis(1), false, STARTUP_GRACE));
    assert!(unknown.completion_due(Duration::from_millis(1), false, STARTUP_GRACE + Duration::from_millis(1)));
}

#[test]
fn failures_skip_first_five_and_stop_on_sixth_then_success_resets() {
    let mut recovery = FailureRecovery::new();
    for _ in 0..5 { assert_eq!(recovery.record_failure(), RecoveryAction::Skip); }
    assert_eq!(recovery.record_failure(), RecoveryAction::Stop);
    assert!(recovery.stopped());
    assert_eq!(recovery.record_failure(), RecoveryAction::Stop);
    let mut fresh = FailureRecovery::new();
    fresh.record_failure(); fresh.record_success();
    assert_eq!(fresh.consecutive_failures(), 0);
    assert_eq!(fresh.record_failure(), RecoveryAction::Skip);
}

#[test]
fn heartbeat_shutdown_guard_blocks_late_poll() {
    assert!(heartbeat_should_continue(false));
    assert!(!heartbeat_should_continue(true));
}
