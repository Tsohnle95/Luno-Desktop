use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use serde::Serialize;
use std::sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Mutex};
use std::time::Duration;
use crate::contracts::{events::{Command, CommandEnvelope, Event, EventEnvelope, RootStatus, ShutdownEnvelope, ShutdownState}, types::{ContractError, ErrorCode, Generation, JobId, LibrarySnapshot, Playlist, Track, TrackId, SessionId, CONTRACT_VERSION}};
use serde_json::Value;
use std::sync::Arc as StdArc;
use crate::library::scanner::{scan_library, OsFileSystem};
use crate::process::supervisor::{ProcessRegistry, Supervisor};
use crate::download::coordinator::DownloadCoordinator;
use crate::domain::identity::TrackMetadata;
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

    #[test]
    fn provider_metadata_is_used_by_scan_publication_path() {
        let state = AppState::new(1);
        let path = PathBuf::from("/music/song.mp3");
        state.set_provider_metadata(path.clone(), TrackMetadata {
            title: "Provider title".into(),
            artist: "Provider artist".into(),
            duration_ms: 1234,
            provider: Some(crate::domain::identity::ProviderIdentity { provider: "youtube".into(), id: "abc".into() }),
        });

        let snapshot = state.replace_library(Generation(1), BTreeMap::from([(String::from("Unsorted"), vec![path])]));

        assert_eq!(snapshot.tracks[0].title, "Provider title");
        assert_eq!(snapshot.tracks[0].provider.as_ref().map(|provider| provider.id.as_str()), Some("abc"));
    }

    #[test]
    fn failed_event_delivery_preserves_event_until_acknowledged() {
        let state = AppState::new(1);
        state.set_event_sink(StdArc::new(|_| Err("emit failed".into())));
        let event = state.event(Generation(1), Event::LibraryRootStatus { root_index: 0, status: RootStatus::Scanned });
        state.publish(event.clone());

        let pending = state.pending_events();
        assert_eq!(pending[0], event);
        assert!(matches!(pending[1].event, Event::Error { .. }));

        state.acknowledge_event(&pending[0].event_id);
        assert_eq!(state.pending_events().len(), 1);
    }

    #[test]
    fn event_drain_has_one_consumer_and_requeues_failures() {
        let state = AppState::new(1);
        state.publish(state.event(Generation(1), Event::LibraryRootStatus { root_index: 0, status: RootStatus::Scanned }));
        let worker_state = state.clone();
        let entered = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = entered.clone();
        let first = std::thread::spawn(move || worker_state.drain_events(|_| { release.wait(); std::thread::sleep(Duration::from_millis(20)); Ok(()) }).unwrap());
        entered.wait();
        assert_eq!(state.drain_events(|_| Ok(())).unwrap(), false);
        assert_eq!(first.join().unwrap(), true);

        state.publish(state.event(Generation(1), Event::LibraryRootStatus { root_index: 1, status: RootStatus::Scanned }));
        assert!(state.drain_events(|_| Err("offline".into())).is_err());
        assert_eq!(state.pending_events().len(), 1);
    }

    #[test]
    fn panicking_worker_is_a_shutdown_failure() {
        let state = AppState::new(1);
        state.submit(TaskSpec::new("panic", Generation(1), None), |_| -> () { panic!("worker failure") }).unwrap();
        let report = state.shutdown_events(crate::contracts::events::ShutdownReason::System, Duration::from_secs(1));
        assert!(report.failed);
        assert!(!report.timed_out);
        assert!(state.events().iter().any(|event| matches!(event.event, Event::Shutdown { envelope: ShutdownEnvelope { state: ShutdownState::Failed, .. } })));
    }

    #[test]
    fn shutdown_admission_barrier_is_active_before_requested_event() {
        let state = AppState::new(1);
        let observed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed_sink = observed.clone();
        let sink_state = state.clone();
        state.set_event_sink(StdArc::new(move |event| {
            if matches!(&event.event, Event::Shutdown { envelope: ShutdownEnvelope { state: ShutdownState::Requested, .. } }) {
                observed_sink.store(sink_state.is_closing(), std::sync::atomic::Ordering::Release);
            }
            Ok(())
        }));
        state.shutdown_events(crate::contracts::events::ShutdownReason::System, Duration::from_secs(1));
        assert!(observed.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn publication_is_serialized_and_latest_events_replay() {
        let state = AppState::new(1);
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink_delivered = delivered.clone();
        state.set_event_sink(StdArc::new(move |event| {
            sink_delivered.lock().unwrap().push(event.event_id.clone());
            Ok(())
        }));
        let snapshot = state.event(Generation(1), Event::LibrarySnapshot { snapshot: state.library() });
        let shutdown = state.event(Generation(0), Event::Shutdown { envelope: ShutdownEnvelope { version: CONTRACT_VERSION, state: ShutdownState::Complete, reason: crate::contracts::events::ShutdownReason::System } });
        state.publish(snapshot.clone());
        state.publish(shutdown.clone());
        *delivered.lock().unwrap() = Vec::new();
        state.replay_latest();
        assert_eq!(*delivered.lock().unwrap(), vec![snapshot.event_id, shutdown.event_id]);
    }

    #[test]
    fn newer_publication_waits_for_older_sink_delivery() {
        let state = AppState::new(1);
        let entered = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = std::sync::Arc::new(std::sync::Barrier::new(2));
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink_delivered = delivered.clone();
        let sink_release = release.clone();
        state.set_event_sink(StdArc::new(move |event| {
            sink_delivered.lock().unwrap().push(event.event_id.clone());
            if event.event_id == "event-1" { entered.wait(); sink_release.wait(); }
            Ok(())
        }));
        let first = state.event(Generation(1), Event::LibraryRootStatus { root_index: 0, status: RootStatus::Scanned });
        let second = state.event(Generation(1), Event::LibraryRootStatus { root_index: 1, status: RootStatus::Scanned });
        let worker_state = state.clone();
        let first_thread = std::thread::spawn(move || worker_state.publish(first));
        entered.wait();
        let worker_state = state.clone();
        let second_thread = std::thread::spawn(move || worker_state.publish(second));
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(delivered.lock().unwrap().len(), 1);
        release.wait();
        first_thread.join().unwrap();
        second_thread.join().unwrap();
        assert_eq!(delivered.lock().unwrap().len(), 2);
    }

    #[test]
    fn allocated_event_ids_publish_in_allocation_order() {
        let state = AppState::new(1);
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink_delivered = delivered.clone();
        state.set_event_sink(StdArc::new(move |event| { sink_delivered.lock().unwrap().push(event.event_id); Ok(()) }));
        let first = state.event(Generation(1), Event::LibraryRootStatus { root_index: 0, status: RootStatus::Scanned });
        let second = state.event(Generation(1), Event::LibraryRootStatus { root_index: 1, status: RootStatus::Scanned });
        state.publish(second);
        assert!(delivered.lock().unwrap().is_empty());
        state.publish(first);
        assert_eq!(*delivered.lock().unwrap(), vec!["event-1", "event-2"]);
    }

    #[test]
    fn retry_without_sink_reports_buffered_events() {
        let state = AppState::new(1);
        state.publish(state.event(Generation(1), Event::LibraryRootStatus { root_index: 0, status: RootStatus::Scanned }));
        assert_eq!(state.retry_event_delivery(Duration::ZERO), 1);
    }

    #[test]
    fn retry_reports_an_active_drainer_until_it_finishes() {
        let state = AppState::new(1);
        state.publish(state.event(Generation(1), Event::LibraryRootStatus { root_index: 0, status: RootStatus::Scanned }));
        let entered = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = entered.clone();
        let worker_state = state.clone();
        let worker = std::thread::spawn(move || worker_state.drain_events(|_| { entered.wait(); release.wait(); Ok(()) }));
        entered.wait();
        assert_eq!(state.retry_event_delivery(Duration::ZERO), 1);
        release.wait();
        assert_eq!(worker.join().unwrap(), Ok(true));
        assert_eq!(state.retry_event_delivery(Duration::ZERO), 0);
    }

    #[test]
    fn replay_uses_the_pending_order_without_duplicates() {
        let state = AppState::new(1);
        let snapshot = state.event(Generation(1), Event::LibrarySnapshot { snapshot: state.library() });
        let shutdown = state.event(Generation(0), Event::Shutdown { envelope: ShutdownEnvelope { version: CONTRACT_VERSION, state: ShutdownState::Complete, reason: crate::contracts::events::ShutdownReason::System } });
        state.publish(snapshot.clone());
        state.publish(shutdown.clone());
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink_delivered = delivered.clone();
        state.set_event_sink(StdArc::new(move |event| { sink_delivered.lock().unwrap().push(event.event_id); Ok(()) }));

        state.replay_latest();

        assert_eq!(*delivered.lock().unwrap(), vec![snapshot.event_id, shutdown.event_id]);
        assert!(state.pending_events().is_empty());
        state.replay_latest();
        assert_eq!(*delivered.lock().unwrap(), vec![snapshot.event_id, shutdown.event_id]);
    }

    #[test]
    fn panicking_delivery_resets_the_drainer() {
        let state = AppState::new(1);
        state.publish(state.event(Generation(1), Event::LibraryRootStatus { root_index: 0, status: RootStatus::Scanned }));
        assert!(state.drain_events(|_| panic!("sink panic")).is_err());
        assert_eq!(state.drain_events(|_| Ok(())), Ok(true));
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
    event_draining: Arc<Mutex<bool>>,
    event_in_flight: Arc<Mutex<usize>>,
    next_event: Arc<AtomicU64>,
    next_publish: Arc<AtomicU64>,
    next_generation: Arc<AtomicU64>,
    event_sink: Arc<Mutex<Option<StdArc<dyn Fn(EventEnvelope) -> Result<(), String> + Send + Sync>>>>,
    publication: Arc<Mutex<()>>,
    unpublished: Arc<Mutex<BTreeMap<u64, EventEnvelope>>>,
    skipped_publications: Arc<Mutex<BTreeSet<u64>>>,
    admission: Arc<Mutex<()>>,
    latest_snapshot: Arc<Mutex<Option<EventEnvelope>>>,
    latest_control: Arc<Mutex<Option<EventEnvelope>>>,
    replayed_events: Arc<Mutex<BTreeSet<String>>>,
    event_retry_stopped: Arc<AtomicBool>,
    shutdown_result: Arc<Mutex<Option<ShutdownReport>>>,
    supervisor: Arc<Supervisor>,
    downloads: Arc<DownloadCoordinator>,
    provider_metadata: Arc<Mutex<BTreeMap<PathBuf, TrackMetadata>>>,
}

impl AppState {
    pub fn new(task_limit: usize) -> Self {
        Self { tasks: Arc::new(TaskRegistry::new(task_limit)), migration_status: Arc::new(Mutex::new(MigrationStatus::NotRun)),
            library: Arc::new(Mutex::new(LibrarySnapshot { version: CONTRACT_VERSION, generation: Generation(0), tracks: Vec::new(), playlists: Vec::new() })),
            roots: Arc::new(Mutex::new(Vec::new())), root_statuses: Arc::new(Mutex::new(Vec::new())), events: Arc::new(Mutex::new(Vec::new())), event_draining: Arc::new(Mutex::new(false)), event_in_flight: Arc::new(Mutex::new(0)),
            next_event: Arc::new(AtomicU64::new(0)), next_publish: Arc::new(AtomicU64::new(1)), next_generation: Arc::new(AtomicU64::new(0)), event_sink: Arc::new(Mutex::new(None)), publication: Arc::new(Mutex::new(())), unpublished: Arc::new(Mutex::new(BTreeMap::new())), skipped_publications: Arc::new(Mutex::new(BTreeSet::new())), admission: Arc::new(Mutex::new(())), latest_snapshot: Arc::new(Mutex::new(None)), latest_control: Arc::new(Mutex::new(None)), replayed_events: Arc::new(Mutex::new(BTreeSet::new())), event_retry_stopped: Arc::new(AtomicBool::new(false)), shutdown_result: Arc::new(Mutex::new(None)), supervisor: Arc::new(Supervisor::new(ProcessRegistry::new())), downloads: Arc::new(DownloadCoordinator::new()), provider_metadata: Arc::new(Mutex::new(BTreeMap::new())) }
    }
    pub fn library(&self) -> LibrarySnapshot { self.library.lock().unwrap().clone() }
    pub fn root_statuses(&self) -> Vec<RootStatus> { self.root_statuses.lock().unwrap().clone() }
    pub fn set_scan_roots(&self, roots: Vec<PathBuf>) { *self.roots.lock().unwrap() = roots; }
    pub fn pending_events(&self) -> Vec<EventEnvelope> { self.events.lock().unwrap().clone() }
    /// Test and integration accessor for the buffered event queue.
    pub fn events(&self) -> Vec<EventEnvelope> { self.pending_events() }
    pub fn acknowledge_event(&self, event_id: &str) { self.events.lock().unwrap().retain(|event| event.event_id != event_id); }
    /// Drain is single-consumer: events are claimed while holding the queue lock and
    /// failed deliveries are returned atomically instead of being duplicated.
    pub fn drain_events<F>(&self, deliver: F) -> Result<bool, String>
    where F: Fn(&EventEnvelope) -> Result<(), String> {
        {
            let mut draining = self.event_draining.lock().unwrap();
            if *draining { return Ok(false); }
            *draining = true;
        }
        struct DrainGuard<'a> { draining: &'a Mutex<bool>, in_flight: &'a Mutex<usize> }
        impl Drop for DrainGuard<'_> {
            fn drop(&mut self) {
                *self.in_flight.lock().unwrap() = 0;
                *self.draining.lock().unwrap() = false;
            }
        }
        let _guard = DrainGuard { draining: &self.event_draining, in_flight: &self.event_in_flight };
        let _publication = self.publication.lock().unwrap();
        let claimed = { std::mem::take(&mut *self.events.lock().unwrap()) };
        *self.event_in_flight.lock().unwrap() = claimed.len();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for (index, event) in claimed.iter().enumerate() {
                if let Err(message) = deliver(event) { return Err((index, message)); }
            }
            Ok(())
        }));
        let (failed, error) = match outcome {
            Ok(Ok(())) => (Vec::new(), None),
            Ok(Err((index, message))) => (claimed[index..].to_vec(), Some(message)),
            Err(_) => (claimed.clone(), Some("event delivery panicked".into())),
        };
        if !failed.is_empty() { self.events.lock().unwrap().splice(0..0, failed); }
        error.map_or(Ok(true), Err)
    }
    pub fn event(&self, generation: Generation, event: Event) -> EventEnvelope {
        let _publication = self.publication.lock().unwrap();
        self.event_locked(generation, event)
    }

    fn event_locked(&self, generation: Generation, event: Event) -> EventEnvelope {
        let id = self.next_event.fetch_add(1, Ordering::Relaxed) + 1;
        EventEnvelope { version: CONTRACT_VERSION, event_id: format!("event-{id}"), generation, job_id: None, session_id: None, event }
    }
    pub fn set_event_sink(&self, sink: StdArc<dyn Fn(EventEnvelope) -> Result<(), String> + Send + Sync>) {
        *self.event_sink.lock().unwrap() = Some(sink);
        self.replayed_events.lock().unwrap().clear();
    }
    /// Replay the latest state/control envelopes after a listener has started.
    pub fn replay_latest(&self) {
        let Some(sink) = self.event_sink.lock().unwrap().clone() else { return; };
        {
            let _publication = self.publication.lock().unwrap();
            for envelope in [self.latest_snapshot.lock().unwrap().clone(), self.latest_control.lock().unwrap().clone()].into_iter().flatten() {
                let mut events = self.events.lock().unwrap();
                let mut replayed = self.replayed_events.lock().unwrap();
                if !events.iter().any(|pending| pending.event_id == envelope.event_id) && !replayed.contains(&envelope.event_id) {
                    let event_id = envelope.event_id.clone();
                    events.push(envelope);
                    events.sort_by_key(|event| event.event_id.strip_prefix("event-").and_then(|id| id.parse::<u64>().ok()).unwrap_or(u64::MAX));
                    replayed.insert(event_id);
                }
            }
        }
        let _ = self.drain_events(|event| sink(event.clone()));
    }
    pub fn stop_event_delivery(&self) { self.event_retry_stopped.store(true, Ordering::Release); }
    pub fn event_delivery_stopped(&self) -> bool { self.event_retry_stopped.load(Ordering::Acquire) }
    pub fn set_provider_metadata(&self, path: PathBuf, metadata: TrackMetadata) { self.provider_metadata.lock().unwrap().insert(path, metadata); }
    pub fn publish(&self, envelope: EventEnvelope) {
        let _publication = self.publication.lock().unwrap();
        let id = envelope.event_id.strip_prefix("event-").and_then(|value| value.parse::<u64>().ok()).expect("internal event IDs are numeric");
        self.unpublished.lock().unwrap().insert(id, envelope);
        self.publish_ready_locked();
    }

    fn publish_ready_locked(&self) {
        loop {
            let next = self.next_publish.load(Ordering::Acquire);
            if self.skipped_publications.lock().unwrap().remove(&next) {
                self.next_publish.fetch_add(1, Ordering::Release);
                continue;
            }
            let Some(envelope) = self.unpublished.lock().unwrap().remove(&next) else { break; };
            self.next_publish.fetch_add(1, Ordering::Release);
            self.publish_one_locked(envelope);
        }
    }

    fn publish_one_locked(&self, envelope: EventEnvelope) {
        if matches!(&envelope.event, Event::LibrarySnapshot { .. }) { *self.latest_snapshot.lock().unwrap() = Some(envelope.clone()); }
        if matches!(&envelope.event, Event::Shutdown { .. }) { *self.latest_control.lock().unwrap() = Some(envelope.clone()); }
        if let Some(sink) = self.event_sink.lock().unwrap().clone() {
            if sink(envelope.clone()).is_err() {
                self.events.lock().unwrap().push(envelope);
                let failure = self.event_locked(Generation(0), Event::Error { error: ContractError { code: ErrorCode::Internal, message: "backend event delivery failed".into(), retryable: true } });
                let failure_id = failure.event_id.strip_prefix("event-").and_then(|value| value.parse::<u64>().ok()).expect("internal event IDs are numeric");
                self.events.lock().unwrap().push(failure);
                self.skipped_publications.lock().unwrap().insert(failure_id);
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
            for issue in outcome.errors {
                let scope = issue.playlist.map_or_else(|| "root".into(), |playlist| format!("playlist {playlist}"));
                let root_path = outcome.roots.get(issue.root_index).map(|(path, _)| path.to_string_lossy().into_owned()).unwrap_or_default();
                let detail = issue.message.replace(&root_path, "<root>");
                let mut error = state.event(generation, Event::Error { error: ContractError { code: ErrorCode::Internal, message: format!("library scan failed for root {} ({scope}): {detail}"), retryable: true } });
                error.job_id = Some(JobId("library-scan".into()));
                state.publish(error);
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
                    let metadata = self.provider_metadata.lock().unwrap().get(&path).cloned().unwrap_or_else(|| crate::domain::identity::filename_metadata(&path.to_string_lossy()));
                    let id = TrackId(crate::domain::identity::track_id(&metadata));
                    tracks.push(Track { id: id.clone(), path: None, title: metadata.title, artist: (!metadata.artist.is_empty()).then_some(metadata.artist), album: None, duration_ms: (metadata.duration_ms != 0).then_some(metadata.duration_ms), artwork_url: None, favorite: false, provider: metadata.provider.map(|provider| crate::contracts::types::ProviderId { provider: provider.provider, id: provider.id }) });
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
        let admission = self.admission.lock().unwrap();
        if !matches!(&envelope.command, Command::Shutdown { .. }) && self.is_closing() { return Err(ContractError { code: ErrorCode::ShuttingDown, message: "backend is shutting down".into(), retryable: false }); }
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
                let report = self.shutdown_events_locked(reason, Duration::from_millis(500), &admission);
                if report.failed {
                    Err(ContractError { code: ErrorCode::Internal, message: "shutdown failed because a worker panicked".into(), retryable: false })
                } else if report.timed_out {
                    Err(ContractError { code: ErrorCode::Internal, message: "shutdown timed out; tasks or events remain undelivered".into(), retryable: false })
                } else { Ok(Value::Null) }
            }
        }
    }

    pub fn shutdown_events(&self, reason: crate::contracts::events::ShutdownReason, deadline: Duration) -> ShutdownReport {
        let admission = self.admission.lock().unwrap();
        self.shutdown_events_locked(reason, deadline, &admission)
    }

    fn shutdown_events_locked(&self, reason: crate::contracts::events::ShutdownReason, deadline: Duration, _admission: &std::sync::MutexGuard<'_, ()>) -> ShutdownReport {
        let mut stored_result = self.shutdown_result.lock().unwrap();
        if let Some(report) = *stored_result { return report; }
        self.tasks.begin_shutdown();
        self.publish(self.event(Generation(0), Event::Shutdown { envelope: ShutdownEnvelope { version: CONTRACT_VERSION, state: ShutdownState::Requested, reason } }));
        self.publish(self.event(Generation(0), Event::Shutdown { envelope: ShutdownEnvelope { version: CONTRACT_VERSION, state: ShutdownState::Draining, reason } }));
        let started = std::time::Instant::now();
        let report = self.shutdown(deadline);
        let remaining = deadline.saturating_sub(started.elapsed());
        let process_ok = self.supervisor.shutdown(remaining);
        let remaining = deadline.saturating_sub(started.elapsed());
        let download_report = self.downloads.shutdown(remaining);
        let remaining = deadline.saturating_sub(started.elapsed());
        let pending_before_terminal = self.retry_event_delivery(remaining);
        let timed_out = report.timed_out || !process_ok || download_report.timed_out || pending_before_terminal > 0;
        let failed = report.failed;
        let state = if failed { ShutdownState::Failed } else if timed_out { ShutdownState::TimedOut } else { ShutdownState::Complete };
        self.publish(self.event(Generation(0), Event::Shutdown { envelope: ShutdownEnvelope { version: CONTRACT_VERSION, state, reason } }));
        if failed { self.publish(self.event(Generation(0), Event::Error { error: ContractError { code: ErrorCode::Internal, message: "shutdown failed because a worker panicked".into(), retryable: false } })); }
        else if timed_out { self.publish(self.event(Generation(0), Event::Error { error: ContractError { code: ErrorCode::Internal, message: "shutdown timed out; workers remain active".into(), retryable: false } })); }
        let remaining = deadline.saturating_sub(started.elapsed());
        let undelivered = self.retry_event_delivery(remaining);
        let result = ShutdownReport { joined: report.joined, timed_out, failed, undelivered };
        *stored_result = Some(result);
        result
    }

    fn retry_event_delivery(&self, deadline: Duration) -> usize {
        let started = std::time::Instant::now();
        loop {
            let pending = self.pending_events().len();
            let in_flight = *self.event_in_flight.lock().unwrap();
            let active = *self.event_draining.lock().unwrap();
            let remaining_events = pending + in_flight.max(usize::from(active));
            let Some(sink) = self.event_sink.lock().unwrap().clone() else { return remaining_events; };
            let drained = self.drain_events(|event| sink(event.clone()));
            let pending = self.pending_events().len();
            let in_flight = *self.event_in_flight.lock().unwrap();
            let active = *self.event_draining.lock().unwrap();
            let remaining = pending + in_flight.max(usize::from(active));
            if remaining == 0 && !matches!(drained, Ok(false)) { return 0; }
            if started.elapsed() >= deadline { return remaining; }
            std::thread::yield_now();
        }
    }

    pub fn run_process(&self, spec: crate::process::supervisor::ProcessSpec) -> Result<crate::process::supervisor::ProcessOutput, crate::process::supervisor::SuperviseError> {
        let _admission = self.admission.lock().unwrap();
        self.supervisor.run_admitted(spec, &self.tasks)
    }
    pub fn process_count(&self) -> usize { self.supervisor.registry.active_count() }
}
