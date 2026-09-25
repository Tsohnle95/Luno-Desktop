use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::contracts::types::{Generation, SessionId};

#[derive(Clone, Debug)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self { Self(Arc::new(AtomicBool::new(false))) }
    pub fn cancel(&self) { self.0.store(true, Ordering::Release); }
    pub fn is_cancelled(&self) -> bool { self.0.load(Ordering::Acquire) }
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() { Err(Cancelled) } else { Ok(()) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskSpec {
    pub key: String,
    pub generation: Generation,
    pub session_id: Option<SessionId>,
}

impl TaskSpec {
    pub fn new(key: impl Into<String>, generation: Generation, session_id: Option<SessionId>) -> Self {
        Self { key: key.into(), generation, session_id }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError { AtCapacity, ShuttingDown, ThreadSpawnFailed }

pub struct TaskHandle<T> { pub spec: TaskSpec, receiver: std::sync::mpsc::Receiver<T> }
impl<T> TaskHandle<T> {
    pub fn try_recv(&self) -> Result<T, std::sync::mpsc::TryRecvError> { self.receiver.try_recv() }
    pub fn recv(self) -> Result<T, std::sync::mpsc::RecvError> { self.receiver.recv() }
}

struct Entry { spec: TaskSpec, token: CancellationToken, join: JoinHandle<()> }
struct Inner { closing: bool, next_id: u64, entries: HashMap<u64, Entry>, latest: HashMap<String, u64>, failed: bool }

pub struct TaskRegistry { limit: usize, inner: Mutex<Inner> }

impl TaskRegistry {
    pub fn new(limit: usize) -> Self {
        assert!(limit > 0, "task limit must be positive");
        Self { limit, inner: Mutex::new(Inner { closing: false, next_id: 0, entries: HashMap::new(), latest: HashMap::new(), failed: false }) }
    }

    pub fn active_count(&self) -> usize { self.inner.lock().unwrap_or_else(|e| e.into_inner()).entries.len() }
    pub fn is_closing(&self) -> bool { self.inner.lock().unwrap_or_else(|e| e.into_inner()).closing }
    pub fn begin_shutdown(&self) { self.inner.lock().unwrap_or_else(|e| e.into_inner()).closing = true; }

    pub fn spawn<T, F>(&self, spec: TaskSpec, work: F) -> Result<TaskHandle<T>, SpawnError>
    where T: Send + 'static, F: FnOnce(CancellationToken) -> T + Send + 'static {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let (id, token) = {
            let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            self.reap_finished(&mut state);
            if state.closing { return Err(SpawnError::ShuttingDown); }
            if state.entries.len() >= self.limit { return Err(SpawnError::AtCapacity); }
            // A newer request supersedes the previous request for this logical task.
            if let Some(old) = state.latest.get(&spec.key).copied() {
                if let Some(entry) = state.entries.get(&old) { entry.token.cancel(); }
            }
            state.next_id += 1;
            let id = state.next_id;
            let token = CancellationToken::new();
            let thread_token = token.clone();
            let started = Arc::new(AtomicBool::new(false));
            let worker_started = started.clone();
            let join = thread::Builder::new().name(format!("luno-task-{}", spec.key)).spawn(move || {
                while !worker_started.load(Ordering::Acquire) {
                    thread::yield_now();
                }
                let result = work(thread_token);
                let _ = tx.send(result);
            }).map_err(|_| SpawnError::ThreadSpawnFailed)?;
            state.latest.insert(spec.key.clone(), id);
            state.entries.insert(id, Entry { spec: spec.clone(), token: token.clone(), join });
            started.store(true, Ordering::Release);
            (id, token)
        };
        let _ = (id, token);
        Ok(TaskHandle { spec, receiver: rx })
    }

    pub fn cancel_key(&self, key: &str) -> bool {
        let state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        state.latest.get(key).and_then(|id| state.entries.get(id)).map(|e| { e.token.cancel(); true }).unwrap_or(false)
    }

    pub fn cancel_all(&self) {
        for entry in self.inner.lock().unwrap_or_else(|e| e.into_inner()).entries.values() { entry.token.cancel(); }
    }

    pub fn is_current(&self, spec: &TaskSpec) -> bool {
        let state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        !state.closing
            && state
                .latest
                .get(&spec.key)
                .and_then(|id| state.entries.get(id))
                .map(|e| e.spec == *spec && !e.token.is_cancelled())
                .unwrap_or(false)
    }

    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport {
        let started = Instant::now();
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !state.closing { state.closing = true; }
        for entry in state.entries.values() { entry.token.cancel(); }
        drop(state);
        let mut joined = 0;
        loop {
            let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            joined += self.reap_finished(&mut state);
            if state.entries.is_empty() { return ShutdownReport { joined, timed_out: false, failed: state.failed, undelivered: 0 }; }
            if started.elapsed() >= deadline { return ShutdownReport { joined, timed_out: true, failed: state.failed, undelivered: 0 }; }
            drop(state);
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn reap_finished(&self, state: &mut Inner) -> usize {
        let done: Vec<u64> = state.entries.iter().filter_map(|(id, e)| e.join.is_finished().then_some(*id)).collect();
        let count = done.len();
        for id in done {
            if let Some(entry) = state.entries.remove(&id) {
                if entry.join.join().is_err() { state.failed = true; }
                if state.latest.get(&entry.spec.key) == Some(&id) { state.latest.remove(&entry.spec.key); }
            }
        }
        count
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownReport { pub joined: usize, pub timed_out: bool, pub failed: bool, pub undelivered: usize }
