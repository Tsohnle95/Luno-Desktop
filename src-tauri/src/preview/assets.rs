//! Disposable recommendation audio staging.
//!
//! This module intentionally has no connection to the library index or the
//! download marker system.  A preview is published only by renaming a
//! completed, bounded `.part` file inside the private preview directory.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_PREVIEW_BYTES: u64 = 512 * 1024 * 1024;
// This stage includes the helper's yt-dlp startup AND the bounded audio fetch.
// The bundled executable can need >15s merely to self-extract on a cold start,
// which incorrectly marked perfectly playable recommendations unavailable.
pub const RESOLVE_DEADLINE: Duration = Duration::from_secs(2 * 60);
pub const DOWNLOAD_DEADLINE: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetState { Waiting, Resolving, Preparing, Ready, Unavailable, Cancelled }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset { pub key: String, pub generation: u64, pub state: AssetState, pub path: Option<PathBuf> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError { Io(String), Timeout, Cancelled, StaleGeneration, TooLarge, InvalidPath, Failed(String) }

impl From<io::Error> for AssetError { fn from(e: io::Error) -> Self { Self::Io(e.to_string()) } }

/// A writer which makes the size limit effective while bytes are being written.
pub struct LimitedWriter { file: File, written: u64 }
impl LimitedWriter {
    pub fn new(file: File) -> Self { Self { file, written: 0 } }
    pub fn bytes_written(&self) -> u64 { self.written }
}
impl Write for LimitedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let remaining = MAX_PREVIEW_BYTES.saturating_sub(self.written);
        if remaining == 0 { return Err(io::Error::new(io::ErrorKind::Other, "preview exceeds size limit")); }
        let take = buf.len().min(remaining as usize);
        let n = self.file.write(&buf[..take])?;
        self.written += n as u64;
        if n < buf.len() { return Err(io::Error::new(io::ErrorKind::Other, "preview exceeds size limit")); }
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> { self.file.flush() }
}

#[derive(Clone)]
pub struct PreviewAssetStore { root: PathBuf, generation: Arc<Mutex<u64>> }

impl PreviewAssetStore {
    /// Uses `~/.luno_previews`, never a child of the configured music root.
    pub fn new() -> Result<Self, AssetError> {
        let home = std::env::var_os("HOME").ok_or_else(|| AssetError::Io("HOME is not set".into()))?;
        Self::at(PathBuf::from(home).join(".luno_previews"))
    }
    pub fn at(root: impl Into<PathBuf>) -> Result<Self, AssetError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        cleanup_dir(&root, &[])?; // startup cleanup; no previous session is trusted
        Ok(Self { root, generation: Arc::new(Mutex::new(0)) })
    }
    pub fn root(&self) -> &Path { &self.root }
    pub fn begin_generation(&mut self) -> u64 {
        let mut generation = self.generation.lock().unwrap_or_else(|e| e.into_inner());
        *generation += 1;
        *generation
    }
    pub fn current_generation(&self) -> u64 { *self.generation.lock().unwrap_or_else(|e| e.into_inner()) }

    pub fn ready_path(&self, key: &str) -> Result<PathBuf, AssetError> {
        let path = self.root.join(format!("{}.aac", safe_key(key)?));
        contained(&self.root, &path).then_some(path).ok_or(AssetError::InvalidPath)
    }

    /// Runs a resolver without allowing a late result to publish to the caller.
    pub fn resolve<T: Send + 'static>(&self, generation: u64, cancel: &AtomicBool, work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, AssetError> {
        self.deadline(generation, cancel, RESOLVE_DEADLINE, work)
    }

    /// Writes a preview to a private partial file and atomically publishes it.
    pub fn download(&self, key: &str, generation: u64, cancel: &AtomicBool, work: impl FnOnce(&mut LimitedWriter) -> Result<(), String> + Send + 'static) -> Result<PathBuf, AssetError> {
        let final_path = self.ready_path(key)?;
        if generation != self.current_generation() { return Err(AssetError::StaleGeneration); }
        let partial = self.partial_path(&final_path)?;
        let file = OpenOptions::new().write(true).create_new(true).open(&partial)?;
        let result = self.deadline(generation, cancel, DOWNLOAD_DEADLINE, move || {
            let mut writer = LimitedWriter::new(file);
            work(&mut writer).map_err(|e| e).and_then(|_| writer.flush().map_err(|e| e.to_string()))
        });
        match result {
            Ok(()) => {
                if cancel.load(Ordering::Acquire) { let _ = fs::remove_file(&partial); return Err(AssetError::Cancelled); }
                // Publication and invalidation share this short lock. A stale
                // worker can finish its encode but cannot rename over a new seed.
                let active = self.generation.lock().unwrap_or_else(|e| e.into_inner());
                if generation != *active { let _ = fs::remove_file(&partial); return Err(AssetError::StaleGeneration); }
                let bytes = fs::metadata(&partial)?.len();
                if bytes == 0 { let _ = fs::remove_file(&partial); return Err(AssetError::Failed("empty preview".into())); }
                if bytes > MAX_PREVIEW_BYTES { let _ = fs::remove_file(&partial); return Err(AssetError::TooLarge); }
                fs::rename(&partial, &final_path)?;
                Ok(final_path)
            }
            Err(error) => { let _ = fs::remove_file(&partial); Err(error) }
        }
    }

    pub fn cleanup_session(&self, preserve: Option<&Path>) -> Result<(), AssetError> {
        let _generation = self.generation.lock().unwrap_or_else(|e| e.into_inner());
        cleanup_dir(&self.root, preserve.map(|p| p.to_path_buf()).as_slice()).map_err(Into::into)
    }
    pub fn mark_unavailable(&self, key: impl Into<String>, generation: u64) -> Asset { Asset { key: key.into(), generation, state: AssetState::Unavailable, path: None } }

    fn partial_path(&self, final_path: &Path) -> Result<PathBuf, AssetError> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let path = PathBuf::from(format!("{}.{}.part", final_path.display(), stamp));
        contained(&self.root, &path).then_some(path).ok_or(AssetError::InvalidPath)
    }
    fn deadline<T: Send + 'static>(&self, generation: u64, cancel: &AtomicBool, limit: Duration, work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, AssetError> {
        if cancel.load(Ordering::Acquire) { return Err(AssetError::Cancelled); }
        if generation != self.current_generation() { return Err(AssetError::StaleGeneration); }
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || { let _ = tx.send(work()); });
        let started = Instant::now();
        loop {
            if cancel.load(Ordering::Acquire) { return Err(AssetError::Cancelled); }
            if generation != self.current_generation() {
                cancel.store(true, Ordering::Release);
                return Err(AssetError::StaleGeneration);
            }
            let remaining = limit.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                // The deadline owns the child-process cancellation bridge.
                // Returning without setting it orphaned yt-dlp/ffmpeg while
                // the UI remained stuck in an indeterminate phase.
                cancel.store(true, Ordering::Release);
                return Err(AssetError::Timeout);
            }
            match rx.recv_timeout(remaining.min(Duration::from_millis(25))) {
                Ok(Ok(value)) => return Ok(value), Ok(Err(error)) => return Err(AssetError::Failed(error)), Err(mpsc::RecvTimeoutError::Timeout) => continue, Err(_) => return Err(AssetError::Failed("preview worker stopped".into()))
            }
        }
    }
}

pub fn safe_key(key: &str) -> Result<String, AssetError> {
    let value: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if value.is_empty() || value == "." || value == ".." { Err(AssetError::InvalidPath) } else { Ok(value) }
}
fn contained(root: &Path, path: &Path) -> bool { path.parent().is_some_and(|p| p == root) }
fn cleanup_dir(root: &Path, preserve: &[PathBuf]) -> io::Result<()> {
    for entry in fs::read_dir(root)? { let path = entry?.path(); if preserve.iter().any(|p| p == &path) { continue; } if path.is_file() { let _ = fs::remove_file(path); } }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_change_does_not_wait_for_a_running_download_and_fences_publication() {
        let root = std::env::temp_dir().join(format!("luno-preview-store-concurrency-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut store = PreviewAssetStore::at(&root).unwrap();
        let generation = store.current_generation();
        let worker_store = store.clone();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            worker_store.download("old-seed", generation, &AtomicBool::new(false), move |writer| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                writer.write_all(b"stale audio").map_err(|error| error.to_string())
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).expect("download should start");
        let started = Instant::now();
        store.begin_generation();
        store.cleanup_session(None).unwrap();
        assert!(started.elapsed() < Duration::from_millis(250), "a new seed must not wait for the old helper");
        release_tx.send(()).unwrap();
        assert_eq!(worker.join().unwrap(), Err(AssetError::StaleGeneration));
        assert!(!store.ready_path("old-seed").unwrap().exists());
        let _ = fs::remove_dir_all(root);
    }
}
