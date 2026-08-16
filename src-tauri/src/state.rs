use crate::contracts::{
    events::{
        Command, CommandEnvelope, Event, EventEnvelope, RepeatMode as ContractRepeatMode, RootStatus, ShutdownEnvelope, ShutdownState,
    },
    types::{
        ContractError, ErrorCode, Generation, JobId, LibrarySnapshot, Playlist, SessionId, Track,
        TrackId, CONTRACT_VERSION, MAX_SAFE_EVENT_ID,
    },
};
use crate::domain::identity::TrackMetadata;
use crate::download::coordinator::DownloadCoordinator;
use crate::download::markers::Markers;
use crate::download::promotion::{promote_output, CollisionPolicy, PromotionError, Validation};
use crate::library::favorites::Favorites;
use crate::library::scanner::{OsFileSystem, ScanOutcome};
#[cfg(target_os = "macos")]
use crate::playback::{
    backend::{AudioBackend, BackendError},
    backend_adapter::CpalSymphoniaBackend,
};
use crate::process::supervisor::{ProcessRegistry, Supervisor};
use crate::playback::{clock::PlaybackClock, queue::QueueState, recovery::FailureRecovery};
use crate::runtime::task_registry::{
    CancellationToken, ShutdownReport, SpawnError, TaskHandle, TaskRegistry, TaskSpec,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Arc as StdArc;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    NotRun,
    Skipped {
        reason: String,
    },
    Completed {
        warnings: usize,
        snapshot_created: bool,
        removed_previews: usize,
    },
    Failed {
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_status_is_sanitized_diagnostic_data() {
        let status = MigrationStatus::Completed {
            warnings: 1,
            snapshot_created: true,
            removed_previews: 2,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("snapshot_created"));
        assert!(!json.contains("/"));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn playback_commands_report_explicit_backend_unavailability() {
        let state = AppState::new(1);
        let error = state
            .dispatch(CommandEnvelope {
                version: 1,
                request_id: "play-test".into(),
                command: Command::Play {
                    track_id: TrackId("track".into()),
                },
            })
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert!(!error.retryable);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn playback_commands_report_missing_track_and_unloaded_state() {
        let state = AppState::new(1);
        let play = state
            .dispatch(CommandEnvelope {
                version: 1,
                request_id: "play-missing".into(),
                command: Command::Play {
                    track_id: TrackId("missing".into()),
                },
            })
            .unwrap_err();
        assert_eq!(play.code, ErrorCode::NotFound);
        let pause = state
            .dispatch(CommandEnvelope {
                version: 1,
                request_id: "pause-empty".into(),
                command: Command::Pause,
            })
            .unwrap_err();
        assert_eq!(pause.code, ErrorCode::NotFound);
    }

    #[test]
    fn provider_metadata_is_used_by_scan_publication_path() {
        let state = AppState::new(1);
        let path = PathBuf::from("/music/song.mp3");
        state.set_provider_metadata(
            path.clone(),
            TrackMetadata {
                title: "Provider title".into(),
                artist: "Provider artist".into(),
                duration_ms: 1234,
                provider: Some(crate::domain::identity::ProviderIdentity {
                    provider: "youtube".into(),
                    id: "abc".into(),
                }),
            },
        );

        let snapshot = state.replace_library(
            Generation(1),
            BTreeMap::from([(String::from("Unsorted"), vec![path])]),
        );

        assert_eq!(snapshot.tracks[0].title, "Provider title");
        assert_eq!(
            snapshot.tracks[0]
                .provider
                .as_ref()
                .map(|provider| provider.id.as_str()),
            Some("abc")
        );
    }

    #[test]
    fn failed_event_delivery_preserves_event_until_acknowledged() {
        let state = AppState::new(1);
        state.set_event_sink(StdArc::new(|_| Err("emit failed".into())));
        let event = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        state.publish(event.clone());

        let pending = state.pending_events();
        assert_eq!(pending[0], event);
        assert!(matches!(pending[1].event, Event::Error { .. }));

        state.acknowledge_event(&pending[0].event_id);
        assert_eq!(state.pending_events().len(), 1);
    }

    #[test]
    fn delivery_error_id_is_outside_domain_sequence() {
        let state = AppState::new(1);
        state.set_event_sink(StdArc::new(|_| Err("offline".into())));
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        let ids: Vec<_> = state
            .pending_events()
            .into_iter()
            .map(|event| event.event_id)
            .collect();
        assert_eq!(ids[0], "event-1");
        assert!(ids[1]
            .strip_prefix("delivery-error-")
            .is_some_and(|id| id.parse::<u64>().is_ok()));
        assert_ne!(ids[1], "delivery-error-2");
    }

    #[test]
    fn event_drain_has_one_consumer_and_requeues_failures() {
        let state = AppState::new(1);
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        let worker_state = state.clone();
        let entered = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = entered.clone();
        let first = std::thread::spawn(move || {
            worker_state
                .drain_events(|_| {
                    release.wait();
                    std::thread::sleep(Duration::from_millis(20));
                    Ok(())
                })
                .unwrap()
        });
        entered.wait();
        assert_eq!(state.drain_events(|_| Ok(())).unwrap(), false);
        assert_eq!(first.join().unwrap(), true);

        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        ));
        assert!(state.drain_events(|_| Err("offline".into())).is_err());
        assert_eq!(state.pending_events().len(), 1);
    }

    #[test]
    fn panicking_worker_is_a_shutdown_failure() {
        let state = AppState::new(1);
        state
            .submit(TaskSpec::new("panic", Generation(1), None), |_| -> () {
                panic!("worker failure")
            })
            .unwrap();
        let report = state.shutdown_events(
            crate::contracts::events::ShutdownReason::System,
            Duration::from_secs(1),
        );
        assert!(report.failed);
        assert!(!report.timed_out);
        assert!(state.events().iter().any(|event| matches!(
            event.event,
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    state: ShutdownState::Failed,
                    ..
                }
            }
        )));
    }

    #[test]
    fn shutdown_admission_barrier_is_active_before_requested_event() {
        let state = AppState::new(1);
        let observed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed_sink = observed.clone();
        let sink_state = state.clone();
        state.set_event_sink(StdArc::new(move |event| {
            if matches!(
                &event.event,
                Event::Shutdown {
                    envelope: ShutdownEnvelope {
                        state: ShutdownState::Requested,
                        ..
                    }
                }
            ) {
                observed_sink.store(
                    sink_state.is_closing(),
                    std::sync::atomic::Ordering::Release,
                );
            }
            Ok(())
        }));
        state.shutdown_events(
            crate::contracts::events::ShutdownReason::System,
            Duration::from_secs(1),
        );
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
        let snapshot = state.event(
            Generation(1),
            Event::LibrarySnapshot {
                snapshot: state.library(),
            },
        );
        let shutdown = state.event(
            Generation(0),
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    version: CONTRACT_VERSION,
                    state: ShutdownState::Complete,
                    reason: crate::contracts::events::ShutdownReason::System,
                },
            },
        );
        state.publish(snapshot.clone());
        state.publish(shutdown.clone());
        *delivered.lock().unwrap() = Vec::new();
        state.replay_latest();
        assert_eq!(
            *delivered.lock().unwrap(),
            vec![snapshot.event_id, shutdown.event_id]
        );
    }

    #[test]
    fn newer_publication_waits_for_older_sink_delivery() {
        let state = AppState::new(1);
        let entered = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = std::sync::Arc::new(std::sync::Barrier::new(2));
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink_delivered = delivered.clone();
        let sink_release = release.clone();
        let sink_entered = entered.clone();
        state.set_event_sink(StdArc::new(move |event| {
            sink_delivered.lock().unwrap().push(event.event_id.clone());
            if event.event_id == "event-1" {
                sink_entered.wait();
                sink_release.wait();
            }
            Ok(())
        }));
        let first = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        let second = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        );
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
        state.set_event_sink(StdArc::new(move |event| {
            sink_delivered.lock().unwrap().push(event.event_id);
            Ok(())
        }));
        let first = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        let second = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        );
        state.publish(second);
        assert!(delivered.lock().unwrap().is_empty());
        state.publish(first);
        assert_eq!(*delivered.lock().unwrap(), vec!["event-1", "event-2"]);
    }

    #[test]
    fn retry_without_sink_reports_buffered_events() {
        let state = AppState::new(1);
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        assert_eq!(state.retry_event_delivery(Duration::ZERO), 1);
    }

    #[test]
    fn retry_reports_an_active_drainer_until_it_finishes() {
        let state = AppState::new(1);
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        let entered = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = entered.clone();
        let worker_state = state.clone();
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let worker = std::thread::spawn(move || {
            worker_state.drain_events(|_| {
                worker_entered.wait();
                worker_release.wait();
                Ok(())
            })
        });
        entered.wait();
        assert_eq!(state.retry_event_delivery(Duration::ZERO), 1);
        release.wait();
        assert_eq!(worker.join().unwrap(), Ok(true));
        assert_eq!(state.retry_event_delivery(Duration::ZERO), 0);
    }

    #[test]
    fn replay_uses_the_pending_order_without_duplicates() {
        let state = AppState::new(1);
        let snapshot = state.event(
            Generation(1),
            Event::LibrarySnapshot {
                snapshot: state.library(),
            },
        );
        let shutdown = state.event(
            Generation(0),
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    version: CONTRACT_VERSION,
                    state: ShutdownState::Complete,
                    reason: crate::contracts::events::ShutdownReason::System,
                },
            },
        );
        state.publish(snapshot.clone());
        state.publish(shutdown.clone());
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink_delivered = delivered.clone();
        state.set_event_sink(StdArc::new(move |event| {
            sink_delivered.lock().unwrap().push(event.event_id);
            Ok(())
        }));

        state.replay_latest();

        assert_eq!(
            *delivered.lock().unwrap(),
            vec![snapshot.event_id.clone(), shutdown.event_id.clone()]
        );
        assert!(state.pending_events().is_empty());
        state.replay_latest();
        assert_eq!(
            *delivered.lock().unwrap(),
            vec![snapshot.event_id, shutdown.event_id]
        );
    }

    #[test]
    fn publication_queues_new_events_behind_a_failed_delivery() {
        let state = AppState::new(1);
        let attempts = std::sync::Arc::new(Mutex::new(Vec::new()));
        let fail_first = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let sink_attempts = attempts.clone();
        let sink_fail_first = fail_first.clone();
        state.set_event_sink(StdArc::new(move |event| {
            sink_attempts.lock().unwrap().push(event.event_id.clone());
            if event.event_id == "event-1"
                && sink_fail_first.swap(false, std::sync::atomic::Ordering::AcqRel)
            {
                Err("offline".into())
            } else {
                Ok(())
            }
        }));
        let first = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        let second = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        );

        state.publish(first);
        state.publish(second);

        assert_eq!(*attempts.lock().unwrap(), vec!["event-1"]);
        assert_eq!(state.retry_event_delivery(Duration::ZERO), 0);
        assert_eq!(
            *attempts.lock().unwrap(),
            vec!["event-1", "event-1", "event-2", "delivery-error-1"]
        );
    }

    #[test]
    fn panicking_delivery_resets_the_drainer() {
        let state = AppState::new(1);
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        assert!(state.drain_events(|_| panic!("sink panic")).is_err());
        assert_eq!(state.drain_events(|_| Ok(())), Ok(true));
    }

    #[test]
    fn panicking_batch_retries_only_undelivered_suffix() {
        let state = AppState::new(1);
        for index in 0..3 {
            state.publish(state.event(
                Generation(1),
                Event::LibraryRootStatus {
                    root_index: index,
                    status: RootStatus::Scanned,
                },
            ));
        }
        let delivered = std::sync::Arc::new(Mutex::new(Vec::new()));
        let seen = delivered.clone();
        assert!(state
            .drain_events(|event| {
                seen.lock().unwrap().push(event.event_id.clone());
                if event.event_id == "event-2" {
                    panic!("sink panic");
                }
                Ok(())
            })
            .is_err());
        assert_eq!(*delivered.lock().unwrap(), vec!["event-1", "event-2"]);
        assert_eq!(
            state
                .pending_events()
                .iter()
                .map(|event| event.event_id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-2", "event-3"]
        );
    }

    #[test]
    fn discarded_event_id_does_not_block_later_publication() {
        let state = AppState::new(1);
        let orphan = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        let later = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        );
        state.discard_event(&orphan.event_id);
        state.publish(later);
        assert_eq!(
            state
                .pending_events()
                .iter()
                .map(|event| event.event_id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-2"]
        );
    }

    #[test]
    fn discarding_unpublished_event_removes_its_buffered_envelope() {
        let state = AppState::new(1);
        let first = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        let second = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        );
        state.publish(second.clone());
        state.discard_event(&second.event_id);
        state.publish(first);
        assert_eq!(
            state
                .pending_events()
                .iter()
                .map(|event| event.event_id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-1"]
        );
    }

    #[test]
    fn discard_rejects_future_and_repeated_ids() {
        let state = AppState::new(1);
        state.discard_event("event-99");
        let event = state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        );
        state.discard_event("event-99");
        state.publish(event);
        assert_eq!(
            state
                .pending_events()
                .iter()
                .map(|event| event.event_id.as_str())
                .collect::<Vec<_>>(),
            vec!["event-1"]
        );
    }

    #[test]
    fn delivery_error_ids_are_separate_and_monotonic() {
        let state = AppState::new(1);
        state.set_event_sink(StdArc::new(|_| Err("offline".into())));
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        let first_ids: Vec<_> = state
            .events()
            .into_iter()
            .map(|event| event.event_id)
            .collect();
        assert_eq!(first_ids, vec!["event-1", "delivery-error-1"]);
        state.acknowledge_event("delivery-error-1");
        assert!(state
            .events()
            .iter()
            .all(|event| event.event_id != "delivery-error-1"));
        state.acknowledge_event("event-1");
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 1,
                status: RootStatus::Scanned,
            },
        ));
        let errors: Vec<_> = state
            .events()
            .into_iter()
            .filter(|event| matches!(event.event, Event::Error { .. }))
            .map(|event| event.event_id)
            .collect();
        assert_eq!(errors, vec!["delivery-error-2"]);
    }

    #[test]
    fn delivery_error_counter_exhaustion_reports_without_unsafe_id() {
        let state = AppState::new(1);
        state
            .next_delivery_error
            .store(MAX_SAFE_EVENT_ID, Ordering::Relaxed);
        state.set_event_sink(StdArc::new(|_| Err("offline".into())));
        state.publish(state.event(
            Generation(1),
            Event::LibraryRootStatus {
                root_index: 0,
                status: RootStatus::Scanned,
            },
        ));
        assert!(state.delivery_failure().is_some());
        assert!(state
            .events()
            .iter()
            .all(|event| !event.event_id.starts_with("delivery-error-")));
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
    next_delivery_error: Arc<AtomicU64>,
    next_publish: Arc<AtomicU64>,
    next_generation: Arc<AtomicU64>,
    allocated_events: Arc<Mutex<BTreeSet<u64>>>,
    event_sink:
        Arc<Mutex<Option<StdArc<dyn Fn(EventEnvelope) -> Result<(), String> + Send + Sync>>>>,
    publication: Arc<Mutex<()>>,
    unpublished: Arc<Mutex<BTreeMap<u64, EventEnvelope>>>,
    skipped_publications: Arc<Mutex<BTreeSet<u64>>>,
    admission: Arc<Mutex<()>>,
    latest_snapshot: Arc<Mutex<Option<EventEnvelope>>>,
    latest_control: Arc<Mutex<Option<EventEnvelope>>>,
    replayed_events: Arc<Mutex<BTreeSet<String>>>,
    event_retry_stopped: Arc<AtomicBool>,
    shutdown_result: Arc<Mutex<Option<ShutdownReport>>>,
    delivery_failure: Arc<Mutex<Option<ContractError>>>,
    supervisor: Arc<Supervisor>,
    downloads: Arc<DownloadCoordinator>,
    download_requests: Arc<Mutex<BTreeMap<String, (PathBuf, PathBuf)>>>,
    provider_metadata: Arc<Mutex<BTreeMap<PathBuf, TrackMetadata>>>,
    track_paths: Arc<Mutex<BTreeMap<TrackId, PathBuf>>>,
    current_track: Arc<Mutex<Option<TrackId>>>,
    playback: Arc<Mutex<QueueState>>,
    clock: Arc<Mutex<Option<PlaybackClock>>>,
    recovery: Arc<Mutex<FailureRecovery>>,
    favorites: Arc<Mutex<Favorites>>,
    recent: Arc<Mutex<Vec<TrackId>>>,
    scan: Arc<Mutex<ScanState>>,
    #[cfg(target_os = "macos")]
    playback_status: Arc<Mutex<crate::contracts::types::PlaybackStatus>>,
    #[cfg(target_os = "macos")]
    audio: Arc<Mutex<Box<dyn AudioBackend + Send>>>,
}

type ScanWork = Arc<dyn Fn(&[PathBuf], CancellationToken) -> ScanOutcome + Send + Sync>;

struct ScanState {
    active: bool,
    pending: Option<ScanWork>,
}

impl AppState {
    pub fn new(task_limit: usize) -> Self {
        Self {
            tasks: Arc::new(TaskRegistry::new(task_limit)),
            migration_status: Arc::new(Mutex::new(MigrationStatus::NotRun)),
            library: Arc::new(Mutex::new(LibrarySnapshot {
                version: CONTRACT_VERSION,
                generation: Generation(0),
                tracks: Vec::new(),
                playlists: Vec::new(),
            })),
            roots: Arc::new(Mutex::new(Vec::new())),
            root_statuses: Arc::new(Mutex::new(Vec::new())),
            events: Arc::new(Mutex::new(Vec::new())),
            event_draining: Arc::new(Mutex::new(false)),
            event_in_flight: Arc::new(Mutex::new(0)),
            next_event: Arc::new(AtomicU64::new(0)),
            next_delivery_error: Arc::new(AtomicU64::new(0)),
            next_publish: Arc::new(AtomicU64::new(1)),
            next_generation: Arc::new(AtomicU64::new(0)),
            allocated_events: Arc::new(Mutex::new(BTreeSet::new())),
            event_sink: Arc::new(Mutex::new(None)),
            publication: Arc::new(Mutex::new(())),
            unpublished: Arc::new(Mutex::new(BTreeMap::new())),
            skipped_publications: Arc::new(Mutex::new(BTreeSet::new())),
            admission: Arc::new(Mutex::new(())),
            latest_snapshot: Arc::new(Mutex::new(None)),
            latest_control: Arc::new(Mutex::new(None)),
            replayed_events: Arc::new(Mutex::new(BTreeSet::new())),
            event_retry_stopped: Arc::new(AtomicBool::new(false)),
            shutdown_result: Arc::new(Mutex::new(None)),
            delivery_failure: Arc::new(Mutex::new(None)),
            supervisor: Arc::new(Supervisor::new(ProcessRegistry::new())),
            downloads: Arc::new(DownloadCoordinator::new()),
            download_requests: Arc::new(Mutex::new(BTreeMap::new())),
            provider_metadata: Arc::new(Mutex::new(BTreeMap::new())),
            track_paths: Arc::new(Mutex::new(BTreeMap::new())),
            current_track: Arc::new(Mutex::new(None)),
            playback: Arc::new(Mutex::new(QueueState::new())),
            clock: Arc::new(Mutex::new(None)),
            recovery: Arc::new(Mutex::new(FailureRecovery::new())),
            favorites: Arc::new(Mutex::new(Favorites::default())),
            recent: Arc::new(Mutex::new(Vec::new())),
            scan: Arc::new(Mutex::new(ScanState {
                active: false,
                pending: None,
            })),
            #[cfg(target_os = "macos")]
            playback_status: Arc::new(Mutex::new(crate::contracts::types::PlaybackStatus::Stopped)),
            #[cfg(target_os = "macos")]
            audio: Arc::new(Mutex::new(Box::new(CpalSymphoniaBackend::new()))),
        }
    }
    pub fn library(&self) -> LibrarySnapshot {
        self.library.lock().unwrap().clone()
    }
    pub fn root_statuses(&self) -> Vec<RootStatus> {
        self.root_statuses.lock().unwrap().clone()
    }
    pub fn set_scan_roots(&self, roots: Vec<PathBuf>) {
        *self.roots.lock().unwrap() = roots;
    }
    pub fn scan_roots(&self) -> Vec<PathBuf> {
        self.roots.lock().unwrap().clone()
    }
    pub fn favorite_paths(&self) -> Vec<String> {
        self.favorites
            .lock()
            .unwrap()
            .paths()
            .map(str::to_owned)
            .collect()
    }
    pub fn set_favorite_path(&self, path: impl AsRef<std::path::Path>) {
        self.favorites.lock().unwrap().set(path, true);
    }
    pub fn prune_favorites(&self) {
        let paths = self
            .track_paths
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        self.favorites.lock().unwrap().project(paths);
    }
    pub fn pending_events(&self) -> Vec<EventEnvelope> {
        self.events.lock().unwrap().clone()
    }
    /// Test and integration accessor for the buffered event queue.
    pub fn events(&self) -> Vec<EventEnvelope> {
        self.pending_events()
    }
    pub fn acknowledge_event(&self, event_id: &str) {
        self.events
            .lock()
            .unwrap()
            .retain(|event| event.event_id != event_id);
    }
    pub fn delivery_failure(&self) -> Option<ContractError> {
        self.delivery_failure.lock().unwrap().clone()
    }
    /// Drain is single-consumer: events are claimed while holding the queue lock and
    /// failed deliveries are returned atomically instead of being duplicated.
    pub fn drain_events<F>(&self, deliver: F) -> Result<bool, String>
    where
        F: Fn(&EventEnvelope) -> Result<(), String>,
    {
        {
            let mut draining = self.event_draining.lock().unwrap();
            if *draining {
                return Ok(false);
            }
            *draining = true;
        }
        struct DrainGuard<'a> {
            draining: &'a Mutex<bool>,
            in_flight: &'a Mutex<usize>,
        }
        impl Drop for DrainGuard<'_> {
            fn drop(&mut self) {
                *self.in_flight.lock().unwrap() = 0;
                *self.draining.lock().unwrap() = false;
            }
        }
        let _guard = DrainGuard {
            draining: &self.event_draining,
            in_flight: &self.event_in_flight,
        };
        let _publication = self.publication.lock().unwrap();
        let claimed = { std::mem::take(&mut *self.events.lock().unwrap()) };
        *self.event_in_flight.lock().unwrap() = claimed.len();
        let mut failed_at = None;
        let mut error = None;
        for (index, event) in claimed.iter().enumerate() {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| deliver(event)));
            match outcome {
                Ok(Ok(())) => {}
                Ok(Err(message)) => {
                    failed_at = Some(index);
                    error = Some(message);
                    break;
                }
                Err(_) => {
                    failed_at = Some(index);
                    error = Some("event delivery panicked".into());
                    break;
                }
            }
        }
        let failed = failed_at.map_or_else(Vec::new, |index| claimed[index..].to_vec());
        if !failed.is_empty() {
            self.events.lock().unwrap().splice(0..0, failed);
        }
        error.map_or(Ok(true), Err)
    }
    pub fn event(&self, generation: Generation, event: Event) -> EventEnvelope {
        let _publication = self.publication.lock().unwrap();
        self.event_locked(generation, event)
    }

    /// Releases an allocated ID when an event is abandoned before publication.
    pub fn discard_event(&self, event_id: &str) {
        let Some(id) = event_id
            .strip_prefix("event-")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return;
        };
        let _publication = self.publication.lock().unwrap();
        if self.allocated_events.lock().unwrap().remove(&id) {
            self.unpublished.lock().unwrap().remove(&id);
            self.skipped_publications.lock().unwrap().insert(id);
            self.publish_ready_locked();
        }
    }

    fn event_locked(&self, generation: Generation, event: Event) -> EventEnvelope {
        let id = self
            .next_event
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current
                    .checked_add(1)
                    .filter(|next| *next <= MAX_SAFE_EVENT_ID)
            })
            .expect("event ID space exhausted")
            + 1;
        self.allocated_events.lock().unwrap().insert(id);
        EventEnvelope {
            version: CONTRACT_VERSION,
            event_id: format!("event-{id}"),
            generation,
            job_id: None,
            session_id: None,
            event,
        }
    }
    pub fn set_event_sink(
        &self,
        sink: StdArc<dyn Fn(EventEnvelope) -> Result<(), String> + Send + Sync>,
    ) {
        *self.event_sink.lock().unwrap() = Some(sink);
        self.replayed_events.lock().unwrap().clear();
    }
    /// Replay the latest state/control envelopes after a listener has started.
    pub fn replay_latest(&self) {
        let Some(sink) = self.event_sink.lock().unwrap().clone() else {
            return;
        };
        {
            let _publication = self.publication.lock().unwrap();
            for envelope in [
                self.latest_snapshot.lock().unwrap().clone(),
                self.latest_control.lock().unwrap().clone(),
            ]
            .into_iter()
            .flatten()
            {
                let mut events = self.events.lock().unwrap();
                let mut replayed = self.replayed_events.lock().unwrap();
                if !events
                    .iter()
                    .any(|pending| pending.event_id == envelope.event_id)
                    && !replayed.contains(&envelope.event_id)
                {
                    let event_id = envelope.event_id.clone();
                    events.push(envelope);
                    events.sort_by_key(|event| {
                        event
                            .event_id
                            .strip_prefix("event-")
                            .and_then(|id| id.parse::<u64>().ok())
                            .unwrap_or(u64::MAX)
                    });
                    replayed.insert(event_id);
                } else if events
                    .iter()
                    .any(|pending| pending.event_id == envelope.event_id)
                {
                    replayed.insert(envelope.event_id.clone());
                }
            }
        }
        let _ = self.drain_events(|event| sink(event.clone()));
    }
    pub fn stop_event_delivery(&self) {
        self.event_retry_stopped.store(true, Ordering::Release);
    }
    pub fn event_delivery_stopped(&self) -> bool {
        self.event_retry_stopped.load(Ordering::Acquire)
    }
    pub fn set_provider_metadata(&self, path: PathBuf, metadata: TrackMetadata) {
        self.provider_metadata
            .lock()
            .unwrap()
            .insert(path, metadata);
    }
    pub fn publish(&self, envelope: EventEnvelope) {
        let _publication = self.publication.lock().unwrap();
        let id = envelope
            .event_id
            .strip_prefix("event-")
            .and_then(|value| value.parse::<u64>().ok())
            .expect("internal event IDs are numeric");
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
            let Some(envelope) = self.unpublished.lock().unwrap().remove(&next) else {
                break;
            };
            self.allocated_events.lock().unwrap().remove(&next);
            self.next_publish.fetch_add(1, Ordering::Release);
            self.publish_one_locked(envelope);
        }
    }

    fn publish_one_locked(&self, envelope: EventEnvelope) {
        if matches!(&envelope.event, Event::LibrarySnapshot { .. }) {
            *self.latest_snapshot.lock().unwrap() = Some(envelope.clone());
        }
        if matches!(&envelope.event, Event::Shutdown { .. }) {
            *self.latest_control.lock().unwrap() = Some(envelope.clone());
        }
        let has_delivery_work = !self.events.lock().unwrap().is_empty()
            || *self.event_in_flight.lock().unwrap() > 0
            || *self.event_draining.lock().unwrap();
        if !has_delivery_work {
            if let Some(sink) = self.event_sink.lock().unwrap().clone() {
                let delivery = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    sink(envelope.clone())
                }));
                if !matches!(delivery, Ok(Ok(()))) {
                    self.queue_event_locked(envelope);
                    let failure = ContractError {
                        code: ErrorCode::Internal,
                        message: "backend event delivery failed".into(),
                        retryable: true,
                    };
                    if let Ok(failure_id) = self.next_delivery_error.fetch_update(
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                        |current| {
                            current
                                .checked_add(1)
                                .filter(|next| *next <= MAX_SAFE_EVENT_ID)
                        },
                    ) {
                        self.queue_event_locked(EventEnvelope {
                            version: CONTRACT_VERSION,
                            event_id: format!("delivery-error-{}", failure_id + 1),
                            generation: Generation(0),
                            job_id: None,
                            session_id: None,
                            event: Event::Error { error: failure },
                        });
                    } else {
                        *self.delivery_failure.lock().unwrap() = Some(failure);
                    }
                }
            } else {
                self.queue_event_locked(envelope);
            }
        } else {
            self.queue_event_locked(envelope);
        }
    }

    fn queue_event_locked(&self, envelope: EventEnvelope) {
        let mut events = self.events.lock().unwrap();
        events.push(envelope);
        events.sort_by_key(|event| {
            event
                .event_id
                .strip_prefix("event-")
                .and_then(|id| id.parse::<u64>().ok())
                .unwrap_or(u64::MAX)
        });
    }
    pub fn next_generation(&self) -> Generation {
        Generation(self.next_generation.fetch_add(1, Ordering::Relaxed) + 1)
    }
    pub fn scan_async(&self) -> Result<Generation, SpawnError> {
        self.scan_async_with(|roots, token| {
            if token.is_cancelled() {
                return ScanOutcome {
                    playlists: BTreeMap::new(),
                    roots: Vec::new(),
                    errors: Vec::new(),
                };
            }
            crate::library::scanner::scan_library_with_cancel(roots, &OsFileSystem, || token.is_cancelled())
        })
    }

    /// Schedule a real task-registry scan while keeping refresh requests
    /// deterministic: one active scan and one deferred follow-up.
    pub fn scan_async_with<F>(&self, work: F) -> Result<Generation, SpawnError>
    where
        F: Fn(&[PathBuf], CancellationToken) -> ScanOutcome + Send + Sync + 'static,
    {
        let work = Arc::new(work);
        {
            let mut scan = self.scan.lock().unwrap();
            if scan.active {
                scan.pending = Some(work);
                return Ok(Generation(self.next_generation.load(Ordering::Acquire)));
            }
            scan.active = true;
        }
        self.start_scan(work)
    }

    fn start_scan(&self, work: ScanWork) -> Result<Generation, SpawnError> {
        let generation = self.next_generation();
        let spec = self.generation("library-scan", generation);
        let state = self.clone();
        let roots = self.roots.lock().unwrap().clone();
        let result = self.submit(spec.clone(), move |token| {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let outcome = work(&roots, token.clone());
                if !token.is_cancelled() {
                    state.publish_scan_result_if_current(&spec, generation, outcome);
                }
            }));
            match outcome {
                Ok(()) => state.finish_scan(token.is_cancelled() || state.is_closing()),
                Err(panic) => {
                    state.finish_scan(true);
                    std::panic::resume_unwind(panic);
                }
            }
        });
        if result.is_err() {
            let mut scan = self.scan.lock().unwrap();
            scan.active = false;
            scan.pending = None;
        }
        result.map(|_| generation)
    }

    fn finish_scan(&self, discard_pending: bool) {
        let pending = {
            let mut scan = self.scan.lock().unwrap();
            if discard_pending || self.is_closing() {
                scan.active = false;
                scan.pending = None;
                None
            } else {
                scan.pending.take().or_else(|| {
                    scan.active = false;
                    None
                })
            }
        };
        if let Some(work) = pending {
            if self.start_scan(work).is_err() {
                let mut scan = self.scan.lock().unwrap();
                scan.active = false;
                scan.pending = None;
            }
        }
    }

    pub fn publish_scan_result_if_current(
        &self,
        spec: &TaskSpec,
        generation: Generation,
        outcome: ScanOutcome,
    ) -> bool {
        if spec.generation != generation || !self.publish_allowed(spec) {
            return false;
        }
        self.publish_scan_outcome(generation, outcome);
        true
    }

    fn publish_scan_outcome(&self, generation: Generation, outcome: ScanOutcome) {
        let statuses: Vec<_> = outcome
            .roots
            .iter()
            .map(|(_, status)| match status {
                crate::library::scanner::RootStatus::Scanned => RootStatus::Scanned,
                crate::library::scanner::RootStatus::Missing => RootStatus::Missing,
                crate::library::scanner::RootStatus::Inaccessible => RootStatus::Inaccessible,
            })
            .collect();
        *self.root_statuses.lock().unwrap() = statuses.clone();
        for (root_index, status) in statuses.iter().copied().enumerate() {
            let mut event = self.event(generation, Event::LibraryRootStatus { root_index, status });
            event.job_id = Some(JobId("library-scan".into()));
            self.publish(event);
            if !matches!(status, RootStatus::Scanned) {
                let mut error = self.event(
                    generation,
                    Event::Error {
                        error: ContractError {
                            code: ErrorCode::Internal,
                            message: "library root could not be scanned".into(),
                            retryable: true,
                        },
                    },
                );
                error.job_id = Some(JobId("library-scan".into()));
                self.publish(error);
            }
        }
        for issue in outcome.errors {
            let scope = issue
                .playlist
                .map_or_else(|| "root".into(), |playlist| format!("playlist {playlist}"));
            let root_path = outcome
                .roots
                .get(issue.root_index)
                .map(|(path, _)| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            let detail = issue.message.replace(&root_path, "<root>");
            let mut error = self.event(
                generation,
                Event::Error {
                    error: ContractError {
                        code: ErrorCode::Internal,
                        message: format!(
                            "library scan failed for root {root_path} ({scope}): {detail}"
                        ),
                        retryable: true,
                    },
                },
            );
            error.job_id = Some(JobId("library-scan".into()));
            self.publish(error);
        }
        let snapshot = self.replace_library(generation, outcome.playlists);
        let mut event = self.event(generation, Event::LibrarySnapshot { snapshot });
        event.job_id = Some(JobId("library-scan".into()));
        self.publish(event);
    }
    pub fn replace_library(
        &self,
        generation: Generation,
        playlists: BTreeMap<String, Vec<PathBuf>>,
    ) -> LibrarySnapshot {
        self.track_paths.lock().unwrap().clear();
        let mut paths = BTreeMap::<PathBuf, TrackId>::new();
        let mut tracks = Vec::new();
        let mut playlist_data = Vec::new();
        for (name, files) in playlists {
            let playlist_id = crate::contracts::types::PlaylistId(format!("playlist:{}", name));
            let mut ids = Vec::new();
            for path in files {
                let id = paths
                    .entry(path.clone())
                    .or_insert_with(|| {
                        let metadata = self
                            .provider_metadata
                            .lock()
                            .unwrap()
                            .get(&path)
                            .cloned()
                            .unwrap_or_else(|| {
                                crate::domain::identity::filename_metadata(&path.to_string_lossy())
                            });
                        let id = TrackId(crate::domain::identity::track_id(&metadata));
                        self.track_paths
                            .lock()
                            .unwrap()
                            .insert(id.clone(), path.clone());
                        let favorite = self.favorites.lock().unwrap().contains(&path);
                        tracks.push(Track {
                            id: id.clone(),
                            path: None,
                            title: metadata.title,
                            artist: (!metadata.artist.is_empty()).then_some(metadata.artist),
                            album: None,
                            duration_ms: (metadata.duration_ms != 0)
                                .then_some(metadata.duration_ms),
                            artwork_url: None,
                            favorite,
                            provider: metadata.provider.map(|provider| {
                                crate::contracts::types::ProviderId {
                                    provider: provider.provider,
                                    id: provider.id,
                                }
                            }),
                        });
                        id
                    })
                    .clone();
                ids.push(id);
            }
            playlist_data.push(Playlist {
                id: playlist_id,
                name: name.clone(),
                path: None,
                url: None,
                ordered_track_ids: ids,
            });
        }
        let snapshot = LibrarySnapshot {
            version: CONTRACT_VERSION,
            generation,
            tracks,
            playlists: playlist_data,
        };
        *self.library.lock().unwrap() = snapshot.clone();
        let scanned = self
            .track_paths
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        self.favorites.lock().unwrap().project(scanned);
        snapshot
    }
    pub fn submit<T, F>(&self, spec: TaskSpec, work: F) -> Result<TaskHandle<T>, SpawnError>
    where
        T: Send + 'static,
        F: FnOnce(CancellationToken) -> T + Send + 'static,
    {
        self.tasks.spawn(spec, work)
    }
    pub fn generation(&self, key: impl Into<String>, generation: Generation) -> TaskSpec {
        TaskSpec::new(key, generation, None)
    }
    pub fn session_task(
        &self,
        key: impl Into<String>,
        generation: Generation,
        session: SessionId,
    ) -> TaskSpec {
        TaskSpec::new(key, generation, Some(session))
    }
    pub fn publish_allowed(&self, spec: &TaskSpec) -> bool {
        self.tasks.is_current(spec)
    }
    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport {
        self.tasks.shutdown(deadline)
    }
    pub fn is_closing(&self) -> bool {
        self.tasks.is_closing()
    }

    pub fn dispatch(&self, envelope: CommandEnvelope) -> Result<Value, ContractError> {
        if envelope.request_id.trim().is_empty() {
            return Err(ContractError::invalid("request_id must not be empty"));
        }
        let admission = self.admission.lock().unwrap();
        if !matches!(&envelope.command, Command::Shutdown { .. }) && self.is_closing() {
            return Err(ContractError {
                code: ErrorCode::ShuttingDown,
                message: "backend is shutting down".into(),
                retryable: false,
            });
        }
        match envelope.command {
            Command::GetLibrary => {
                let snapshot = self.library();
                self.publish(self.event(
                    snapshot.generation,
                    Event::LibrarySnapshot {
                        snapshot: snapshot.clone(),
                    },
                ));
                serde_json::to_value(snapshot).map_err(|e| ContractError {
                    code: ErrorCode::Internal,
                    message: e.to_string(),
                    retryable: false,
                })
            }
            Command::ScanLibrary => {
                let generation = self.scan_async().map_err(|e| match e {
                    SpawnError::AtCapacity => ContractError {
                        code: ErrorCode::Busy,
                        message: "task capacity is full".into(),
                        retryable: true,
                    },
                    SpawnError::ShuttingDown => ContractError {
                        code: ErrorCode::ShuttingDown,
                        message: "backend is shutting down".into(),
                        retryable: false,
                    },
                })?;
                Ok(serde_json::json!({ "generation": generation.0, "job_id": "library-scan" }))
            }
            Command::LoadConfig { home } => self.load_config(PathBuf::from(home)),
            Command::RegisterRoot { root } => {
                let root = PathBuf::from(root);
                if !root.is_absolute() || root.to_string_lossy().chars().any(char::is_control) {
                    return Err(ContractError::invalid("library root must be an absolute directory"));
                }
                self.set_scan_roots(vec![root.clone()]);
                Ok(serde_json::json!({ "root": root.display().to_string() }))
            }
            Command::PromoteAudioOutput { temporary, destination } => {
                self.promote_audio_output(
                    PathBuf::from(temporary),
                    PathBuf::from(destination),
                )
            }
            Command::SyncMarkers { folder } => {
                let folder = validate_library_path(self, &PathBuf::from(&folder), false)?;
                let markers = Markers::load_checked(folder).map_err(|message| ContractError {
                    code: if message.contains("missing") {
                        ErrorCode::NotFound
                    } else {
                        ErrorCode::InvalidRequest
                    },
                    message,
                    retryable: false,
                })?;
                Ok(serde_json::json!({
                    "successful": markers.successful.len(),
                    "permanent_failures": markers.failed.len()
                }))
            }
            Command::SearchDownloader { query, try_soundcloud } => {
                let sources = crate::download::workflows::search_sources(&query, try_soundcloud)
                    .map_err(|error| ContractError::invalid(format!("invalid downloader search: {error:?}")))?;
                Ok(serde_json::to_value(sources.into_iter().map(|(provider, query)| {
                    serde_json::json!({ "provider": format!("{provider:?}").to_lowercase(), "query": query })
                }).collect::<Vec<_>>()).unwrap_or(Value::Null))
            }
            Command::ValidateDirectUrl { url, destination } => {
                crate::download::workflows::validate_http_url(&url)
                    .map_err(|_| ContractError::invalid("URL must use http or https"))?;
                let destination = PathBuf::from(destination);
                if !destination.is_absolute() {
                    return Err(ContractError::invalid("destination must be an absolute path"));
                }
                validate_library_path(self, &destination, true)?;
                Ok(serde_json::json!({ "url": url.trim(), "destination": destination.display().to_string() }))
            }
            Command::PreviewCsv { csv } => {
                let tracks = crate::download::csv::parse(csv.as_bytes())
                    .map_err(|error| ContractError::invalid(format!("invalid CSV: {error:?}")))?;
                Ok(serde_json::json!({ "rows": tracks.iter().enumerate().map(|(index, track)| serde_json::json!({
                    "id": format!("csv:{}", index + 1), "title": track.title, "artist": track.artist,
                    "album": track.album, "url": track.url
                })).collect::<Vec<_>>() }))
            }
            Command::QueueUrls { urls } => {
                let mut accepted = Vec::new();
                let mut rejected = Vec::new();
                for url in urls {
                    match crate::download::workflows::validate_http_url(&url) {
                        Ok(url) => accepted.push(url.to_owned()),
                        Err(_) => rejected.push(serde_json::json!({ "url": url.trim(), "reason": "invalid HTTP/HTTPS URL" })),
                    }
                }
                Ok(serde_json::json!({ "accepted": accepted, "rejected": rejected }))
            }
            Command::SelectLibraryRoot { root } => {
                let root = PathBuf::from(root);
                if !root.is_absolute() || root.to_string_lossy().chars().any(char::is_control) { return Err(ContractError::invalid("library root must be an absolute directory")); }
                if !root.is_dir() { return Err(ContractError { code: ErrorCode::NotFound, message: "library root does not exist".into(), retryable: false }); }
                let root = std::fs::canonicalize(root).map_err(|_| ContractError::invalid("library root could not be canonicalized"))?;
                self.set_scan_roots(vec![root.clone()]);
                Ok(serde_json::json!({ "root": root.display().to_string() }))
            }
            Command::StartDownload { job_id, destination, helper } => self.start_download(job_id, PathBuf::from(destination), PathBuf::from(helper)),
            Command::RetryDownload { job_id } => {
                let (destination, helper) = self.download_requests.lock().unwrap().get(&job_id.0).cloned().ok_or_else(|| ContractError { code: ErrorCode::NotFound, message: "download request not found".into(), retryable: false })?;
                self.start_download(job_id, destination, helper)
            }
            Command::LoadSettings { home } => self.load_config(PathBuf::from(home)),
            Command::SaveSettings { home, settings } => self.save_settings(PathBuf::from(home), settings),
            Command::TransferPreview { manifest, destination } => self.transfer_preview(&manifest, PathBuf::from(destination)),
            Command::TransferExport => self.transfer_export(),
            Command::TransferApply { manifest, destination, approvals } => self.transfer_apply(&manifest, PathBuf::from(destination), approvals),
            Command::ToggleFavorite { track_id } => self.toggle_favorite(track_id),
            Command::RecordRecent { track_id } => {
                if !self.track_paths.lock().unwrap().contains_key(&track_id) {
                    return Err(ContractError {
                        code: ErrorCode::NotFound,
                        message: "track is not available locally".into(),
                        retryable: false,
                    });
                }
                self.record_recent(track_id);
                Ok(Value::Null)
            }
            Command::GetRecent => serde_json::to_value(self.recent.lock().unwrap().clone())
                .map_err(|_| ContractError {
                    code: ErrorCode::Internal,
                    message: "recent history unavailable".into(),
                    retryable: false,
                }),
            Command::ClearRecent => {
                self.recent.lock().unwrap().clear();
                Ok(Value::Null)
            }
            Command::PlayNext { track_ids } => self.queue_insert(track_ids, true),
            Command::AddToQueue { track_ids } => self.queue_insert(track_ids, false),
            Command::RemoveFromQueue { index } => self.queue_remove(index),
            Command::ReorderQueue { from, to } => self.queue_reorder(from, to),
            Command::ClearQueue => {
                self.playback.lock().unwrap().clear();
                #[cfg(target_os = "macos")]
                {
                    if let Ok(mut backend) = self.audio.lock() { backend.stop(); }
                    if let Some(clock) = self.clock.lock().unwrap().as_mut() { clock.stop(); }
                    *self.playback_status.lock().unwrap() = crate::contracts::types::PlaybackStatus::Stopped;
                    *self.current_track.lock().unwrap() = None;
                }
                self.publish_queue_snapshot();
                Ok(Value::Null)
            }
            Command::ToggleShuffle => {
                self.playback.lock().unwrap().toggle_shuffle();
                self.publish_queue_snapshot();
                Ok(Value::Null)
            }
            Command::SetRepeat { repeat } => {
                self.playback.lock().unwrap().set_repeat(match repeat {
                    ContractRepeatMode::Off => crate::playback::queue::RepeatMode::Off,
                    ContractRepeatMode::One => crate::playback::queue::RepeatMode::One,
                    ContractRepeatMode::All => crate::playback::queue::RepeatMode::All,
                });
                self.publish_queue_snapshot();
                Ok(Value::Null)
            }
            Command::CancelJob { job_id } => {
                if self.downloads.cancel_job(&job_id.0) {
                    Ok(Value::Null)
                } else if self.tasks.cancel_key(&job_id.0) {
                    Ok(Value::Null)
                } else {
                    Err(ContractError {
                        code: ErrorCode::NotFound,
                        message: "job not found".into(),
                        retryable: false,
                    })
                }
            }
            #[cfg(not(target_os = "macos"))]
            Command::Play { .. } | Command::PlayWithContext { .. } | Command::Next | Command::Previous | Command::Pause | Command::Seek { .. } => Err(ContractError {
                code: ErrorCode::Unavailable,
                message: "audio backend unavailable on this platform".into(),
                retryable: false,
            }),
            #[cfg(target_os = "macos")]
            Command::Play { track_id } => self.play_track(track_id),
            #[cfg(target_os = "macos")]
            Command::PlayWithContext { track_id, context } => self.play_with_context(track_id, context),
            #[cfg(target_os = "macos")]
            Command::Next => self.next_track(),
            #[cfg(target_os = "macos")]
            Command::Previous => self.previous_track(),
            #[cfg(target_os = "macos")]
            Command::Pause => self.pause_track(),
            #[cfg(target_os = "macos")]
            Command::Seek { position_ms } => self.seek_track(position_ms),
            Command::VideoSource { .. }
            | Command::VideoStage { .. }
            | Command::VideoPlay
            | Command::VideoPause
            | Command::VideoSeek { .. }
            | Command::VideoResize { .. }
            | Command::VideoFullscreen { .. } => Err(ContractError {
                code: ErrorCode::Unavailable,
                message: "video backend unavailable".into(),
                retryable: false,
            }),
            Command::Shutdown { reason } => {
                let report =
                    self.shutdown_events_locked(reason, Duration::from_millis(500), &admission);
                if report.failed {
                    Err(ContractError {
                        code: ErrorCode::Internal,
                        message: "shutdown failed because a worker panicked".into(),
                        retryable: false,
                    })
                } else if report.timed_out {
                    Err(ContractError {
                        code: ErrorCode::Internal,
                        message: "shutdown timed out; tasks or events remain undelivered".into(),
                        retryable: false,
                    })
                } else {
                    Ok(Value::Null)
                }
            }
        }
    }

    fn load_config(&self, home: PathBuf) -> Result<Value, ContractError> {
        validate_home(&home)?;
        let path = home.join(".luno_config.json");
        let config = match crate::persistence::legacy::read_config(&path) {
            Ok(config) => config,
            Err(error) if error.contains("No such file") || error.contains("not found") => {
                return Err(ContractError {
                    code: ErrorCode::NotFound,
                    message: "configuration is missing".into(),
                    retryable: false,
                });
            }
            Err(_) => {
                return Err(ContractError {
                    code: ErrorCode::InvalidRequest,
                    message: "configuration is corrupt".into(),
                    retryable: false,
                });
            }
        };
        let root = config.music_root.map(|value| value.trim().to_owned());
        if root.as_deref().is_some_and(str::is_empty) {
            return Err(ContractError {
                code: ErrorCode::InvalidRequest,
                message: "configuration music root is empty".into(),
                retryable: false,
            });
        }
        let root = root.map(|root| {
            let root = PathBuf::from(root);
            root
        });
        if let Some(root) = &root {
            if !root.is_absolute() || root.to_string_lossy().chars().any(char::is_control) || !root.is_dir() {
                return Err(ContractError::invalid("configuration music root is unsafe or unavailable"));
            }
        }
        self.set_scan_roots(root.clone().into_iter().collect());
        if let Some(paths) = config.favorite_songs {
            let mut favorites = self.favorites.lock().unwrap();
            for path in paths {
                favorites.set(path, true);
            }
        }
        Ok(serde_json::json!({
            "music_root": root.map(|path| path.display().to_string()),
            "auto_discovery": config.auto_discovery,
            "volume": config.volume,
            "sleep_timer": config.sleep_timer,
            "global_keys": config.global_keys,
            "favorite_songs": self.favorites.lock().unwrap().paths().map(str::to_owned).collect::<Vec<_>>(),
        }))
    }

    fn save_settings(&self, home: PathBuf, settings: crate::contracts::types::SettingsInput) -> Result<Value, ContractError> {
        validate_home(&home)?;
        if home.as_os_str().is_empty()
            || settings.favorite_songs.iter().any(|path| path.is_empty() || path.contains('\0') || path.chars().any(char::is_control))
            || settings.volume.is_some_and(|volume| !volume.is_finite() || !(0.0..=1.0).contains(&volume))
        {
            return Err(ContractError::invalid("settings contain an unsafe path"));
        }
        if let Some(root) = settings.music_root.as_deref() {
            let root = PathBuf::from(root);
            if !root.is_absolute() || !root.is_dir() {
                return Err(ContractError::invalid("music root must be an existing absolute directory"));
            }
        }
        let value = serde_json::json!({
            "music_root": settings.music_root,
            "auto_discovery": settings.auto_discovery,
            "volume": settings.volume,
            "sleep_timer": settings.sleep_timer,
            "global_keys": settings.global_keys,
            "favorite_songs": settings.favorite_songs,
        });
        let bytes = serde_json::to_vec_pretty(&value).map_err(|_| ContractError { code: ErrorCode::Internal, message: "settings could not be encoded".into(), retryable: false })?;
        let path = home.join(".luno_config.json");
        crate::persistence::atomic_write::write_atomic(&path, &bytes).map_err(|_| ContractError { code: ErrorCode::Internal, message: "settings could not be persisted".into(), retryable: true })?;
        Ok(serde_json::json!({ "saved": true }))
    }

    fn start_download(&self, job_id: JobId, destination: PathBuf, helper: PathBuf) -> Result<Value, ContractError> {
        let root = self.scan_roots().first().cloned().ok_or_else(|| ContractError::invalid("no configured library root"))?;
        let root = std::fs::canonicalize(root).map_err(|_| ContractError { code: ErrorCode::NotFound, message: "configured library root is unavailable".into(), retryable: false })?;
        let parent = destination.parent().ok_or_else(|| ContractError::invalid("destination must name a file"))?;
        let parent = std::fs::canonicalize(parent).map_err(|_| ContractError::invalid("destination directory is unavailable"))?;
        if !parent.starts_with(&root) || !helper.is_absolute() || !helper.is_file() {
            return Err(ContractError::invalid("download helper or destination is unsafe"));
        }
        self.download_requests.lock().unwrap().insert(job_id.0.clone(), (destination.clone(), helper.clone()));
        let generation = self.next_generation();
        let job = job_id.clone();
        let coordinator = self.downloads.clone();
        let state = self.clone();
        let helper_for_worker = helper.clone();
        let receiver = coordinator.events_for(&job.0);
        let worker_home = root.join(".luno-gate4-worker-home");
        std::fs::create_dir_all(&worker_home).map_err(|_| ContractError::invalid("download worker home is unavailable"))?;
        let start_result = coordinator.start(crate::download::coordinator::DownloadRequest { job_id: job.0.clone(), destination }, Box::new(move |temporary, token, progress| {
            if token.is_cancelled() { return Err("cancelled".into()); }
            let mut child = std::process::Command::new(&helper_for_worker).arg(temporary).env("HOME", &worker_home).spawn().map_err(|_| "download helper could not start".to_string())?;
            loop {
                if token.is_cancelled() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("cancelled".into());
                }
                match child.try_wait().map_err(|_| "download helper failed".to_string())? {
                    Some(status) => { progress(1.0); return if status.success() { Ok(()) } else { Err("download helper failed".into()) }; }
                    None => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        }));
        if let Err(error) = start_result {
            coordinator.remove_events_for(&job.0);
            return Err(ContractError { code: match error { crate::download::coordinator::StartError::Busy => ErrorCode::Busy, crate::download::coordinator::StartError::ShuttingDown => ErrorCode::ShuttingDown, crate::download::coordinator::StartError::InvalidRequest => ErrorCode::InvalidRequest }, message: "download could not start".into(), retryable: matches!(error, crate::download::coordinator::StartError::Busy) });
        }
        let spec = self.generation(format!("download-{}", job.0), generation);
        let worker_job = job.clone();
        self.submit(spec, move |task_token| {
            while !state.is_closing() {
                if task_token.is_cancelled() { break; }
                let Ok(event) = receiver.recv_timeout(Duration::from_millis(50)) else {
                    continue;
                };
                use crate::download::coordinator::DownloadEvent;
                let (state_value, progress, error) = match &event {
                    DownloadEvent::Started { .. } => (crate::contracts::types::JobState::Running, None, None),
                    DownloadEvent::Progress { fraction, .. } => (crate::contracts::types::JobState::Running, Some(*fraction), None),
                    DownloadEvent::Cancelling { .. } => (crate::contracts::types::JobState::Cancelling, None, None),
                    DownloadEvent::Succeeded { .. } => (crate::contracts::types::JobState::Succeeded, Some(1.0), None),
                    DownloadEvent::Failed { error, .. } => {
                        (crate::contracts::types::JobState::Failed, None, Some(ContractError { code: ErrorCode::Unavailable, message: sanitize_download_error(error), retryable: true }))
                    },
                    DownloadEvent::Cancelled { .. } => (crate::contracts::types::JobState::Cancelled, None, None),
                };
                let mut envelope = state.event(generation, Event::JobUpdated { job: crate::contracts::types::DownloadJob { id: worker_job.clone(), kind: crate::contracts::types::DownloadKind::Single, state: state_value, track_id: None, progress, generation } });
                envelope.job_id = Some(worker_job.clone());
                state.publish(envelope);
                if let Some(error) = error { state.publish(state.event(generation, Event::Error { error })); }
                if matches!(event, DownloadEvent::Succeeded { .. } | DownloadEvent::Failed { .. } | DownloadEvent::Cancelled { .. }) { break; }
            }
        }).map_err(|_| ContractError { code: ErrorCode::Internal, message: "download worker could not start".into(), retryable: true })?;
        Ok(serde_json::json!({ "job_id": job.0, "generation": generation.0 }))
    }

    fn transfer_export(&self) -> Result<Value, ContractError> {
        let library = self.library();
        let mut assigned = BTreeSet::new();
        let playlists = library.playlists.iter().map(|playlist| {
            for id in &playlist.ordered_track_ids { assigned.insert(id.clone()); }
            crate::transfer::codec::Playlist { name: playlist.name.clone(), description: String::new(), track_refs: playlist.ordered_track_ids.iter().map(|id| id.0.clone()).collect() }
        }).collect::<Vec<_>>();
        let tracks = library.tracks.iter().map(|track| crate::transfer::codec::Track {
            r#ref: track.id.0.clone(), title: track.title.clone(), artist: track.artist.clone().unwrap_or_default(), album: track.album.clone().unwrap_or_default(), duration_ms: track.duration_ms.unwrap_or_default(), source: track.provider.as_ref().map(|source| crate::transfer::codec::Source { provider: source.provider.clone(), id: source.id.clone() }), ambiguity_confirmation_required: track.provider.is_none(),
        }).collect::<Vec<_>>();
        let unassigned_track_refs = library.tracks.iter().filter(|track| !assigned.contains(&track.id)).map(|track| track.id.0.clone()).collect();
        let manifest = crate::transfer::codec::Manifest { format: crate::transfer::codec::MANIFEST_FORMAT.into(), manifest_version: crate::transfer::codec::MANIFEST_VERSION, scope: "library".into(), created_at: "1970-01-01T00:00:00Z".into(), tracks, playlists, unassigned_track_refs };
        let manifest = crate::transfer::codec::encode(&manifest).map_err(|error| ContractError::invalid(error.to_string()))?;
        Ok(serde_json::json!({ "manifest": manifest }))
    }

    fn transfer_index(&self) -> crate::transfer::provider_index::ProviderIndex {
        let metadata = self.provider_metadata.lock().unwrap();
        crate::transfer::provider_index::ProviderIndex::new(self.track_paths.lock().unwrap().iter().map(|(id, path)| crate::transfer::provider_index::IndexedTrack { path: path.clone(), metadata: metadata.get(path).cloned().unwrap_or_else(|| crate::domain::identity::filename_metadata(&id.0)) }))
    }

    fn transfer_preview(&self, manifest: &str, destination: PathBuf) -> Result<Value, ContractError> {
        self.validate_transfer_destination(&destination)?;
        let manifest = crate::transfer::codec::decode(manifest).map_err(|error| ContractError::invalid(error.to_string()))?;
        let plan = crate::transfer::planner::plan_import(&manifest, &self.transfer_index(), destination);
        let matched = plan.tracks.iter().filter(|track| matches!(track.match_plan, crate::transfer::planner::MatchPlan::Matched(_))).count();
        let ambiguous = plan.tracks.iter().filter(|track| matches!(track.match_plan, crate::transfer::planner::MatchPlan::Ambiguous(_))).count();
        let missing = plan.tracks.iter().filter(|track| matches!(track.match_plan, crate::transfer::planner::MatchPlan::Unresolved)).count();
        let provider_matches = manifest.tracks.iter().filter(|track| track.source.is_some()).count().min(matched);
        Ok(serde_json::json!({ "matched": matched, "ambiguous": ambiguous, "missing": missing, "provider_matches": provider_matches, "unresolved_provider_handoff": missing, "no_deletions": plan.no_deletions }))
    }

    fn transfer_apply(&self, manifest: &str, destination: PathBuf, approvals: std::collections::BTreeMap<String, String>) -> Result<Value, ContractError> {
        self.validate_transfer_destination(&destination)?;
        let manifest = crate::transfer::codec::decode(manifest).map_err(|error| ContractError::invalid(error.to_string()))?;
        let plan = crate::transfer::planner::plan_import(&manifest, &self.transfer_index(), &destination);
        let approvals = approvals.into_iter().map(|(key, value)| (key, PathBuf::from(value))).collect();
        let result = crate::transfer::planner::apply_import_plan(&plan, &approvals);
        Ok(serde_json::json!({ "copied": result.copied.len(), "failures": result.failures.len(), "no_deletions": plan.no_deletions }))
    }

    fn validate_transfer_destination(&self, destination: &PathBuf) -> Result<(), ContractError> {
        let roots = self.scan_roots();
        let root = roots.first().ok_or_else(|| ContractError::invalid("no configured library root"))?;
        let root = std::fs::canonicalize(root).map_err(|_| ContractError { code: ErrorCode::NotFound, message: "configured library root is unavailable".into(), retryable: false })?;
        let destination = std::fs::canonicalize(destination).map_err(|_| ContractError::invalid("transfer destination is unavailable"))?;
        if destination.starts_with(root) { Ok(()) } else { Err(ContractError::invalid("transfer destination is outside the configured library")) }
    }

    fn promote_audio_output(
        &self,
        temporary: PathBuf,
        destination: PathBuf,
    ) -> Result<Value, ContractError> {
        if temporary.as_os_str().is_empty() || destination.as_os_str().is_empty() {
            return Err(ContractError::invalid("promotion paths must not be empty"));
        }
        let configured_root = self
            .scan_roots()
            .first()
            .cloned()
            .ok_or_else(|| ContractError::invalid("no configured library root"))?;
        let root = std::fs::canonicalize(&configured_root).map_err(|_| ContractError {
            code: ErrorCode::NotFound,
            message: "configured library root is unavailable".into(),
            retryable: false,
        })?;
        let parent = destination.parent().ok_or_else(|| {
            ContractError::invalid("promotion destination must name a file")
        })?;
        let canonical_parent = std::fs::canonicalize(parent).map_err(|_| {
            ContractError::invalid("promotion destination directory is unavailable")
        })?;
        if !canonical_parent.starts_with(&root)
            || destination.file_name().is_none_or(|name| name.is_empty())
        {
            return Err(ContractError::invalid(
                "promotion destination is outside the configured library",
            ));
        }
        promote_output(
            &temporary,
            &destination,
            Validation::audio(),
            CollisionPolicy::Reject,
            None,
        )
        .map(|receipt| serde_json::json!({ "path": receipt.path, "bytes": receipt.bytes }))
        .map_err(Self::promotion_error)
    }

fn promotion_error(error: PromotionError) -> ContractError {
        match error {
            PromotionError::Cancelled => ContractError {
                code: ErrorCode::Cancelled,
                message: "audio promotion was cancelled".into(),
                retryable: false,
            },
            PromotionError::Collision(_) => ContractError {
                code: ErrorCode::Busy,
                message: "promotion destination already exists".into(),
                retryable: false,
            },
            PromotionError::InvalidOutput(message) => ContractError::invalid(message),
            PromotionError::Io(error) => ContractError {
                code: if error.kind() == std::io::ErrorKind::NotFound {
                    ErrorCode::NotFound
                } else {
                    ErrorCode::Internal
                },
                message: "audio promotion could not complete".into(),
                retryable: error.kind() != std::io::ErrorKind::NotFound,
            },
        }
    }

    fn toggle_favorite(&self, track_id: TrackId) -> Result<Value, ContractError> {
        let path = self
            .track_paths
            .lock()
            .unwrap()
            .get(&track_id)
            .cloned()
            .ok_or_else(|| ContractError {
                code: ErrorCode::NotFound,
                message: "track is not available locally".into(),
                retryable: false,
            })?;
        let favorite = {
            let mut favorites = self.favorites.lock().unwrap();
            let next = !favorites.contains(&path);
            favorites.set(&path, next);
            next
        };
        let mut library = self.library.lock().unwrap();
        if let Some(track) = library.tracks.iter_mut().find(|track| track.id == track_id) {
            track.favorite = favorite;
        }
        Ok(serde_json::json!({ "favorite": favorite }))
    }

    pub fn record_recent(&self, track_id: TrackId) {
        let mut recent = self.recent.lock().unwrap();
        recent.retain(|id| id != &track_id);
        recent.insert(0, track_id);
        recent.truncate(100);
    }

    #[cfg(target_os = "macos")]
    fn playback_error(error: BackendError) -> ContractError {
        let code = match error {
            BackendError::InvalidSeek(_) => ErrorCode::InvalidRequest,
            BackendError::NotLoaded => ErrorCode::NotFound,
            BackendError::UnsupportedFormat(_)
            | BackendError::Io(_)
            | BackendError::Decode(_)
            | BackendError::Device(_) => ErrorCode::Unavailable,
        };
        ContractError {
            code,
            message: error.to_string(),
            retryable: matches!(code, ErrorCode::Unavailable),
        }
    }

    #[cfg(target_os = "macos")]
    fn recover_from_backend_error(&self, error: BackendError) {
        let contract = Self::playback_error(error);
        self.publish(self.event(self.library().generation, Event::Error { error: contract }));
        let action = self.recovery.lock().unwrap().record_failure();
        if matches!(action, crate::playback::recovery::RecoveryAction::Stop) {
            *self.playback_status.lock().unwrap() = crate::contracts::types::PlaybackStatus::Failed;
            self.publish_queue_snapshot();
        } else {
            let _ = self.next_track();
        }
    }

    fn playback_snapshot(
        &self,
        status: crate::contracts::types::PlaybackStatus,
        position: Duration,
        duration: Option<Duration>,
    ) -> crate::contracts::types::PlaybackSnapshot {
        let queue = self.playback.lock().unwrap();
        crate::contracts::types::PlaybackSnapshot {
            queue: queue.queue().to_vec(),
            current_index: queue.current_index().map(|index| index as u32),
            shuffle: queue.shuffle(),
            repeat: match queue.repeat() {
                crate::playback::queue::RepeatMode::Off => ContractRepeatMode::Off,
                crate::playback::queue::RepeatMode::One => ContractRepeatMode::One,
                crate::playback::queue::RepeatMode::All => ContractRepeatMode::All,
            },
            position_ms: position.as_millis().min(u64::MAX as u128) as u64,
            duration_ms: duration.map(|value| value.as_millis().min(u64::MAX as u128) as u64),
            status,
            generation: self.next_generation(),
        }
    }

    fn publish_playback(
        &self,
        snapshot: crate::contracts::types::PlaybackSnapshot,
        track: Option<TrackId>,
    ) {
        self.publish(self.event(snapshot.generation, Event::PlaybackSnapshot { snapshot }));
        if let Some(track_id) = track {
            self.publish(self.event(self.library().generation, Event::TrackStarted { track_id }));
        }
    }

    fn publish_queue_snapshot(&self) {
        let status = {
            #[cfg(target_os = "macos")]
            { *self.playback_status.lock().unwrap() }
            #[cfg(not(target_os = "macos"))]
            { crate::contracts::types::PlaybackStatus::Stopped }
        };
        let snapshot = self.playback_snapshot(status, Duration::ZERO, None);
        self.publish_playback(snapshot, None);
    }

    fn queue_insert(&self, track_ids: Vec<TrackId>, next: bool) -> Result<Value, ContractError> {
        if next { self.playback.lock().unwrap().play_next(track_ids); }
        else { self.playback.lock().unwrap().add_to_queue(track_ids); }
        self.publish_queue_snapshot();
        Ok(Value::Null)
    }

    fn queue_remove(&self, index: usize) -> Result<Value, ContractError> {
        if !self.playback.lock().unwrap().remove(index) {
            return Err(ContractError::invalid("queue index is out of range"));
        }
        self.publish_queue_snapshot();
        Ok(Value::Null)
    }

    fn queue_reorder(&self, from: usize, to: usize) -> Result<Value, ContractError> {
        if !self.playback.lock().unwrap().reorder(from, to) {
            return Err(ContractError::invalid("queue index is out of range"));
        }
        self.publish_queue_snapshot();
        Ok(Value::Null)
    }

    #[cfg(target_os = "macos")]
    fn play_track(&self, track_id: TrackId) -> Result<Value, ContractError> {
        self.play_with_context(track_id.clone(), vec![track_id])
    }

    #[cfg(target_os = "macos")]
    fn play_with_context(&self, track_id: TrackId, context: Vec<TrackId>) -> Result<Value, ContractError> {
        if context.is_empty() || !context.iter().any(|id| id == &track_id) {
            return Err(ContractError::invalid("playback context must contain the selected track"));
        }
        let path = self
            .track_paths
            .lock()
            .unwrap()
            .get(&track_id)
            .cloned()
            .ok_or_else(|| ContractError {
                code: ErrorCode::NotFound,
                message: "track is not available locally".into(),
                retryable: false,
            })?;
        self.playback.lock().unwrap().play_with_context(track_id.clone(), context);
        let mut backend = self.audio.lock().map_err(|_| ContractError {
            code: ErrorCode::Internal,
            message: "audio backend lock failed".into(),
            retryable: false,
        })?;
        let info = match backend.load(&path) {
            Ok(info) => info,
            Err(error) => {
                drop(backend);
                self.recover_from_backend_error(error);
                return Err(ContractError { code: ErrorCode::Unavailable, message: "audio track could not be loaded".into(), retryable: true });
            }
        };
        if let Err(error) = backend.play() {
            drop(backend);
            self.recover_from_backend_error(error);
            return Err(ContractError { code: ErrorCode::Unavailable, message: "audio output could not start".into(), retryable: true });
        }
        self.playback.lock().unwrap().record_started(track_id.clone(), false);
        self.record_recent(track_id.clone());
        *self.clock.lock().unwrap() = Some(PlaybackClock::new(info.duration));
        if let Some(clock) = self.clock.lock().unwrap().as_mut() { clock.start(0.0); }
        self.recovery.lock().unwrap().record_success();
        *self.current_track.lock().unwrap() = Some(track_id.clone());
        *self.playback_status.lock().unwrap() = crate::contracts::types::PlaybackStatus::Playing;
        let snapshot = self.playback_snapshot(
            crate::contracts::types::PlaybackStatus::Playing,
            Duration::ZERO,
            info.duration,
        );
        self.publish_playback(snapshot.clone(), Some(track_id));
        let heartbeat_state = self.clone();
        let generation = snapshot.generation;
        let _ = self.submit(self.generation("playback-heartbeat", generation), move |token| {
            while !token.is_cancelled() && !heartbeat_state.is_closing() {
                std::thread::sleep(crate::playback::clock::HEARTBEAT);
                if token.is_cancelled() || heartbeat_state.is_closing() { break; }
                if heartbeat_state.heartbeat_tick().is_err() { break; }
            }
        });
        serde_json::to_value(snapshot).map_err(|e| ContractError {
            code: ErrorCode::Internal,
            message: e.to_string(),
            retryable: false,
        })
    }

    #[cfg(target_os = "macos")]
    fn next_track(&self) -> Result<Value, ContractError> {
        let next = self.playback.lock().unwrap().next().cloned();
        next.map_or_else(|| {
            if let Ok(mut backend) = self.audio.lock() { backend.stop(); }
            if let Some(clock) = self.clock.lock().unwrap().as_mut() { clock.stop(); }
            *self.playback_status.lock().unwrap() = crate::contracts::types::PlaybackStatus::Stopped;
            self.publish_queue_snapshot();
            Ok(Value::Null)
        }, |track| {
            let context = self.playback.lock().unwrap().queue().to_vec();
            self.play_with_context(track, context)
        })
    }

    #[cfg(target_os = "macos")]
    fn previous_track(&self) -> Result<Value, ContractError> {
        let previous = self.playback.lock().unwrap().previous().cloned();
        previous.map_or_else(|| Ok(Value::Null), |track| {
            let context = self.playback.lock().unwrap().queue().to_vec();
            self.play_with_context(track, context)
        })
    }

    #[cfg(target_os = "macos")]
    fn pause_track(&self) -> Result<Value, ContractError> {
        let mut backend = self.audio.lock().map_err(|_| ContractError {
            code: ErrorCode::Internal,
            message: "audio backend lock failed".into(),
            retryable: false,
        })?;
        let was_paused = *self.playback_status.lock().unwrap() == crate::contracts::types::PlaybackStatus::Paused;
        if was_paused { backend.play().map_err(Self::playback_error)?; }
        else { backend.pause().map_err(Self::playback_error)?; }
        let position = match backend.position() {
            Ok(position) => position,
            Err(error) => { drop(backend); self.recover_from_backend_error(error); return Ok(Value::Null); }
        };
        let duration = match backend.duration() {
            Ok(duration) => duration,
            Err(error) => { drop(backend); self.recover_from_backend_error(error); return Ok(Value::Null); }
        };
        if was_paused {
            if let Some(clock) = self.clock.lock().unwrap().as_mut() { clock.resume(0.0); }
        } else if let Some(clock) = self.clock.lock().unwrap().as_mut() {
            clock.pause(0.0, Some(position));
        }
        *self.playback_status.lock().unwrap() = if was_paused { crate::contracts::types::PlaybackStatus::Playing } else { crate::contracts::types::PlaybackStatus::Paused };
        let snapshot = self.playback_snapshot(
            *self.playback_status.lock().unwrap(),
            position,
            duration,
        );
        self.publish_playback(snapshot.clone(), None);
        serde_json::to_value(snapshot).map_err(|e| ContractError {
            code: ErrorCode::Internal,
            message: e.to_string(),
            retryable: false,
        })
    }

    #[cfg(target_os = "macos")]
    fn seek_track(&self, position_ms: u64) -> Result<Value, ContractError> {
        let mut backend = self.audio.lock().map_err(|_| ContractError {
            code: ErrorCode::Internal,
            message: "audio backend lock failed".into(),
            retryable: false,
        })?;
        backend
            .seek(Duration::from_millis(position_ms))
            .map_err(Self::playback_error)?;
        let position = backend.position().map_err(Self::playback_error)?;
        let duration = backend.duration().map_err(Self::playback_error)?;
        if let Some(clock) = self.clock.lock().unwrap().as_mut() {
            clock.seek(position, 0.0);
        }
        let status = *self.playback_status.lock().unwrap();
        let snapshot = self.playback_snapshot(status, position, duration);
        self.publish_playback(snapshot.clone(), None);
        serde_json::to_value(snapshot).map_err(|e| ContractError {
            code: ErrorCode::Internal,
            message: e.to_string(),
            retryable: false,
        })
    }

    #[cfg(target_os = "macos")]
    fn heartbeat_tick(&self) -> Result<(), ContractError> {
        let backend = self.audio.lock().map_err(|_| ContractError { code: ErrorCode::Internal, message: "audio backend lock failed".into(), retryable: false })?;
        let position = match backend.position() {
            Ok(position) => position,
            Err(error) => {
                drop(backend);
                self.recover_from_backend_error(error);
                return Err(ContractError { code: ErrorCode::Unavailable, message: "audio position is unavailable".into(), retryable: true });
            }
        };
        let duration = match backend.duration() {
            Ok(duration) => duration,
            Err(error) => {
                drop(backend);
                self.recover_from_backend_error(error);
                return Err(ContractError { code: ErrorCode::Unavailable, message: "audio duration is unavailable".into(), retryable: true });
            }
        };
        let status = *self.playback_status.lock().unwrap();
        let elapsed = position;
        let due = self.clock.lock().unwrap().as_ref().is_some_and(|clock| clock.completion_due(elapsed, true, elapsed));
        if due && self.clock.lock().unwrap().as_mut().is_some_and(|clock| clock.dispatch_completion()) {
            drop(backend);
            let _ = self.next_track()?;
        } else {
            let snapshot = self.playback_snapshot(status, position, duration);
            self.publish_playback(snapshot, None);
        }
        Ok(())
    }

    pub fn shutdown_events(
        &self,
        reason: crate::contracts::events::ShutdownReason,
        deadline: Duration,
    ) -> ShutdownReport {
        let admission = self.admission.lock().unwrap();
        self.shutdown_events_locked(reason, deadline, &admission)
    }

    fn shutdown_events_locked(
        &self,
        reason: crate::contracts::events::ShutdownReason,
        deadline: Duration,
        _admission: &std::sync::MutexGuard<'_, ()>,
    ) -> ShutdownReport {
        let mut stored_result = self.shutdown_result.lock().unwrap();
        if let Some(report) = *stored_result {
            return report;
        }
        self.tasks.begin_shutdown();
        #[cfg(target_os = "macos")]
        if let Ok(mut backend) = self.audio.lock() {
            backend.shutdown();
        }
        self.publish(self.event(
            Generation(0),
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    version: CONTRACT_VERSION,
                    state: ShutdownState::Requested,
                    reason,
                },
            },
        ));
        self.publish(self.event(
            Generation(0),
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    version: CONTRACT_VERSION,
                    state: ShutdownState::Draining,
                    reason,
                },
            },
        ));
        let started = std::time::Instant::now();
        let report = self.shutdown(deadline);
        let remaining = deadline.saturating_sub(started.elapsed());
        let process_ok = self.supervisor.shutdown(remaining);
        let remaining = deadline.saturating_sub(started.elapsed());
        let download_report = self.downloads.shutdown(remaining);
        let remaining = deadline.saturating_sub(started.elapsed());
        let pending_before_terminal = self.retry_event_delivery(remaining);
        let terminal_timed_out = report.timed_out
            || !process_ok
            || download_report.timed_out
            || pending_before_terminal > 0;
        let failed = report.failed;
        let state = if failed {
            ShutdownState::Failed
        } else if terminal_timed_out {
            ShutdownState::TimedOut
        } else {
            ShutdownState::Complete
        };
        self.publish(self.event(
            Generation(0),
            Event::Shutdown {
                envelope: ShutdownEnvelope {
                    version: CONTRACT_VERSION,
                    state,
                    reason,
                },
            },
        ));
        if failed {
            self.publish(self.event(
                Generation(0),
                Event::Error {
                    error: ContractError {
                        code: ErrorCode::Internal,
                        message: "shutdown failed because a worker panicked".into(),
                        retryable: false,
                    },
                },
            ));
        } else if terminal_timed_out {
            self.publish(self.event(
                Generation(0),
                Event::Error {
                    error: ContractError {
                        code: ErrorCode::Internal,
                        message: "shutdown timed out; workers remain active".into(),
                        retryable: false,
                    },
                },
            ));
        }
        let remaining = deadline.saturating_sub(started.elapsed());
        let undelivered = self.retry_event_delivery(remaining);
        // The terminal drain is the final authority for the shutdown report.
        let result = ShutdownReport {
            joined: report.joined,
            timed_out: !failed && (terminal_timed_out || undelivered > 0),
            failed,
            undelivered,
        };
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
            let Some(sink) = self.event_sink.lock().unwrap().clone() else {
                return remaining_events;
            };
            let drained = self.drain_events(|event| sink(event.clone()));
            let pending = self.pending_events().len();
            let in_flight = *self.event_in_flight.lock().unwrap();
            let active = *self.event_draining.lock().unwrap();
            let remaining = pending + in_flight.max(usize::from(active));
            if remaining == 0 && !matches!(drained, Ok(false)) {
                return 0;
            }
            if started.elapsed() >= deadline {
                return remaining;
            }
            std::thread::yield_now();
        }
    }

    pub fn run_process(
        &self,
        spec: crate::process::supervisor::ProcessSpec,
    ) -> Result<crate::process::supervisor::ProcessOutput, crate::process::supervisor::SuperviseError>
    {
        let _admission = self.admission.lock().unwrap();
        self.supervisor.run_admitted(spec, &self.tasks)
    }
    pub fn process_count(&self) -> usize {
        self.supervisor.registry.active_count()
    }
}

fn validate_home(home: &PathBuf) -> Result<(), ContractError> {
    if !home.is_absolute() || home.as_os_str().is_empty() || home.to_string_lossy().chars().any(char::is_control) || !home.is_dir() || std::fs::canonicalize(home).is_err() {
        return Err(ContractError::invalid("settings home must be an absolute safe directory"));
    }
    Ok(())
}

fn validate_library_path(state: &AppState, path: &Path, allow_file: bool) -> Result<PathBuf, ContractError> {
    let root = state.scan_roots().first().cloned().ok_or_else(|| ContractError::invalid("no configured library root"))?;
    let root = std::fs::canonicalize(root).map_err(|_| ContractError { code: ErrorCode::NotFound, message: "configured library root is unavailable".into(), retryable: false })?;
    if !path.is_absolute() || path.to_string_lossy().chars().any(char::is_control) { return Err(ContractError::invalid("path must be absolute and safe")); }
    let candidate = if path.exists() { std::fs::canonicalize(path) } else { path.parent().map(std::fs::canonicalize).unwrap_or_else(|| Err(std::io::Error::other("missing parent"))) };
    let candidate = candidate.map_err(|_| ContractError::invalid("path is unavailable"))?;
    if !candidate.starts_with(&root) || (!allow_file && !path.is_dir()) { return Err(ContractError::invalid("path is outside the configured library")); }
    Ok(candidate)
}

fn sanitize_download_error(error: &str) -> String {
    let first = error.lines().next().unwrap_or("download failed");
    if first.contains('/') || first.contains('\\') { "download worker failed".into() } else { first.chars().take(160).collect() }
}
