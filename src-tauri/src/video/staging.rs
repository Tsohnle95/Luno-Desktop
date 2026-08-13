//! Current/next temporary MP4 staging, isolated from the music library.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot { Current, Next }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageState { NotChecked, Downloading, Ready, Unavailable }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedVideo { pub slot: Slot, pub generation: u64, pub state: StageState, pub path: Option<PathBuf> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageError { Io(String), Cancelled, StaleGeneration, InvalidRoot, InvalidPath, Empty }
impl From<io::Error> for StageError { fn from(e: io::Error) -> Self { Self::Io(e.to_string()) } }

#[derive(Debug)]
pub struct VideoStage { root: PathBuf, library_root: Option<PathBuf>, generation: u64 }

impl VideoStage {
    pub fn at(root: impl Into<PathBuf>, library_root: Option<&Path>) -> Result<Self, StageError> {
        let root = root.into();
        if let Some(library) = library_root {
            if root == library || root.starts_with(library) { return Err(StageError::InvalidRoot); }
        }
        fs::create_dir_all(&root)?;
        cleanup(&root)?;
        Ok(Self { root, library_root: library_root.map(Path::to_path_buf), generation: 0 })
    }
    pub fn root(&self) -> &Path { &self.root }
    pub fn begin_generation(&mut self) -> u64 { self.generation += 1; self.generation }
    pub fn state(&self, slot: Slot, generation: u64) -> StagedVideo { StagedVideo { slot, generation, state: StageState::NotChecked, path: None } }
    pub fn adopt(&self, slot: Slot, generation: u64, path: impl AsRef<Path>) -> Result<StagedVideo, StageError> {
        self.check_generation(generation)?;
        let path = path.as_ref();
        if !self.contained(path) || !path.is_file() || path.extension().and_then(|x| x.to_str()) != Some("mp4") { return Err(StageError::InvalidPath); }
        Ok(StagedVideo { slot, generation, state: StageState::Ready, path: Some(path.to_path_buf()) })
    }
    pub fn stage<F>(&self, slot: Slot, key: &str, generation: u64, cancel: &AtomicBool, write: F) -> Result<StagedVideo, StageError>
    where F: FnOnce(&mut dyn Write) -> Result<(), String> {
        self.check_generation(generation)?;
        if cancel.load(Ordering::Acquire) { return Err(StageError::Cancelled); }
        let destination = self.destination(slot, key)?;
        if destination.is_file() { return Ok(StagedVideo { slot, generation, state: StageState::Ready, path: Some(destination) }); }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let safe = safe_key(key)?;
        let partial = self.root.join(format!(".{safe}.{stamp}.part"));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&partial)?;
        let result = write(&mut file).map_err(StageError::Io).and_then(|_| {
            file.flush()?;
            if cancel.load(Ordering::Acquire) { return Err(StageError::Cancelled); }
            self.check_generation(generation)?;
            if fs::metadata(&partial)?.len() == 0 { return Err(StageError::Empty); }
            fs::rename(&partial, &destination)?;
            Ok(StagedVideo { slot, generation, state: StageState::Ready, path: Some(destination) })
        });
        if result.is_err() { let _ = fs::remove_file(&partial); }
        result
    }
    pub fn unavailable(&self, slot: Slot, generation: u64) -> StagedVideo { StagedVideo { slot, generation, state: StageState::Unavailable, path: None } }
    pub fn cleanup(&self, preserve: &[PathBuf]) -> Result<(), StageError> { for entry in fs::read_dir(&self.root)? { let path = entry?.path(); if preserve.contains(&path) { continue; } if path.is_file() { let _ = fs::remove_file(path); } } Ok(()) }
    fn check_generation(&self, generation: u64) -> Result<(), StageError> { if generation != self.generation { Err(StageError::StaleGeneration) } else { Ok(()) } }
    fn destination(&self, slot: Slot, key: &str) -> Result<PathBuf, StageError> { let name = match slot { Slot::Current => "current", Slot::Next => "next" }; let safe = safe_key(key)?; Ok(self.root.join(format!("{name}-{safe}.mp4"))) }
    fn contained(&self, path: &Path) -> bool { path.parent() == Some(self.root.as_path()) && self.library_root.as_ref().map(|r| !path.starts_with(r)).unwrap_or(true) }
}

fn cleanup(root: &Path) -> io::Result<()> { for entry in fs::read_dir(root)? { let path = entry?.path(); if path.is_file() { let _ = fs::remove_file(path); } } Ok(()) }
fn safe_key(key: &str) -> Result<String, StageError> { let safe: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect(); if safe.is_empty() { Err(StageError::InvalidPath) } else { Ok(safe) } }
