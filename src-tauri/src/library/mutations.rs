//! Safe mutations of physical playlist folders.

use crate::library::index::{LibraryIndex, RefreshRequest};
use crate::library::metadata::TrackMetadataStore;
use crate::persistence::atomic_write::write_atomic;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

const URL_FILE: &str = ".luno_playlist_urls.json";
const SYNC_SOURCES_FILE: &str = ".luno_playlist_sync_sources.json";
const SYNC_SOURCES_VERSION: u8 = 1;
static PLAYLIST_SOURCE_FILES_LOCK: Mutex<()> = Mutex::new(());
static LIBRARY_FILE_DELETE_LOCK: Mutex<()> = Mutex::new(());
static DELETE_REVIEW_CACHE: OnceLock<Mutex<HashMap<String, PendingDeleteReview>>> = OnceLock::new();
const DELETE_REVIEW_TTL: Duration = Duration::from_secs(600);
const DELETE_REVIEW_LIMIT: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaylistSyncSourcesFile {
    version: u8,
    playlists: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum MutationError {
    #[error("playlist name is empty or invalid")] InvalidName,
    #[error("playlist '{0}' is protected")] Protected(String),
    #[error("playlist '{0}' already exists")] Collision(String),
    #[error("playlist '{0}' was not found")] NotFound(String),
    #[error("invalid playlist URL")] InvalidUrl,
    #[error("deleting the final copy requires a second confirmation")] ConfirmationRequired,
    #[error("track path is not a regular file inside the configured library")] UnsafePath,
    #[error("filesystem error at {path}: {message}")] Io { path: PathBuf, message: String },
    #[error("{0} files could not be copied")] PartialCopy(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationResult<T> { pub value: T, pub refresh: RefreshRequest, pub errors: Vec<MutationError> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyResult { pub copied: usize, pub failed: usize, pub copied_paths: Vec<(PathBuf, PathBuf)> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeDeleteResult {
    pub deleted: usize,
    pub needs_confirmation: bool,
    pub verification_incomplete: bool,
    pub final_copy_review_id: Option<String>,
    pub final_copies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FinalCopyStamp { path: PathBuf, length: u64, modified: Option<SystemTime> }

#[derive(Debug, Clone)]
struct PendingDeleteReview {
    issued_at: Instant,
    roots: Vec<PathBuf>,
    requested: Vec<PathBuf>,
    final_copies: Vec<FinalCopyStamp>,
    verification_incomplete: bool,
}

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
        let outcome = self.empty_with_review(name, std::slice::from_ref(&self.root), None)?;
        if outcome.needs_confirmation { return Err(MutationError::ConfirmationRequired); }
        Ok(self.result(outcome.deleted, index))
    }
    pub fn delete(&self, name: &str, index: &mut LibraryIndex) -> Result<MutationResult<()>, MutationError> {
        let outcome = self.delete_with_review(name, std::slice::from_ref(&self.root), None)?;
        if outcome.needs_confirmation { return Err(MutationError::ConfirmationRequired); }
        Ok(self.result((), index))
    }
    pub fn empty_with_review(&self, name: &str, roots: &[PathBuf], review_id: Option<&str>) -> Result<SafeDeleteResult, MutationError> {
        let _delete_guard = LIBRARY_FILE_DELETE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let path = self.playlist_path(name)?;
        let files = playlist_audio_files(&path)?;
        delete_files_locked(&files, roots, review_id)
    }
    pub fn delete_with_review(&self, name: &str, roots: &[PathBuf], review_id: Option<&str>) -> Result<SafeDeleteResult, MutationError> {
        let safe = sanitize_name(name).ok_or(MutationError::InvalidName)?;
        let _delete_guard = LIBRARY_FILE_DELETE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let _sources_guard = PLAYLIST_SOURCE_FILES_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let path = self.playlist_path(&safe)?;
        let files = playlist_audio_files(&path)?;
        let mut outcome = delete_files_locked(&files, roots, review_id)?;
        if outcome.needs_confirmation { return Ok(outcome); }
        fs::remove_dir_all(&path).map_err(|e| io_error(&path, e))?;
        let mut sources = self.read_sync_sources()?;
        let mut urls = self.read_urls()?;
        if sources.playlists.remove(&safe).is_some() { self.write_sync_sources(&sources)?; }
        if urls.remove(&safe).is_some() { self.write_urls(&urls)?; }
        outcome.deleted = files.len();
        Ok(outcome)
    }
    pub fn set_url(&self, name: &str, url: Option<&str>, index: &mut LibraryIndex) -> Result<MutationResult<Option<String>>, MutationError> {
        let values = url.map(str::trim).filter(|value| !value.is_empty()).map(|value| vec![value.to_owned()]).unwrap_or_default();
        let result = self.set_urls(name, &values, index)?;
        Ok(MutationResult { value: result.value.into_iter().next(), refresh: result.refresh, errors: result.errors })
    }
    pub fn set_urls(&self, name: &str, values: &[String], index: &mut LibraryIndex) -> Result<MutationResult<Vec<String>>, MutationError> {
        let safe = sanitize_name(name).ok_or(MutationError::InvalidName)?;
        let _sources_guard = PLAYLIST_SOURCE_FILES_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        self.playlist_path(&safe)?;
        let mut normalized = Vec::new();
        for value in values.iter().map(|value| value.trim()).filter(|value| !value.is_empty()) {
            let Some((scheme, host)) = value.split_once("://") else { return Err(MutationError::InvalidUrl) };
            if !matches!(scheme, "http" | "https") || host.is_empty() { return Err(MutationError::InvalidUrl); }
            if !normalized.iter().any(|existing| existing == value) { normalized.push(value.to_owned()); }
        }

        let mut sources = self.read_sync_sources()?;
        let mut legacy_urls = self.read_urls()?;
        for (playlist, url) in legacy_urls.iter() {
            sources.playlists.entry(playlist.clone()).or_insert_with(|| vec![url.clone()]);
        }
        sources.playlists.insert(safe.clone(), normalized.clone());
        self.write_sync_sources(&sources)?;
        match normalized.first() {
            Some(first) => { legacy_urls.insert(safe, first.clone()); },
            None => { legacy_urls.remove(&safe); },
        }
        self.write_urls(&legacy_urls)?;
        Ok(self.result(normalized, index))
    }
    pub fn playlist_urls(&self) -> Result<BTreeMap<String, String>, MutationError> {
        let mut urls = self.read_urls()?;
        for (name, sources) in self.read_sync_sources()?.playlists {
            match sources.first() {
                Some(first) => { urls.insert(name, first.clone()); },
                None => { urls.remove(&name); },
            }
        }
        Ok(urls)
    }
    pub fn playlist_sync_sources(&self, name: &str) -> Result<Vec<String>, MutationError> {
        let safe = sanitize_name(name).ok_or(MutationError::InvalidName)?; self.check_name(&safe)?;
        if let Some(sources) = self.read_sync_sources()?.playlists.get(&safe) { return Ok(sources.clone()); }
        Ok(self.read_urls()?.get(&safe).cloned().into_iter().collect())
    }
    pub fn dump(&self, source: &str, destination: &str, index: &mut LibraryIndex) -> Result<MutationResult<CopyResult>, MutationError> {
        let _delete_guard = LIBRARY_FILE_DELETE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let source_path = self.playlist_path(source)?; let destination_path = self.playlist_path(destination)?;
        if source_path == destination_path { return Err(MutationError::Collision(destination.to_owned())); }
        let mut copied = 0; let mut errors = Vec::new(); let mut copied_paths = Vec::new();
        let mut metadata = match TrackMetadataStore::load(&self.root) {
            Ok(store) => Some(store),
            Err(error) => { errors.push(io_error(&self.root.join(crate::library::metadata::FILE_NAME), error)); None }
        };
        let initial_metadata = metadata.clone();
        for entry in fs::read_dir(&source_path).map_err(|e| io_error(&source_path, e))? {
            let entry = entry.map_err(|e| io_error(&source_path, e))?; let child = entry.path();
            if !entry.file_type().map_err(|e| io_error(&child, e))?.is_file() { continue; }
            let target = destination_path.join(entry.file_name());
            if target.exists() { errors.push(MutationError::Collision(target.display().to_string())); continue; }
            match fs::copy(&child, &target) {
                Ok(_) => {
                    copied += 1;
                    copied_paths.push((child.clone(), target.clone()));
                    if let Some(store) = metadata.as_mut() {
                        if let Err(error) = store.copy_path(&self.root, &child, &target) {
                            errors.push(io_error(&self.root.join(crate::library::metadata::FILE_NAME), error));
                        }
                    }
                }
                Err(e) => errors.push(io_error(&target, e)),
            }
        }
        if metadata != initial_metadata {
            if let Some(store) = metadata.as_ref() {
                if let Err(error) = store.save(&self.root) {
                    errors.push(io_error(&self.root.join(crate::library::metadata::FILE_NAME), error));
                }
            }
        }
        let failed = errors.len(); let mut result = self.result_with_errors(CopyResult { copied, failed, copied_paths }, errors, index); if failed > 0 { result.errors.push(MutationError::PartialCopy(failed)); } Ok(result)
    }
    /// Copy selected library files into a physical playlist folder without
    /// overwriting same-named audio that is already there.
    pub fn add_files(&self, files: &[PathBuf], destination: &str, index: &mut LibraryIndex) -> Result<MutationResult<CopyResult>, MutationError> {
        let _delete_guard = LIBRARY_FILE_DELETE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let destination_path = self.playlist_path(destination)?; let mut copied = 0; let mut errors = Vec::new(); let mut copied_paths = Vec::new();
        let mut metadata = match TrackMetadataStore::load(&self.root) {
            Ok(store) => Some(store),
            Err(error) => { errors.push(io_error(&self.root.join(crate::library::metadata::FILE_NAME), error)); None }
        };
        let initial_metadata = metadata.clone();
        for source in files {
            let Some(name) = source.file_name() else { continue }; let target = destination_path.join(name);
            if target.exists() { continue; }
            match fs::copy(source, &target) {
                Ok(_) => {
                    copied += 1;
                    copied_paths.push((source.clone(), target.clone()));
                    if let Some(store) = metadata.as_mut() {
                        if let Err(error) = store.copy_path(&self.root, source, &target) {
                            errors.push(io_error(&self.root.join(crate::library::metadata::FILE_NAME), error));
                        }
                    }
                }
                Err(error) => errors.push(io_error(&target, error)),
            }
        }
        if metadata != initial_metadata {
            if let Some(store) = metadata.as_ref() {
                if let Err(error) = store.save(&self.root) {
                    errors.push(io_error(&self.root.join(crate::library::metadata::FILE_NAME), error));
                }
            }
        }
        let failed = errors.len(); let mut result = self.result_with_errors(CopyResult { copied, failed, copied_paths }, errors, index); if failed > 0 { result.errors.push(MutationError::PartialCopy(failed)); } Ok(result)
    }
    fn playlist_path(&self, name: &str) -> Result<PathBuf, MutationError> {
        let safe = sanitize_name(name).ok_or(MutationError::InvalidName)?;
        self.check_name(&safe)?;
        let path = self.root.join(&safe);
        let metadata = fs::symlink_metadata(&path).map_err(|error| if error.kind() == io::ErrorKind::NotFound { MutationError::NotFound(safe.clone()) } else { io_error(&path, error) })?;
        if metadata.file_type().is_symlink() { return Err(MutationError::UnsafePath); }
        if !metadata.is_dir() { return Err(MutationError::NotFound(safe)); }
        let root = fs::canonicalize(&self.root).map_err(|error| io_error(&self.root, error))?;
        let canonical = fs::canonicalize(&path).map_err(|error| io_error(&path, error))?;
        if !canonical.starts_with(root) { return Err(MutationError::UnsafePath); }
        Ok(path)
    }
    fn check_name(&self, name: &str) -> Result<(), MutationError> { let root = self.root.file_name().and_then(|n| n.to_str()).unwrap_or(""); if ["All Music", "Unsorted", "Luno_Library", root].contains(&name) { Err(MutationError::Protected(name.to_owned())) } else { Ok(()) } }
    fn result<T>(&self, value: T, index: &mut LibraryIndex) -> MutationResult<T> { self.result_with_errors(value, Vec::new(), index) }
    fn result_with_errors<T>(&self, value: T, errors: Vec<MutationError>, index: &mut LibraryIndex) -> MutationResult<T> { MutationResult { value, errors, refresh: index.request_refresh() } }
    fn read_urls(&self) -> Result<BTreeMap<String, String>, MutationError> { let path = self.root.join(URL_FILE); if !path.exists() { return Ok(BTreeMap::new()); } serde_json::from_slice(&fs::read(&path).map_err(|e| io_error(&path, e))?).map_err(|e| io_error(&path, io::Error::other(e))) }
    fn write_urls(&self, urls: &BTreeMap<String, String>) -> Result<(), MutationError> { let path = self.root.join(URL_FILE); let bytes = serde_json::to_vec_pretty(urls).map_err(io::Error::other).map_err(|e| io_error(&path, e))?; write_atomic(&path, &bytes).map(|_| ()).map_err(|e| io_error(&path, e)) }
    fn read_sync_sources(&self) -> Result<PlaylistSyncSourcesFile, MutationError> {
        let path = self.root.join(SYNC_SOURCES_FILE);
        if !path.exists() { return Ok(PlaylistSyncSourcesFile { version: SYNC_SOURCES_VERSION, playlists: BTreeMap::new() }); }
        let value: PlaylistSyncSourcesFile = serde_json::from_slice(&fs::read(&path).map_err(|e| io_error(&path, e))?)
            .map_err(|e| io_error(&path, io::Error::other(e)))?;
        if value.version != SYNC_SOURCES_VERSION { return Err(io_error(&path, io::Error::other("unsupported playlist sync source file version"))); }
        Ok(value)
    }
    fn write_sync_sources(&self, sources: &PlaylistSyncSourcesFile) -> Result<(), MutationError> {
        let path = self.root.join(SYNC_SOURCES_FILE);
        let bytes = serde_json::to_vec_pretty(sources).map_err(io::Error::other).map_err(|e| io_error(&path, e))?;
        write_atomic(&path, &bytes).map(|_| ()).map_err(|e| io_error(&path, e))
    }
}

pub fn delete_files_safely(paths: &[PathBuf], roots: &[PathBuf], review_id: Option<&str>) -> Result<SafeDeleteResult, MutationError> {
    let _delete_guard = LIBRARY_FILE_DELETE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    delete_files_locked(paths, roots, review_id)
}

fn delete_files_locked(paths: &[PathBuf], roots: &[PathBuf], review_id: Option<&str>) -> Result<SafeDeleteResult, MutationError> {
    if paths.is_empty() {
        return Ok(SafeDeleteResult { deleted: 0, needs_confirmation: false, verification_incomplete: false, final_copy_review_id: None, final_copies: Vec::new() });
    }
    let (roots, mut verification_incomplete) = canonical_roots(roots)?;
    let mut requested = paths.iter().map(|path| canonical_file(path, &roots)).collect::<Result<Vec<_>, _>>()?;
    requested.sort();
    requested.dedup();
    let requested_set: HashSet<PathBuf> = requested.iter().cloned().collect();

    let scan = crate::library::scanner::scan_library(&roots, &crate::library::scanner::OsFileSystem);
    verification_incomplete |= !scan.errors.is_empty()
        || scan.roots.iter().any(|(_, status)| *status != crate::library::scanner::RootStatus::Scanned);
    let mut library_files = HashSet::new();
    for path in scan.playlists.into_values().flatten() {
        match canonical_file(&path, &roots) {
            Ok(path) => { library_files.insert(path); }
            Err(_) => verification_incomplete = true,
        }
    }
    library_files.extend(requested.iter().cloned());

    let mut final_copies = Vec::new();
    for path in &requested {
        let metadata = fs::metadata(path).map_err(|error| io_error(path, error))?;
        let file_name = path.file_name().ok_or(MutationError::UnsafePath)?;
        let mut has_identical_copy_outside_request = false;
        for candidate in &library_files {
            if candidate == path || requested_set.contains(candidate) || candidate.file_name() != Some(file_name) { continue; }
            let candidate_metadata = match fs::metadata(candidate) {
                Ok(metadata) => metadata,
                Err(_) => { verification_incomplete = true; continue; }
            };
            if !candidate_metadata.is_file() || candidate_metadata.len() != metadata.len() { continue; }
            match same_file_contents(path, candidate) {
                Ok(true) => { has_identical_copy_outside_request = true; break; }
                Ok(false) => {}
                Err(_) => verification_incomplete = true,
            }
        }
        if !has_identical_copy_outside_request {
            final_copies.push(FinalCopyStamp { path: path.clone(), length: metadata.len(), modified: metadata.modified().ok() });
            if final_copies.last().is_some_and(|copy| copy.modified.is_none()) { verification_incomplete = true; }
        }
    }

    if !final_copies.is_empty() {
        let approved = review_id.and_then(take_delete_review).is_some_and(|review| {
            review.roots == roots
                && review.requested == requested
                && review.final_copies == final_copies
                && review.verification_incomplete == verification_incomplete
        });
        if !approved {
            let labels = final_copies.iter().map(|copy| relative_file_label(&copy.path, &roots)).collect::<Vec<_>>();
            let review_id = store_delete_review(PendingDeleteReview {
                issued_at: Instant::now(), roots, requested, final_copies, verification_incomplete,
            })?;
            return Ok(SafeDeleteResult { deleted: 0, needs_confirmation: true, verification_incomplete, final_copy_review_id: Some(review_id), final_copies: labels });
        }
    }

    let mut deleted = 0;
    for path in &requested {
        fs::remove_file(path).map_err(|error| io_error(path, error))?;
        deleted += 1;
    }
    Ok(SafeDeleteResult { deleted, needs_confirmation: false, verification_incomplete: false, final_copy_review_id: None, final_copies: Vec::new() })
}

fn canonical_roots(roots: &[PathBuf]) -> Result<(Vec<PathBuf>, bool), MutationError> {
    let mut canonical = Vec::new();
    let mut incomplete = false;
    for root in roots {
        match fs::canonicalize(root) {
            Ok(path) if path.is_dir() => canonical.push(path),
            Ok(_) | Err(_) => incomplete = true,
        }
    }
    canonical.sort();
    canonical.dedup();
    if canonical.is_empty() { return Err(MutationError::NotFound("configured library root".into())); }
    Ok((canonical, incomplete))
}

fn canonical_file(path: &Path, roots: &[PathBuf]) -> Result<PathBuf, MutationError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() { return Err(MutationError::UnsafePath); }
    let canonical = fs::canonicalize(path).map_err(|error| io_error(path, error))?;
    if !roots.iter().any(|root| canonical.starts_with(root)) { return Err(MutationError::UnsafePath); }
    Ok(canonical)
}

fn playlist_audio_files(folder: &Path) -> Result<Vec<PathBuf>, MutationError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(folder).map_err(|error| io_error(folder, error))? {
        let entry = entry.map_err(|error| io_error(folder, error))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|error| io_error(&path, error))?;
        if kind.is_file() && crate::library::scanner::is_scannable_audio(&path) { files.push(path); }
    }
    Ok(files)
}

fn same_file_contents(left: &Path, right: &Path) -> io::Result<bool> {
    let mut left = File::open(left)?;
    let mut right = File::open(right)?;
    let mut left_buffer = [0_u8; 64 * 1024];
    let mut right_buffer = [0_u8; 64 * 1024];
    loop {
        let left_count = left.read(&mut left_buffer)?;
        let right_count = right.read(&mut right_buffer)?;
        if left_count != right_count || left_buffer[..left_count] != right_buffer[..right_count] { return Ok(false); }
        if left_count == 0 { return Ok(true); }
    }
}

fn relative_file_label(path: &Path, roots: &[PathBuf]) -> String {
    let relative = roots.iter().find_map(|root| path.strip_prefix(root).ok()).unwrap_or(path);
    relative.to_string_lossy().replace('\\', "/")
}

fn take_delete_review(review_id: &str) -> Option<PendingDeleteReview> {
    let mut reviews = DELETE_REVIEW_CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|error| error.into_inner());
    reviews.retain(|_, review| review.issued_at.elapsed() <= DELETE_REVIEW_TTL);
    reviews.remove(review_id)
}

fn store_delete_review(review: PendingDeleteReview) -> Result<String, MutationError> {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes).map_err(|error| io_error(Path::new("delete review"), io::Error::other(error.to_string())))?;
    let review_id = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    let mut reviews = DELETE_REVIEW_CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|error| error.into_inner());
    reviews.retain(|_, old| old.issued_at.elapsed() <= DELETE_REVIEW_TTL);
    while reviews.len() >= DELETE_REVIEW_LIMIT {
        let oldest = reviews.iter().min_by_key(|(_, old)| old.issued_at).map(|(id, _)| id.clone());
        let Some(oldest) = oldest else { break; };
        reviews.remove(&oldest);
    }
    reviews.insert(review_id.clone(), review);
    Ok(review_id)
}

fn sanitize_name(name: &str) -> Option<String> { let clean: String = name.chars().filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c)).collect(); let clean = clean.trim().trim_end_matches('.').trim().to_owned(); (!clean.is_empty() && clean != "." && clean != "..").then_some(clean) }
fn io_error(path: &Path, error: io::Error) -> MutationError { MutationError::Io { path: path.to_path_buf(), message: error.to_string() } }
