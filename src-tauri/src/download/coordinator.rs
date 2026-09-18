//! Coordination for the one normal library download/write job.

use crate::download::promotion::{promote_output, stage_output, CollisionPolicy, PromotionError, Validation};
use crate::runtime::task_registry::CancellationToken;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRequest {
    pub job_id: String,
    pub destination: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadEvent {
    Started { job_id: String },
    Progress { job_id: String, fraction: f32 },
    /// The helper has exited successfully; promotion and validation are still
    /// in progress and the destination is not playable yet.
    Finalizing { job_id: String },
    Cancelling { job_id: String },
    Succeeded { job_id: String, path: PathBuf, bytes: u64 },
    Failed { job_id: String, error: String },
    Cancelled { job_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError { Busy, ShuttingDown, InvalidRequest }

pub type DownloadWorker = Box<dyn FnOnce(&Path, &CancellationToken, &dyn Fn(f32)) -> Result<(), String> + Send + 'static>;

struct Job { job_id: String, token: CancellationToken, terminal: Arc<AtomicBool>, join: JoinHandle<()> }
struct Inner { closing: bool, active: Option<Job> }

/// Owns the worker handle.  In particular, this type never uses a detached
/// thread: an unfinished handle remains owned until its worker has exited.
pub struct DownloadCoordinator {
    inner: Arc<Mutex<Inner>>,
    subscribers: Arc<Mutex<HashMap<String, Vec<mpsc::Sender<DownloadEvent>>>>>,
}

impl DownloadCoordinator {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(Inner { closing: false, active: None })), subscribers: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Subscribe before starting a job. Each job delivery consumer owns its receiver.
    pub fn events_for(&self, job_id: &str) -> mpsc::Receiver<DownloadEvent> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers.lock().unwrap_or_else(|e| e.into_inner()).entry(job_id.to_owned()).or_default().push(sender);
        receiver
    }

    pub fn remove_events_for(&self, job_id: &str) {
        self.subscribers.lock().unwrap_or_else(|e| e.into_inner()).remove(job_id);
    }

    /// Compatibility subscription for policy tests; it receives every job event.
    pub fn events(&self) -> Option<mpsc::Receiver<DownloadEvent>> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers.lock().unwrap_or_else(|e| e.into_inner()).entry("*".into()).or_default().push(sender);
        Some(receiver)
    }

    pub fn is_busy(&self) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        reap(&mut inner);
        inner.active.is_some()
    }

    pub fn start(&self, request: DownloadRequest, worker: DownloadWorker) -> Result<(), StartError> {
        if request.job_id.trim().is_empty() || request.destination.as_os_str().is_empty() { return Err(StartError::InvalidRequest); }
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        reap(&mut inner);
        if inner.closing { return Err(StartError::ShuttingDown); }
        if inner.active.is_some() { return Err(StartError::Busy); }
        let temporary = stage_output(&request.destination).map_err(|_| StartError::InvalidRequest)?;
        let job_id = request.job_id.clone();
        let owned_job_id = job_id.clone();
        let token = CancellationToken::new();
        let thread_token = token.clone();
        let terminal = Arc::new(AtomicBool::new(false));
        let worker_terminal = terminal.clone();
        let subscribers = self.subscribers.clone();
        let destination = request.destination;
        let join = thread::Builder::new().name("luno-download".into()).spawn(move || {
            let emit = |event: DownloadEvent| {
                let mut subscriptions = subscribers.lock().unwrap_or_else(|e| e.into_inner());
                let mut send = |key: &str| {
                    if let Some(list) = subscriptions.get_mut(key) { list.retain(|sender| sender.send(event.clone()).is_ok()); }
                };
                send(&job_id);
                send("*");
                subscriptions.retain(|_, list| !list.is_empty());
                if matches!(event, DownloadEvent::Succeeded { .. } | DownloadEvent::Failed { .. } | DownloadEvent::Cancelled { .. }) {
                    subscriptions.remove(&job_id);
                }
            };
            emit(DownloadEvent::Started { job_id: job_id.clone() });
            let progress_events = |fraction: f32| { emit(DownloadEvent::Progress { job_id: job_id.clone(), fraction: fraction.clamp(0.0, 1.0) }); };
            let result = if thread_token.is_cancelled() { Err("cancelled".into()) } else { worker(&temporary, &thread_token, &progress_events) };
            let terminal = if thread_token.is_cancelled() {
                let _ = std::fs::remove_file(&temporary);
                DownloadEvent::Cancelled { job_id: job_id.clone() }
            } else {
                match result {
                    Ok(()) => {
                        emit(DownloadEvent::Finalizing { job_id: job_id.clone() });
                        match promote_output(&temporary, &destination, Validation::audio(), CollisionPolicy::Reject, None) {
                            Ok(receipt) => DownloadEvent::Succeeded { job_id: job_id.clone(), path: receipt.path, bytes: receipt.bytes },
                            Err(error) => DownloadEvent::Failed { job_id: job_id.clone(), error: promotion_error(error) },
                        }
                    },
                    Err(error) => { let _ = std::fs::remove_file(&temporary); DownloadEvent::Failed { job_id: job_id.clone(), error } },
                }
            };
            worker_terminal.store(true, Ordering::Release);
            emit(terminal);
        }).map_err(|_| StartError::InvalidRequest)?;
        inner.active = Some(Job { job_id: owned_job_id, token, terminal, join });
        Ok(())
    }

    pub fn cancel(&self) -> bool {
        let job_id = self.inner.lock().unwrap_or_else(|e| e.into_inner()).active.as_ref().map(|job| job.job_id.clone());
        job_id.is_some_and(|job_id| self.cancel_job(&job_id))
    }

    pub fn cancel_job(&self, requested_job_id: &str) -> bool {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(job) = inner.active.as_ref().filter(|job| job.job_id == requested_job_id) {
            if job.token.is_cancelled() { false } else {
                job.token.cancel();
                self.emit(DownloadEvent::Cancelling { job_id: job.job_id.clone() });
                true
            }
        } else { false }
    }

    fn emit(&self, event: DownloadEvent) {
        let mut subscriptions = self.subscribers.lock().unwrap_or_else(|e| e.into_inner());
        for key in [event.job_id(), "*"] {
            if let Some(list) = subscriptions.get_mut(key) { list.retain(|sender| sender.send(event.clone()).is_ok()); }
        }
        subscriptions.retain(|_, list| !list.is_empty());
    }

    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport {
        let started = Instant::now();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.closing = true;
        if let Some(job) = &inner.active { job.token.cancel(); }
        while inner.active.as_ref().is_some_and(|job| !job.join.is_finished()) && started.elapsed() < deadline {
            drop(inner);
            thread::sleep(Duration::from_millis(2));
            inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        }
        let timed_out = inner.active.as_ref().is_some_and(|job| !job.join.is_finished());
        if !timed_out { reap(&mut inner); }
        ShutdownReport { timed_out }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownReport { pub timed_out: bool }

fn reap(inner: &mut Inner) {
    if inner.active.as_ref().is_some_and(|job| job.join.is_finished() || job.terminal.load(Ordering::Acquire)) {
        if let Some(job) = inner.active.take() { let _ = job.join.join(); }
    }
}

fn promotion_error(error: PromotionError) -> String { format!("{error:?}") }

impl DownloadEvent {
    fn job_id(&self) -> &str {
        match self { Self::Started { job_id } | Self::Progress { job_id, .. } | Self::Finalizing { job_id } | Self::Cancelling { job_id } | Self::Succeeded { job_id, .. } | Self::Failed { job_id, .. } | Self::Cancelled { job_id } => job_id }
    }
}

impl Default for DownloadCoordinator { fn default() -> Self { Self::new() } }

impl Drop for DownloadCoordinator {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(job) = inner.active.take() {
            job.token.cancel();
            let _ = job.join.join();
        }
    }
}
