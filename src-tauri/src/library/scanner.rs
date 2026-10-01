//! Exact desktop scan semantics. This intentionally does not recurse.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
#[cfg(test)]
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "m4a", "wav", "flac", "mp4", "webm"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

pub trait FileSystem {
    fn read_dir(&self, path: &Path) -> Result<Vec<Entry>, String>;
    fn modified(&self, path: &Path) -> Result<SystemTime, String>;
}

pub struct OsFileSystem;

#[cfg(test)]
type OsScanHook = Arc<dyn Fn(&Path) + Send + Sync>;

#[cfg(test)]
static OS_SCAN_HOOK: OnceLock<Mutex<Option<OsScanHook>>> = OnceLock::new();
#[cfg(test)]
static OS_SCAN_HOOK_SCOPE: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
pub struct OsScanHookGuard {
    _scope: MutexGuard<'static, ()>,
}

#[cfg(test)]
pub fn scoped_os_scan_hook(hook: OsScanHook) -> OsScanHookGuard {
    let scope = OS_SCAN_HOOK_SCOPE
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *OS_SCAN_HOOK.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner()) = Some(hook);
    OsScanHookGuard { _scope: scope }
}

#[cfg(test)]
impl Drop for OsScanHookGuard {
    fn drop(&mut self) {
        *OS_SCAN_HOOK.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

impl FileSystem for OsFileSystem {
    fn read_dir(&self, path: &Path) -> Result<Vec<Entry>, String> {
        #[cfg(test)]
        if let Some(hook) = OS_SCAN_HOOK.get().and_then(|hook| hook.lock().unwrap_or_else(|e| e.into_inner()).clone()) {
            hook(path);
        }
        fs::read_dir(path)
            .map_err(|e| e.to_string())?
            .map(|item| {
                let item = item.map_err(|e| e.to_string())?;
                let file_type = item.file_type().map_err(|e| e.to_string())?;
                Ok(Entry {
                    path: item.path(),
                    name: item.file_name().to_string_lossy().into_owned(),
                    is_dir: file_type.is_dir(),
                })
            })
            .collect()
    }
    fn modified(&self, path: &Path) -> Result<SystemTime, String> {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootStatus {
    Scanned,
    Missing,
    Inaccessible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOutcome {
    pub playlists: BTreeMap<String, Vec<PathBuf>>,
    pub roots: Vec<(PathBuf, RootStatus)>,
    pub errors: Vec<ScanError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanError {
    pub root_index: usize,
    pub playlist: Option<String>,
    pub message: String,
}

fn supported(path: &Path) -> bool {
    if crate::download::workflows::is_intermediate_download(path) { return false; }
    path.extension()
        .and_then(|x| x.to_str())
        .map(|x| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|ext| x.eq_ignore_ascii_case(ext))
        })
        .unwrap_or(false)
}

fn internal_dir(name: &str) -> bool {
    name == ".luno_previews"
        || name == crate::download::workflows::DOWNLOAD_STAGING_DIR
        || name == crate::download::workflows::WORKER_HOME_DIR
        || name == crate::download::workflows::LEGACY_WORKER_HOME_DIR
}

fn filename_key(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn is_missing_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("not found") || error.contains("no such file or directory")
}

pub fn scan_library<F: FileSystem>(roots: &[PathBuf], fs: &F) -> ScanOutcome {
    scan_library_with_cancel(roots, fs, || false)
}

pub fn scan_library_with_cancel<F: FileSystem, C: Fn() -> bool>(
    roots: &[PathBuf],
    fs: &F,
    should_cancel: C,
) -> ScanOutcome {
    let mut playlists = BTreeMap::new();
    let mut statuses = Vec::with_capacity(roots.len());
    let mut errors = Vec::new();
    for (root_index, root) in roots.iter().enumerate() {
        if should_cancel() {
            break;
        }
        let entries = match fs.read_dir(root) {
            Ok(entries) => entries,
            Err(error) => {
                let status = if is_missing_error(&error) {
                    RootStatus::Missing
                } else {
                    RootStatus::Inaccessible
                };
                statuses.push((root.clone(), status));
                errors.push(ScanError {
                    root_index,
                    playlist: None,
                    message: error,
                });
                continue;
            }
        };
        statuses.push((root.clone(), RootStatus::Scanned));
        let mut unsorted = Vec::new();
        let mut folders: Vec<Entry> = entries
            .iter()
            .filter(|e| e.is_dir && !internal_dir(&e.name))
            .cloned()
            .collect();
        folders.sort_by_key(|e| (e.name.to_lowercase(), e.name.clone()));
        for folder in folders {
            if should_cancel() {
                break;
            }
            let child_entries = match fs.read_dir(&folder.path) {
                Ok(entries) => entries,
                Err(error) => {
                    errors.push(ScanError {
                        root_index,
                        playlist: Some(folder.name.clone()),
                        message: error,
                    });
                    playlists.insert(folder.name.clone(), Vec::new());
                    continue;
                }
            };
            let mut songs = child_entries
                .into_iter()
                .filter(|e| !e.is_dir && supported(&e.path))
                .map(|e| e.path)
                .collect::<Vec<_>>();
            songs.sort_by_key(|p| (filename_key(p), p.clone()));
            playlists.insert(folder.name, songs);
        }
        for entry in &entries {
            if !entry.is_dir && supported(&entry.path) {
                unsorted.push(entry.path.clone());
            }
        }
        if !unsorted.is_empty() {
            if should_cancel() {
                break;
            }
            unsorted.sort_by(|a, b| {
                let am = fs.modified(a).unwrap_or_else(|error| {
                    errors.push(ScanError {
                        root_index,
                        playlist: Some("Unsorted".into()),
                        message: error,
                    });
                    SystemTime::UNIX_EPOCH
                });
                let bm = fs.modified(b).unwrap_or_else(|error| {
                    errors.push(ScanError {
                        root_index,
                        playlist: Some("Unsorted".into()),
                        message: error,
                    });
                    SystemTime::UNIX_EPOCH
                });
                bm.cmp(&am)
                    .then_with(|| filename_key(a).cmp(&filename_key(b)))
                    .then_with(|| a.cmp(b))
            });
            let existing = playlists.entry("Unsorted".into()).or_insert_with(Vec::new);
            for path in unsorted {
                if !existing.contains(&path) {
                    existing.push(path);
                }
            }
        }
    }
    ScanOutcome {
        playlists,
        roots: statuses,
        errors,
    }
}
