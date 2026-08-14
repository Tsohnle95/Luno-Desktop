use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use luno_desktop_lib::contracts::types::{Generation, SessionId};
use luno_desktop_lib::runtime::task_registry::{SpawnError, TaskRegistry, TaskSpec};
use luno_desktop_lib::state::AppState;

#[test]
fn cancellation_is_cooperative_and_task_can_observe_it() {
    let registry = TaskRegistry::new(1);
    let handle = registry.spawn(TaskSpec::new("scan", Generation(1), None), |token| {
        while !token.is_cancelled() { thread::yield_now(); }
        "cancelled"
    }).unwrap();
    assert!(registry.cancel_key("scan"));
    assert_eq!(handle.recv().unwrap(), "cancelled");
}

#[test]
fn superseded_generation_and_session_cannot_publish() {
    let state = AppState::new(2);
    let old = state.session_task("recommendations", Generation(1), SessionId::new("s1").unwrap());
    let new = state.session_task("recommendations", Generation(2), SessionId::new("s2").unwrap());
    let release = state.submit(old.clone(), |_| 1).unwrap();
    let current = state.submit(new.clone(), |_| 2).unwrap();
    assert!(!state.publish_allowed(&old));
    assert!(state.publish_allowed(&new));
    assert_eq!(release.recv().unwrap(), 1);
    assert_eq!(current.recv().unwrap(), 2);
}

#[test]
fn admission_is_bounded() {
    let registry = TaskRegistry::new(1);
    let (release_tx, release_rx) = mpsc::channel();
    let first = registry.spawn(TaskSpec::new("first", Generation(0), None), move |_| {
        release_rx.recv().unwrap();
    }).unwrap();
    assert!(matches!(registry.spawn(TaskSpec::new("second", Generation(0), None), |_| ()), Err(SpawnError::AtCapacity)));
    release_tx.send(()).unwrap();
    first.recv().unwrap();
}

#[test]
fn close_during_job_rejects_new_work_and_deadline_is_reported() {
    let registry = TaskRegistry::new(1);
    let (release_tx, release_rx) = mpsc::channel();
    let job = registry.spawn(TaskSpec::new("download", Generation(0), None), move |token| {
        release_rx.recv().unwrap();
        token.is_cancelled()
    }).unwrap();
    let report = registry.shutdown(Duration::from_millis(5));
    assert!(report.timed_out);
    assert!(matches!(registry.spawn(TaskSpec::new("late", Generation(0), None), |_| ()), Err(SpawnError::ShuttingDown)));
    release_tx.send(()).unwrap();
    assert!(job.recv().unwrap());
    assert!(!registry.shutdown(Duration::from_millis(50)).timed_out);
}

#[test]
fn shutdown_is_idempotent_and_late_publication_is_suppressed() {
    let state = AppState::new(1);
    let spec = state.generation("artwork", Generation(4));
    let job = state.submit(spec.clone(), |_| 7).unwrap();
    assert_eq!(job.recv().unwrap(), 7);
    thread::sleep(Duration::from_millis(2));
    assert!(!state.shutdown(Duration::from_millis(50)).timed_out);
    assert!(!state.shutdown(Duration::from_millis(50)).timed_out);
    assert!(!state.publish_allowed(&spec));
}

#[test]
fn shutdown_deadline_is_bounded() {
    let registry = TaskRegistry::new(1);
    let (_tx, rx) = mpsc::channel::<()>();
    let _job = registry.spawn(TaskSpec::new("blocked", Generation(0), None), move |_| { let _ = rx.recv(); }).unwrap();
    let start = Instant::now();
    let report = registry.shutdown(Duration::from_millis(10));
    assert!(report.timed_out);
    assert!(start.elapsed() < Duration::from_millis(100));
}

#[test]
fn shutdown_joined_counts_finished_tasks_not_poll_iterations() {
    let registry = TaskRegistry::new(1);
    let task = registry.spawn(TaskSpec::new("quick", Generation(0), None), |_| ()).unwrap();
    task.recv().unwrap();
    let report = registry.shutdown(Duration::from_secs(1));
    assert_eq!(report.joined, 1);
    assert!(!report.timed_out);
}
