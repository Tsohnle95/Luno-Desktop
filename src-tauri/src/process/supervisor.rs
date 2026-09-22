use crate::runtime::task_registry::{CancellationToken, TaskRegistry};
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const DEFAULT_OUTPUT_LIMIT: usize = 1024 * 1024;
pub const DEFAULT_KILL_GRACE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason { Exited, Failed, TimedOut, Cancelled }

#[derive(Debug)]
pub struct ProcessOutput { pub status: std::process::ExitStatus, pub stdout: Vec<u8>, pub stderr: Vec<u8>, pub truncated_stdout: bool, pub truncated_stderr: bool, pub reason: StopReason }

#[derive(Debug)]
pub enum SuperviseError { Io(io::Error), Timeout, Cancelled, OutputThread, Spawn(io::Error) }
impl From<io::Error> for SuperviseError { fn from(e: io::Error) -> Self { Self::Io(e) } }

#[derive(Clone, Default)]
pub struct ProcessRegistry { inner: Arc<Mutex<HashMap<u64, CancellationToken>>>, next: Arc<Mutex<u64>>, admission: Arc<Mutex<()>>, closing: Arc<AtomicBool> }
impl ProcessRegistry {
    pub fn new() -> Self { Self::default() }
    fn register(&self) -> Result<(u64, CancellationToken), SuperviseError> {
        self.register_checked(false)
    }
    fn register_checked(&self, closing: bool) -> Result<(u64, CancellationToken), SuperviseError> {
        let _admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        if closing || self.closing.load(Ordering::Acquire) { return Err(SuperviseError::Cancelled); }
        let mut n = self.next.lock().unwrap_or_else(|e| e.into_inner()); *n += 1; let id = *n;
        let token = CancellationToken::new(); self.inner.lock().unwrap_or_else(|e| e.into_inner()).insert(id, token.clone()); Ok((id, token))
    }
    fn unregister(&self, id: u64) { self.inner.lock().unwrap_or_else(|e| e.into_inner()).remove(&id); }
    pub fn cancel_all(&self) { for token in self.inner.lock().unwrap_or_else(|e| e.into_inner()).values() { token.cancel(); } }
    pub fn active_count(&self) -> usize { self.inner.lock().unwrap_or_else(|e| e.into_inner()).len() }
    pub fn shutdown(&self, deadline: Duration) -> bool {
        let _admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        self.closing.store(true, Ordering::Release);
        self.cancel_all();
        drop(_admission);
        let start = Instant::now();
        while self.active_count() != 0 { if start.elapsed() >= deadline { return false; } thread::sleep(Duration::from_millis(2)); }
        true
    }
}

pub struct ProcessSpec { pub program: PathBuf, pub args: Vec<String>, pub deadline: Duration, pub output_limit: usize, pub output_path: Option<PathBuf>, pub kill_grace: Duration }
impl ProcessSpec { pub fn new(program: impl Into<PathBuf>, args: Vec<String>, deadline: Duration) -> Self { Self { program: program.into(), args, deadline, output_limit: DEFAULT_OUTPUT_LIMIT, output_path: None, kill_grace: DEFAULT_KILL_GRACE } } }

pub struct Supervisor { pub registry: ProcessRegistry }
impl Supervisor {
    pub fn new(registry: ProcessRegistry) -> Self { Self { registry } }
    pub fn shutdown(&self, deadline: Duration) -> bool { self.registry.shutdown(deadline) }
    pub fn run(&self, spec: ProcessSpec) -> Result<ProcessOutput, SuperviseError> {
        let (id, token) = self.registry.register()?;
        let result = run_process(&spec, &token, None);
        self.registry.unregister(id);
        if result.as_ref().map(|output| !output.status.success()).unwrap_or(true) {
            if let Some(path) = &spec.output_path { let _ = std::fs::remove_file(path); }
        }
        result
    }
    pub fn run_admitted(&self, spec: ProcessSpec, tasks: &TaskRegistry) -> Result<ProcessOutput, SuperviseError> {
        let (id, token) = self.registry.register_checked(tasks.is_closing())?;
        let result = run_process(&spec, &token, None);
        self.registry.unregister(id);
        if result.as_ref().map(|output| !output.status.success()).unwrap_or(true) {
            if let Some(path) = &spec.output_path { let _ = std::fs::remove_file(path); }
        }
        result
    }
    pub fn run_cancellable(&self, spec: ProcessSpec, external: &CancellationToken) -> Result<ProcessOutput, SuperviseError> {
        let (id, token) = self.registry.register()?;
        let result = run_process(&spec, &token, Some(external));
        self.registry.unregister(id);
        result
    }
}

fn run_process(spec: &ProcessSpec, token: &CancellationToken, external: Option<&CancellationToken>) -> Result<ProcessOutput, SuperviseError> {
    let mut command = Command::new(&spec.program); command.args(&spec.args).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)] unsafe { use std::os::unix::process::CommandExt; command.pre_exec(|| { if libc::setpgid(0, 0) != 0 { return Err(io::Error::last_os_error()); } Ok(()) }); }
    let mut child = command.spawn().map_err(SuperviseError::Spawn)?;
    let stdout = child.stdout.take().ok_or_else(|| SuperviseError::Io(io::Error::other("stdout pipe unavailable")))?;
    let stderr = child.stderr.take().ok_or_else(|| SuperviseError::Io(io::Error::other("stderr pipe unavailable")))?;
    let limit = spec.output_limit; let out = thread::spawn(move || read_bounded(stdout, limit)); let err = thread::spawn(move || read_bounded(stderr, limit));
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? { let (o, ot) = out.join().map_err(|_| SuperviseError::OutputThread)??; let (e, et) = err.join().map_err(|_| SuperviseError::OutputThread)??; return Ok(ProcessOutput { status, stdout:o, stderr:e, truncated_stdout:ot, truncated_stderr:et, reason: if status.success() { StopReason::Exited } else { StopReason::Failed } }); }
        if token.is_cancelled() || external.is_some_and(CancellationToken::is_cancelled) { terminate(&mut child, spec.kill_grace); let _ = out.join(); let _ = err.join(); return Err(SuperviseError::Cancelled); }
        if start.elapsed() >= spec.deadline { terminate(&mut child, spec.kill_grace); let _ = out.join(); let _ = err.join(); return Err(SuperviseError::Timeout); }
        thread::sleep(Duration::from_millis(5));
    }
}

fn read_bounded(mut reader: impl Read, limit: usize) -> io::Result<(Vec<u8>, bool)> { let mut data = Vec::new(); let mut buf = [0u8; 8192]; let mut truncated = false; loop { let n = reader.read(&mut buf)?; if n == 0 { break; } let room = limit.saturating_sub(data.len()); if n > room { data.extend_from_slice(&buf[..room]); truncated = true; } else { data.extend_from_slice(&buf[..n]); } } Ok((data, truncated)) }

fn terminate(child: &mut Child, grace: Duration) {
    #[cfg(unix)] { let _ = Command::new("/bin/kill").arg("-TERM").arg(format!("-{}", child.id())).status(); }
    let start = Instant::now(); while start.elapsed() < grace { if child.try_wait().ok().flatten().is_some() { return; } thread::sleep(Duration::from_millis(5)); }
    #[cfg(unix)] { let _ = Command::new("/bin/kill").arg("-KILL").arg(format!("-{}", child.id())).status(); }
    let _ = child.kill(); let _ = child.wait();
}
