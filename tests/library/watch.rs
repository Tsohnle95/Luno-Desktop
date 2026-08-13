use std::path::PathBuf;
use std::time::{Duration, Instant};

use luno_desktop_lib::library::reconcile::{ReconcileAction, Reconciler, ScanGeneration};
use luno_desktop_lib::library::watch::{is_partial_path, Debouncer, EventKind, OperationGates, WatchEvent};

#[test]
fn debounce_coalesces_create_modify_and_rename_requires_full_scan() {
    let start = Instant::now();
    let mut watcher = Debouncer::new(Duration::from_millis(10));
    assert!(watcher.push(WatchEvent::new(EventKind::Create, "/music/a.mp3"), start));
    assert!(watcher.push(WatchEvent::new(EventKind::Modify, "/music/a.mp3"), start));
    assert!(watcher.push(WatchEvent::rename("/music/a.mp3", "/music/b.mp3"), start));
    let batch = watcher.take(start + Duration::from_millis(10)).unwrap();
    assert_eq!(batch.paths, vec![PathBuf::from("/music/a.mp3"), PathBuf::from("/music/b.mp3")]);
    assert!(batch.full_reconcile);
}

#[test]
fn partial_files_are_ignored_and_gates_defer_until_released() {
    let start = Instant::now();
    assert!(is_partial_path(std::path::Path::new("song.mp3.part")));
    let mut watcher = Debouncer::default();
    assert!(!watcher.push(WatchEvent::new(EventKind::Create, "song.mp3.part"), start));
    watcher.push(WatchEvent::new(EventKind::Create, "song.mp3"), start);
    watcher.set_gates(OperationGates { active_write: false, download: true, transfer: false });
    assert!(watcher.take(start + Duration::from_secs(1)).is_none());
    assert!(watcher.deferred());
    watcher.set_gates(OperationGates::default());
    assert!(watcher.flush(start + Duration::from_secs(1)).is_some());
}

#[test]
fn reconciliation_uses_generations_and_full_scan_fallback() {
    let mut reconciler = Reconciler::default();
    let first = reconciler.request(None);
    assert_eq!(first.action, ReconcileAction::FullScan);
    let pending = reconciler.request(None);
    assert_eq!(pending.generation, first.generation);
    assert!(!reconciler.finish(ScanGeneration { generation: first.generation + 1, action: first.action }));
    assert!(reconciler.finish(first));
    assert!(reconciler.take_pending());
}
