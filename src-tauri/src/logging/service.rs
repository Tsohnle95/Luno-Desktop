use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use crate::contracts::types::ErrorCode;
use super::redaction::redact;

pub const MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel { Debug, Info, Warn, Error }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind { InvalidRequest, NotFound, Busy, Cancelled, Unavailable, Internal, ShuttingDown, Worker }

impl From<ErrorCode> for ErrorKind {
    fn from(code: ErrorCode) -> Self { match code { ErrorCode::InvalidRequest => Self::InvalidRequest, ErrorCode::UnsupportedVersion => Self::InvalidRequest, ErrorCode::NotFound => Self::NotFound, ErrorCode::Busy => Self::Busy, ErrorCode::Cancelled => Self::Cancelled, ErrorCode::Unavailable => Self::Unavailable, ErrorCode::Internal => Self::Internal, ErrorCode::ShuttingDown => Self::ShuttingDown } }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp_ms: u128,
    pub level: LogLevel,
    pub area: String,
    pub message: String,
    pub error_kind: Option<ErrorKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogEvent { Added(LogEntry), Shutdown }

#[derive(Clone)]
pub struct LogService { inner: Arc<Mutex<Inner>> }
struct Inner { entries: VecDeque<LogEntry>, subscribers: Vec<mpsc::Sender<LogEvent>>, closed: bool }

impl Default for LogService { fn default() -> Self { Self::new() } }
impl LogService {
    pub fn new() -> Self { Self { inner: Arc::new(Mutex::new(Inner { entries: VecDeque::with_capacity(MAX_ENTRIES), subscribers: Vec::new(), closed: false })) } }
    pub fn subscribe(&self) -> mpsc::Receiver<LogEvent> { let (tx, rx) = mpsc::channel(); let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner()); if inner.closed { let _ = tx.send(LogEvent::Shutdown); } else { inner.subscribers.push(tx); } rx }
    pub fn record(&self, level: LogLevel, area: impl Into<String>, message: impl AsRef<str>) -> Result<LogEntry, ErrorKind> { self.record_kind(level, area, message, None) }
    pub fn worker_error(&self, area: impl Into<String>, message: impl AsRef<str>) -> Result<LogEntry, ErrorKind> { self.record_kind(LogLevel::Error, area, message, Some(ErrorKind::Worker)) }
    pub fn error(&self, area: impl Into<String>, message: impl AsRef<str>, kind: ErrorKind) -> Result<LogEntry, ErrorKind> { self.record_kind(LogLevel::Error, area, message, Some(kind)) }
    pub fn entries(&self) -> Vec<LogEntry> { self.inner.lock().unwrap_or_else(|e| e.into_inner()).entries.iter().cloned().collect() }
    pub fn shutdown(&self) { let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner()); if inner.closed { return; } inner.closed = true; for subscriber in inner.subscribers.drain(..) { let _ = subscriber.send(LogEvent::Shutdown); } }
    pub fn is_shutdown(&self) -> bool { self.inner.lock().unwrap_or_else(|e| e.into_inner()).closed }
    fn record_kind(&self, level: LogLevel, area: impl Into<String>, message: impl AsRef<str>, error_kind: Option<ErrorKind>) -> Result<LogEntry, ErrorKind> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner()); if inner.closed { return Err(ErrorKind::ShuttingDown); }
        let entry = LogEntry { timestamp_ms: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(), level, area: redact(&area.into()), message: redact(message.as_ref()), error_kind };
        if inner.entries.len() == MAX_ENTRIES { inner.entries.pop_front(); } inner.entries.push_back(entry.clone());
        inner.subscribers.retain(|s| s.send(LogEvent::Added(entry.clone())).is_ok());
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn bounds_entries() { let log = LogService::new(); for i in 0..501 { log.record(LogLevel::Info, "test", i.to_string()).unwrap(); } let entries = log.entries(); assert_eq!(entries.len(), 500); assert_eq!(entries[0].message, "1"); }
    #[test] fn delivers_worker_failure_and_shutdown() { let log = LogService::new(); let events = log.subscribe(); let entry = log.worker_error("download", "worker failed api_key=secret").unwrap(); assert_eq!(entry.error_kind, Some(ErrorKind::Worker)); assert!(matches!(events.recv().unwrap(), LogEvent::Added(_))); log.shutdown(); assert_eq!(events.recv().unwrap(), LogEvent::Shutdown); assert_eq!(log.record(LogLevel::Info, "x", "late"), Err(ErrorKind::ShuttingDown)); }
}
