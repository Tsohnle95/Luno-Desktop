//! Filesystem event policy. The OS-specific watcher belongs at the edge; this
//! module keeps debounce and safety decisions deterministic and testable.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind { Create, Modify, Remove, Rename }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEvent { pub kind: EventKind, pub path: PathBuf, pub old_path: Option<PathBuf> }

impl WatchEvent {
    pub fn new(kind: EventKind, path: impl Into<PathBuf>) -> Self {
        Self { kind, path: path.into(), old_path: None }
    }
    pub fn rename(old_path: impl Into<PathBuf>, path: impl Into<PathBuf>) -> Self {
        Self { kind: EventKind::Rename, path: path.into(), old_path: Some(old_path.into()) }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OperationGates { pub active_write: bool, pub download: bool, pub transfer: bool }

impl OperationGates { pub fn blocked(self) -> bool { self.active_write || self.download || self.transfer } }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchBatch { pub paths: Vec<PathBuf>, pub full_reconcile: bool }

#[derive(Debug)]
pub struct Debouncer {
    delay: Duration,
    first: Option<Instant>,
    events: Vec<WatchEvent>,
    gates: OperationGates,
    deferred: bool,
}

impl Default for Debouncer { fn default() -> Self { Self::new(DEFAULT_DEBOUNCE) } }

impl Debouncer {
    pub fn new(delay: Duration) -> Self { Self { delay, first: None, events: Vec::new(), gates: OperationGates::default(), deferred: false } }
    pub fn set_gates(&mut self, gates: OperationGates) { self.gates = gates; }
    pub fn gates(&self) -> OperationGates { self.gates }

    pub fn push(&mut self, event: WatchEvent, now: Instant) -> bool {
        if is_partial_path(&event.path) || event.old_path.as_deref().is_some_and(is_partial_path) { return false; }
        if self.first.is_none() { self.first = Some(now); }
        self.events.push(event);
        true
    }

    pub fn ready(&self, now: Instant) -> bool { self.first.is_some_and(|start| now.duration_since(start) >= self.delay) }

    /// Returns a single coalesced batch. Gated events are retained until the
    /// operation finishes, preventing a download's transient writes escaping.
    pub fn take(&mut self, now: Instant) -> Option<WatchBatch> {
        if !self.ready(now) { return None; }
        if self.gates.blocked() { self.deferred = true; return None; }
        if self.events.is_empty() { self.first = None; return None; }
        self.first = None;
        self.deferred = false;
        let events = std::mem::take(&mut self.events);
        Some(coalesce(events))
    }

    pub fn deferred(&self) -> bool { self.deferred }
    pub fn flush(&mut self, now: Instant) -> Option<WatchBatch> { self.take(now) }
}

fn coalesce(events: Vec<WatchEvent>) -> WatchBatch {
    let mut paths = BTreeSet::new();
    let mut full = false;
    for event in events {
        paths.insert(event.path);
        if let Some(old) = event.old_path { paths.insert(old); }
        if event.kind == EventKind::Rename || event.kind == EventKind::Remove { full = true; }
    }
    WatchBatch { paths: paths.into_iter().collect(), full_reconcile: full }
}

pub fn is_partial_path(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_ascii_lowercase();
    [".part", ".tmp", ".crdownload", ".download", ".luno-tmp"]
        .iter().any(|suffix| name.ends_with(suffix) || name.contains(suffix))
        || name.starts_with('.')
}
