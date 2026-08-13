use luno_desktop_lib::download::coordinator::*;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

fn root() -> PathBuf { let p = std::env::temp_dir().join(format!("luno-coordinator-{}", std::process::id())); let _ = fs::remove_dir_all(&p); fs::create_dir_all(&p).unwrap(); p }
fn request(root: &PathBuf, id: &str) -> DownloadRequest { DownloadRequest { job_id: id.into(), destination: root.join(format!("{id}.mp3")) } }

#[test]
fn admits_one_and_rejects_busy_deterministically() {
    let root = root(); let c = DownloadCoordinator::new();
    c.start(request(&root, "one"), Box::new(|_, token, _| { while !token.is_cancelled() { std::thread::sleep(Duration::from_millis(2)); } Ok(()) })).unwrap();
    assert_eq!(c.start(request(&root, "two"), Box::new(|_, _, _| Ok(()))), Err(StartError::Busy));
    c.shutdown(Duration::from_secs(1)); let _ = fs::remove_dir_all(root);
}

#[test]
fn progress_and_success_require_validated_promotion() {
    let root = root(); let c = DownloadCoordinator::new(); let events = c.events().unwrap();
    c.start(request(&root, "ok"), Box::new(|path, _, progress| { progress(0.5); fs::write(path, b"ID3audio").unwrap(); Ok(()) })).unwrap();
    c.shutdown(Duration::from_secs(1)); let got: Vec<_> = events.try_iter().collect();
    assert!(got.iter().any(|e| matches!(e, DownloadEvent::Progress { fraction, .. } if *fraction == 0.5)));
    assert!(got.iter().any(|e| matches!(e, DownloadEvent::Succeeded { .. })));
    assert!(root.join("ok.mp3").exists()); let _ = fs::remove_dir_all(root);
}

#[test]
fn cancellation_emits_terminal_event_and_shutdown_joins_worker() {
    let root = root(); let c = DownloadCoordinator::new(); let events = c.events().unwrap();
    c.start(request(&root, "cancel"), Box::new(|_, token, _| { while !token.is_cancelled() { std::thread::sleep(Duration::from_millis(2)); } Ok(()) })).unwrap();
    assert!(c.cancel()); let report = c.shutdown(Duration::from_secs(1)); assert!(!report.timed_out);
    assert!(events.try_iter().any(|e| matches!(e, DownloadEvent::Cancelled { .. })));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn missing_output_is_failure_not_success() {
    let root = root(); let c = DownloadCoordinator::new(); let events = c.events().unwrap();
    c.start(request(&root, "missing"), Box::new(|_, _, _| Ok(()))).unwrap(); c.shutdown(Duration::from_secs(1));
    assert!(events.try_iter().any(|e| matches!(e, DownloadEvent::Failed { .. })));
    assert!(!root.join("missing.mp3").exists()); let _ = fs::remove_dir_all(root);
}
