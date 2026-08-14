//! Safe mutations of physical playlist folders.

use crate::library::index::{LibraryIndex, RefreshRequest};
use crate::persistence::atomic_write::write_atomic;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const URL_FILE: &str = ".luno_playlist_urls.json";

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum MutationError {
    #[error("playlist name is empty or invalid")] InvalidName,
    #[error("playlist '{0}' is protected")] Protected(String),
    #[error("playlist '{0}' already exists")] Collision(String),
    #[error("playlist '{0}' was not found")] NotFound(String),
    #[error("invalid playlist URL")] InvalidUrl,
    #[error("filesystem error at {path}: {message}")] Io { path: PathBuf, message: String },
    #[error("{0} files could not be copied")] PartialCopy(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationResult<T> { pub value: T, pub refresh: RefreshRequest, pub errors: Vec<MutationError> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyResult { pub copied: usize, pub failed: usize }

#[derive(Debug, Clone)]
pub struct PlaylistService { root: PathBuf }

impl PlaylistService {
    pub fn new(root: impl Into<PathBuf>) -> Self { Self { root: root.into() } }
    pub fn create(&self, name: &str, index: &mut LibraryIndex) -> Result<MutationResult<PathBuf>, MutationError> {
        let safe = sanitize_name(name).ok_or(MutationError::InvalidName)?; self.check_name(&safe)?;
        fs::create_dir_all(&self.root).map_err(|e| io_error(&self.root, e))?;
        let path = self.root.join(&safe);
        if path.exists() { return Err(MutationError::Collision(safe)); }
        fs::create_dir(&path).map_err(|e| io_error(&path, e))?;
        Ok(self.result(path, index))
    }
    pub fn empty(&self, name: &str, index: &mut LibraryIndex) -> Result<MutationResult<usize>, MutationError> {
        let path = self.playlist_path(name)?; let mut removed = 0; let mut errors = Vec::new();
        for entry in fs::read_dir(&path).map_err(|e| io_error(&path, e))? {
            let entry = entry.map_err(|e| io_error(&path, e))?; let child = entry.path();
            if entry.file_type().map_err(|e| io_error(&child, e))?.is_file() { match fs::remove_file(&child) { Ok(()) => removed += 1, Err(e) => errors.push(io_error(&child, e)) } }
        }
        Ok(self.result_with_errors(removed, errors, index))
    }
    pub fn delete(&self, name: &str, index: &mut LibraryIndex) -> Result<MutationResult<()>, MutationError> {
        let safe = sanitize_name(name).ok_or(MutationError::InvalidName)?;
        let path = self.playlist_path(&safe)?; fs::remove_dir_all(&path).map_err(|e| io_error(&path, e))?;
        let mut urls = self.read_urls()?; if urls.remove(&safe).is_some() { self.write_urls(&urls)?; }
        Ok(self.result((), index))
    }
    pub fn set_url(&self, name: &str, url: Option<&str>, index: &mut LibraryIndex) -> Result<MutationResult<Option<String>>, MutationError> {
        let safe = sanitize_name(name).ok_or(MutationError::InvalidName)?; self.check_name(&safe)?;
        let value = url.map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
        if let Some(ref value) = value { let Some((scheme, host)) = value.split_once("://") else { return Err(MutationError::InvalidUrl) }; if !matches!(scheme, "http" | "https") || host.is_empty() { return Err(MutationError::InvalidUrl); } }
        let mut urls = self.read_urls()?; match &value { Some(v) => { urls.insert(safe, v.clone()); }, None => { urls.remove(&safe); } }
        self.write_urls(&urls)?; Ok(self.result(value, index))
    }
    pub fn dump(&self, source: &str, destination: &str, index: &mut LibraryIndex) -> Result<MutationResult<CopyResult>, MutationError> {
        let source_path = self.playlist_path(source)?; let destination_path = self.playlist_path(destination)?;
        if source_path == destination_path { return Err(MutationError::Collision(destination.to_owned())); }
        let mut copied = 0; let mut errors = Vec::new();
        for entry in fs::read_dir(&source_path).map_err(|e| io_error(&source_path, e))? {
            let entry = entry.map_err(|e| io_error(&source_path, e))?; let child = entry.path();
            if !entry.file_type().map_err(|e| io_error(&child, e))?.is_file() { continue; }
            let target = destination_path.join(entry.file_name());
            if target.exists() { errors.push(MutationError::Collision(target.display().to_string())); continue; }
            match fs::copy(&child, &target) { Ok(_) => copied += 1, Err(e) => errors.push(io_error(&target, e)) }
        }
        let failed = errors.len(); let mut result = self.result_with_errors(CopyResult { copied, failed }, errors, index); if failed > 0 { result.errors.push(MutationError::PartialCopy(failed)); } Ok(result)
    }
    fn playlist_path(&self, name: &str) -> Result<PathBuf, MutationError> { let safe = sanitize_name(name).ok_or(MutationError::InvalidName)?; self.check_name(&safe)?; let path = self.root.join(&safe); if !path.exists() || !path.is_dir() { return Err(MutationError::NotFound(safe)); } Ok(path) }
    fn check_name(&self, name: &str) -> Result<(), MutationError> { let root = self.root.file_name().and_then(|n| n.to_str()).unwrap_or(""); if ["All Music", "Unsorted", "Luno_Library", root].contains(&name) { Err(MutationError::Protected(name.to_owned())) } else { Ok(()) } }
    fn result<T>(&self, value: T, index: &mut LibraryIndex) -> MutationResult<T> { self.result_with_errors(value, Vec::new(), index) }
    fn result_with_errors<T>(&self, value: T, errors: Vec<MutationError>, index: &mut LibraryIndex) -> MutationResult<T> { MutationResult { value, errors, refresh: index.request_refresh() } }
    fn read_urls(&self) -> Result<BTreeMap<String, String>, MutationError> { let path = self.root.join(URL_FILE); if !path.exists() { return Ok(BTreeMap::new()); } serde_json::from_slice(&fs::read(&path).map_err(|e| io_error(&path, e))?).map_err(|e| io_error(&path, io::Error::other(e))) }
    fn write_urls(&self, urls: &BTreeMap<String, String>) -> Result<(), MutationError> { let path = self.root.join(URL_FILE); let bytes = serde_json::to_vec_pretty(urls).map_err(io::Error::other).map_err(|e| io_error(&path, e))?; write_atomic(&path, &bytes).map(|_| ()).map_err(|e| io_error(&path, e)) }
}

fn sanitize_name(name: &str) -> Option<String> { let clean: String = name.chars().filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c)).collect(); let clean = clean.trim().trim_end_matches('.').trim().to_owned(); (!clean.is_empty() && clean != "." && clean != "..").then_some(clean) }
fn io_error(path: &Path, error: io::Error) -> MutationError { MutationError::Io { path: path.to_path_buf(), message: error.to_string() } }
