//! Coordination for the one normal library download/write job.

use crate::download::promotion::{promote_output, stage_output, CollisionPolicy, PromotionError, Validation};
use crate::runtime::task_registry::CancellationToken;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
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
    Cancelling { job_id: String },
    Succeeded { job_id: String, path: PathBuf, bytes: u64 },
    Failed { job_id: String, error: String },
    Cancelled { job_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError { Busy, ShuttingDown, InvalidRequest }

pub type DownloadWorker = Box<dyn FnOnce(&Path, &CancellationToken, &dyn Fn(f32)) -> Result<(), String> + Send + 'static>;

struct Job { job_id: String, token: CancellationToken, join: JoinHandle<()> }
struct Inner { closing: bool, active: Option<Job> }

/// Owns the worker handle.  In particular, this type never uses a detached
/// thread: an unfinished handle remains owned until its worker has exited.
pub struct DownloadCoordinator {
    inner: Arc<Mutex<Inner>>,
    events: mpsc::Sender<DownloadEvent>,
    receiver: Mutex<Option<mpsc::Receiver<DownloadEvent>>>,
}

impl DownloadCoordinator {
    pub fn new() -> Self {
        let (events, receiver) = mpsc::channel();
        Self { inner: Arc::new(Mutex::new(Inner { closing: false, active: None })), events, receiver: Mutex::new(Some(receiver)) }
    }

    pub fn events(&self) -> Option<mpsc::Receiver<DownloadEvent>> { self.receiver.lock().unwrap().take() }

    pub fn is_busy(&self) -> bool {
        let mut inner = self.inner.lock().unwrap();
        reap(&mut inner);
        inner.active.is_some()
    }

    pub fn start(&self, request: DownloadRequest, worker: DownloadWorker) -> Result<(), StartError> {
        if request.job_id.trim().is_empty() || request.destination.as_os_str().is_empty() { return Err(StartError::InvalidRequest); }
        let mut inner = self.inner.lock().unwrap();
        reap(&mut inner);
        if inner.closing { return Err(StartError::ShuttingDown); }
        if inner.active.is_some() { return Err(StartError::Busy); }
        let temporary = stage_output(&request.destination).map_err(|_| StartError::InvalidRequest)?;
        let job_id = request.job_id.clone();
        let owned_job_id = job_id.clone();
        let token = CancellationToken::new();
        let thread_token = token.clone();
        let events = self.events.clone();
        let destination = request.destination;
        let join = thread::Builder::new().name("luno-download".into()).spawn(move || {
            let _ = events.send(DownloadEvent::Started { job_id: job_id.clone() });
            let progress_events = |fraction: f32| { let _ = events.send(DownloadEvent::Progress { job_id: job_id.clone(), fraction: fraction.clamp(0.0, 1.0) }); };
            let result = if thread_token.is_cancelled() { Err("cancelled".into()) } else { worker(&temporary, &thread_token, &progress_events) };
            let terminal = if thread_token.is_cancelled() {
                let _ = std::fs::remove_file(&temporary);
                DownloadEvent::Cancelled { job_id: job_id.clone() }
            } else {
                match result {
                    Ok(()) => match promote_output(&temporary, &destination, Validation::audio(), CollisionPolicy::Reject, None) {
                        Ok(receipt) => DownloadEvent::Succeeded { job_id: job_id.clone(), path: receipt.path, bytes: receipt.bytes },
                        Err(error) => DownloadEvent::Failed { job_id: job_id.clone(), error: promotion_error(error) },
                    },
                    Err(error) => { let _ = std::fs::remove_file(&temporary); DownloadEvent::Failed { job_id: job_id.clone(), error } },
                }
            };
            let _ = events.send(terminal);
        }).map_err(|_| StartError::InvalidRequest)?;
        inner.active = Some(Job { job_id: owned_job_id, token, join });
        Ok(())
    }

    pub fn cancel(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        if let Some(job) = &inner.active {
            job.token.cancel();
            let _ = self.events.send(DownloadEvent::Cancelling { job_id: job.job_id.clone() });
            true
        } else { false }
    }

    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport {
        let started = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        inner.closing = true;
        if let Some(job) = &inner.active { job.token.cancel(); }
        while inner.active.as_ref().is_some_and(|job| !job.join.is_finished()) && started.elapsed() < deadline {
            drop(inner);
            thread::sleep(Duration::from_millis(2));
            inner = self.inner.lock().unwrap();
        }
        let timed_out = inner.active.as_ref().is_some_and(|job| !job.join.is_finished());
        if !timed_out { reap(&mut inner); }
        ShutdownReport { timed_out }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownReport { pub timed_out: bool }

fn reap(inner: &mut Inner) {
    if inner.active.as_ref().is_some_and(|job| job.join.is_finished()) {
        if let Some(job) = inner.active.take() { let _ = job.join.join(); }
    }
}

fn promotion_error(error: PromotionError) -> String { format!("{error:?}") }

impl Default for DownloadCoordinator { fn default() -> Self { Self::new() } }

impl Drop for DownloadCoordinator {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(job) = inner.active.take() {
            job.token.cancel();
            let _ = job.join.join();
        }
    }
}
