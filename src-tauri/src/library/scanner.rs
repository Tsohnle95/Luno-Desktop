//! Exact desktop scan semantics. This intentionally does not recurse.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "m4a", "wav", "flac", "mp4", "webm"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry { pub path: PathBuf, pub name: String, pub is_dir: bool }

pub trait FileSystem {
    fn read_dir(&self, path: &Path) -> Result<Vec<Entry>, String>;
    fn modified(&self, path: &Path) -> Result<SystemTime, String>;
}

pub struct OsFileSystem;

impl FileSystem for OsFileSystem {
    fn read_dir(&self, path: &Path) -> Result<Vec<Entry>, String> {
        fs::read_dir(path).map_err(|e| e.to_string())?.map(|item| {
            let item = item.map_err(|e| e.to_string())?;
            let file_type = item.file_type().map_err(|e| e.to_string())?;
            Ok(Entry { path: item.path(), name: item.file_name().to_string_lossy().into_owned(), is_dir: file_type.is_dir() })
        }).collect()
    }
    fn modified(&self, path: &Path) -> Result<SystemTime, String> { fs::metadata(path).and_then(|m| m.modified()).map_err(|e| e.to_string()) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootStatus { Scanned, Missing, Inaccessible }

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
    path.extension().and_then(|x| x.to_str()).map(|x| SUPPORTED_EXTENSIONS.iter().any(|ext| x.eq_ignore_ascii_case(ext))).unwrap_or(false)
}

fn preview_dir(name: &str) -> bool { name == ".luno_previews" }

fn filename_key(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn scan_library<F: FileSystem>(roots: &[PathBuf], fs: &F) -> ScanOutcome {
    let mut playlists = BTreeMap::new();
    let mut statuses = Vec::with_capacity(roots.len());
    let mut errors = Vec::new();
    for (root_index, root) in roots.iter().enumerate() {
        let entries = match fs.read_dir(root) {
            Ok(entries) => entries,
            Err(error) => {
                let status = if error.to_ascii_lowercase().contains("not found") { RootStatus::Missing } else { RootStatus::Inaccessible };
                statuses.push((root.clone(), status));
                errors.push(ScanError { root_index, playlist: None, message: error });
                continue;
            }
        };
        statuses.push((root.clone(), RootStatus::Scanned));
        let mut unsorted = Vec::new();
        let mut folders: Vec<Entry> = entries.iter().filter(|e| e.is_dir && !preview_dir(&e.name)).cloned().collect();
        folders.sort_by_key(|e| (e.name.to_lowercase(), e.name.clone()));
        for folder in folders {
            let child_entries = match fs.read_dir(&folder.path) {
                Ok(entries) => entries,
                Err(error) => {
                    errors.push(ScanError { root_index, playlist: Some(folder.name.clone()), message: error });
                    playlists.insert(folder.name.clone(), Vec::new());
                    continue;
                }
            };
            let mut songs = child_entries.into_iter()
                .filter(|e| !e.is_dir && supported(&e.path)).map(|e| e.path).collect::<Vec<_>>();
            songs.sort_by_key(|p| (filename_key(p), p.clone()));
            playlists.insert(folder.name, songs);
        }
        for entry in &entries {
            if !entry.is_dir && supported(&entry.path) { unsorted.push(entry.path.clone()); }
        }
        if !unsorted.is_empty() {
            unsorted.sort_by(|a, b| {
                let am = fs.modified(a).unwrap_or_else(|error| { errors.push(ScanError { root_index, playlist: Some("Unsorted".into()), message: error }); SystemTime::UNIX_EPOCH });
                let bm = fs.modified(b).unwrap_or_else(|error| { errors.push(ScanError { root_index, playlist: Some("Unsorted".into()), message: error }); SystemTime::UNIX_EPOCH });
                bm.cmp(&am).then_with(|| filename_key(a).cmp(&filename_key(b))).then_with(|| a.cmp(b))
            });
            let existing = playlists.entry("Unsorted".into()).or_insert_with(Vec::new);
            for path in unsorted {
                if !existing.contains(&path) { existing.push(path); }
            }
        }
    }
    ScanOutcome { playlists, roots: statuses, errors }
}
