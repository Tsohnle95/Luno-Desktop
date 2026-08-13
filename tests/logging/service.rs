use luno_desktop_lib::logging::service::{ErrorKind, LogEvent, LogLevel, LogService, MAX_ENTRIES};

#[test]
fn retains_only_latest_entries() {
    let service = LogService::new();
    for index in 0..=MAX_ENTRIES { service.record(LogLevel::Info, "test", index.to_string()).unwrap(); }
    let entries = service.entries();
    assert_eq!(entries.len(), MAX_ENTRIES);
    assert_eq!(entries.first().unwrap().message, "1");
}

#[test]
fn worker_errors_are_delivered_and_shutdown_is_terminal() {
    let service = LogService::new();
    let events = service.subscribe();
    let entry = service.worker_error("worker", "failed api_key=private").unwrap();
    assert_eq!(entry.error_kind, Some(ErrorKind::Worker));
    assert!(matches!(events.recv().unwrap(), LogEvent::Added(_)));
    service.shutdown();
    assert_eq!(events.recv().unwrap(), LogEvent::Shutdown);
    assert_eq!(service.record(LogLevel::Info, "late", "ignored"), Err(ErrorKind::ShuttingDown));
}
