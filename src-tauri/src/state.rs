use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use serde::Serialize;
use std::sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Mutex};
use std::time::Duration;
use crate::contracts::{events::{Command, CommandEnvelope, Event, EventEnvelope, RootStatus, ShutdownEnvelope, ShutdownState}, types::{ContractError, ErrorCode, Generation, JobId, LibrarySnapshot, Playlist, Track, TrackId, SessionId, CONTRACT_VERSION}};
use serde_json::Value;
use std::sync::Arc as StdArc;
use crate::library::scanner::{scan_library, OsFileSystem};
use crate::runtime::task_registry::{CancellationToken, ShutdownReport, SpawnError, TaskHandle, TaskRegistry, TaskSpec};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus { NotRun, Skipped { reason: String }, Completed { warnings: usize, snapshot_created: bool, removed_previews: usize }, Failed { reason: String } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_status_is_sanitized_diagnostic_data() {
        let status = MigrationStatus::Completed { warnings: 1, snapshot_created: true, removed_previews: 2 };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("snapshot_created"));
        assert!(!json.contains("/"));
    }
}

#[derive(Clone)]
pub struct AppState {
    pub tasks: Arc<TaskRegistry>,
    pub migration_status: Arc<Mutex<MigrationStatus>>,
    library: Arc<Mutex<LibrarySnapshot>>,
    roots: Arc<Mutex<Vec<PathBuf>>>,
    root_statuses: Arc<Mutex<Vec<RootStatus>>>,
    events: Arc<Mutex<Vec<EventEnvelope>>>,
    next_event: Arc<AtomicU64>,
    next_generation: Arc<AtomicU64>,
    event_sink: Arc<Mutex<Option<StdArc<dyn Fn(EventEnvelope) -> Result<(), String> + Send + Sync>>>>,
    shutdown_started: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(task_limit: usize) -> Self {
        Self { tasks: Arc::new(TaskRegistry::new(task_limit)), migration_status: Arc::new(Mutex::new(MigrationStatus::NotRun)),
            library: Arc::new(Mutex::new(LibrarySnapshot { version: CONTRACT_VERSION, generation: Generation(0), tracks: Vec::new(), playlists: Vec::new() })),
            roots: Arc::new(Mutex::new(Vec::new())), root_statuses: Arc::new(Mutex::new(Vec::new())), events: Arc::new(Mutex::new(Vec::new())),
            next_event: Arc::new(AtomicU64::new(0)), next_generation: Arc::new(AtomicU64::new(0)), event_sink: Arc::new(Mutex::new(None)), shutdown_started: Arc::new(AtomicBool::new(false)) }
    }
    pub fn library(&self) -> LibrarySnapshot { self.library.lock().unwrap().clone() }
    pub fn root_statuses(&self) -> Vec<RootStatus> { self.root_statuses.lock().unwrap().clone() }
    pub fn set_scan_roots(&self, roots: Vec<PathBuf>) { *self.roots.lock().unwrap() = roots; }
    pub fn events(&self) -> Vec<EventEnvelope> { std::mem::take(&mut *self.events.lock().unwrap()) }
    pub fn event(&self, generation: Generation, event: Event) -> EventEnvelope {
        let id = self.next_event.fetch_add(1, Ordering::Relaxed) + 1;
        EventEnvelope { version: CONTRACT_VERSION, event_id: format!("event-{id}"), generation, job_id: None, session_id: None, event }
    }
    pub fn set_event_sink(&self, sink: StdArc<dyn Fn(EventEnvelope) -> Result<(), String> + Send + Sync>) { *self.event_sink.lock().unwrap() = Some(sink); }
    pub fn publish(&self, envelope: EventEnvelope) {
        if let Some(sink) = self.event_sink.lock().unwrap().clone() {
            if sink(envelope).is_err() {
                let failure = self.event(Generation(0), Event::Error { error: ContractError { code: ErrorCode::Internal, message: "backend event delivery failed".into(), retryable: true } });
                self.events.lock().unwrap().push(failure);
            }
        } else { self.events.lock().unwrap().push(envelope); }
    }
    pub fn next_generation(&self) -> Generation { Generation(self.next_generation.fetch_add(1, Ordering::Relaxed) + 1) }
    pub fn scan_async(&self) -> Result<Generation, SpawnError> {
        let generation = self.next_generation();
        let spec = self.generation("library-scan", generation);
        let state = self.clone();
        let roots = self.roots.lock().unwrap().clone();
        self.submit(spec.clone(), move |token| {
            let outcome = scan_library(&roots, &OsFileSystem);
            if token.is_cancelled() || !state.publish_allowed(&spec) { return; }
            let statuses: Vec<_> = outcome.roots.iter().map(|(_, status)| match status {
                crate::library::scanner::RootStatus::Scanned => RootStatus::Scanned,
                crate::library::scanner::RootStatus::Missing => RootStatus::Missing,
                crate::library::scanner::RootStatus::Inaccessible => RootStatus::Inaccessible,
            }).collect();
            *state.root_statuses.lock().unwrap() = statuses.clone();
            for (root_index, status) in statuses.iter().copied().enumerate() {
                let mut event = state.event(generation, Event::LibraryRootStatus { root_index, status });
                event.job_id = Some(JobId("library-scan".into()));
                state.publish(event);
                if !matches!(status, RootStatus::Scanned) {
                    let mut error = state.event(generation, Event::Error { error: ContractError { code: ErrorCode::Internal, message: "library root could not be scanned".into(), retryable: true } });
                    error.job_id = Some(JobId("library-scan".into()));
                    state.publish(error);
                }
            }
            let snapshot = state.replace_library(generation, outcome.playlists);
            let mut event = state.event(generation, Event::LibrarySnapshot { snapshot });
            event.job_id = Some(JobId("library-scan".into()));
            state.publish(event);
        }).map(|_| generation)
    }
    fn replace_library(&self, generation: Generation, playlists: BTreeMap<String, Vec<PathBuf>>) -> LibrarySnapshot {
        let mut paths = BTreeMap::<PathBuf, TrackId>::new();
        let mut tracks = Vec::new();
        let mut playlist_data = Vec::new();
        for (name, files) in playlists {
            let playlist_id = crate::contracts::types::PlaylistId(format!("playlist:{}", name));
            let mut ids = Vec::new();
            for path in files {
                let id = paths.entry(path.clone()).or_insert_with(|| {
                    let metadata = crate::domain::identity::filename_metadata(&path.to_string_lossy());
                    let id = TrackId(crate::domain::identity::track_id(&metadata));
                    tracks.push(Track { id: id.clone(), path: None, title: metadata.title, artist: (!metadata.artist.is_empty()).then_some(metadata.artist), album: None, duration_ms: None, artwork_url: None, favorite: false, provider: None });
                    id
                }).clone();
                ids.push(id);
            }
            playlist_data.push(Playlist { id: playlist_id, name: name.clone(), path: None, url: None, ordered_track_ids: ids });
        }
        let snapshot = LibrarySnapshot { version: CONTRACT_VERSION, generation, tracks, playlists: playlist_data };
        *self.library.lock().unwrap() = snapshot.clone();
        snapshot
    }
    pub fn submit<T, F>(&self, spec: TaskSpec, work: F) -> Result<TaskHandle<T>, SpawnError>
    where T: Send + 'static, F: FnOnce(CancellationToken) -> T + Send + 'static { self.tasks.spawn(spec, work) }
    pub fn generation(&self, key: impl Into<String>, generation: Generation) -> TaskSpec { TaskSpec::new(key, generation, None) }
    pub fn session_task(&self, key: impl Into<String>, generation: Generation, session: SessionId) -> TaskSpec { TaskSpec::new(key, generation, Some(session)) }
    pub fn publish_allowed(&self, spec: &TaskSpec) -> bool { self.tasks.is_current(spec) }
    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport { self.tasks.shutdown(deadline) }
    pub fn is_closing(&self) -> bool { self.tasks.is_closing() }

    pub fn dispatch(&self, envelope: CommandEnvelope) -> Result<Value, ContractError> {
        if envelope.request_id.trim().is_empty() { return Err(ContractError::invalid("request_id must not be empty")); }
        if !matches!(envelope.command, Command::Shutdown { .. }) && self.is_closing() { return Err(ContractError { code: ErrorCode::ShuttingDown, message: "backend is shutting down".into(), retryable: false }); }
        match envelope.command {
            Command::GetLibrary => {
                let snapshot = self.library();
                self.publish(self.event(snapshot.generation, Event::LibrarySnapshot { snapshot: snapshot.clone() }));
                serde_json::to_value(snapshot).map_err(|e| ContractError { code: ErrorCode::Internal, message: e.to_string(), retryable: false })
            }
            Command::ScanLibrary => {
                let generation = self.scan_async().map_err(|e| match e { SpawnError::AtCapacity => ContractError { code: ErrorCode::Busy, message: "task capacity is full".into(), retryable: true }, SpawnError::ShuttingDown => ContractError { code: ErrorCode::ShuttingDown, message: "backend is shutting down".into(), retryable: false } })?;
                Ok(serde_json::json!({ "generation": generation.0, "job_id": "library-scan" }))
            }
            Command::CancelJob { job_id } => {
                if self.tasks.cancel_key(&job_id.0) { Ok(Value::Null) }
                else { Err(ContractError { code: ErrorCode::NotFound, message: "job not found".into(), retryable: false }) }
            }
            Command::Play { .. } | Command::Pause | Command::Seek { .. } => Err(ContractError { code: ErrorCode::Internal, message: "audio backend unavailable".into(), retryable: true }),
            Command::Shutdown { reason } => {
                let report = self.shutdown_events(reason, Duration::from_millis(500));
                if report.timed_out {
                    Err(ContractError { code: ErrorCode::Internal, message: "shutdown timed out; tasks remain active".into(), retryable: false })
                } else { Ok(Value::Null) }
            }
        }
    }

    pub fn shutdown_events(&self, reason: crate::contracts::events::ShutdownReason, deadline: Duration) -> ShutdownReport {
        if self.shutdown_started.swap(true, Ordering::AcqRel) { return ShutdownReport { joined: 0, timed_out: false }; }
        self.publish(self.event(Generation(0), Event::Shutdown { envelope: ShutdownEnvelope { version: CONTRACT_VERSION, state: ShutdownState::Requested, reason } }));
        self.publish(self.event(Generation(0), Event::Shutdown { envelope: ShutdownEnvelope { version: CONTRACT_VERSION, state: ShutdownState::Draining, reason } }));
        let report = self.shutdown(deadline);
        let state = if report.timed_out { ShutdownState::TimedOut } else { ShutdownState::Complete };
        self.publish(self.event(Generation(0), Event::Shutdown { envelope: ShutdownEnvelope { version: CONTRACT_VERSION, state, reason } }));
        if report.timed_out { self.publish(self.event(Generation(0), Event::Error { error: ContractError { code: ErrorCode::Internal, message: "shutdown timed out; tasks remain active".into(), retryable: false } })); }
        report
    }
}
