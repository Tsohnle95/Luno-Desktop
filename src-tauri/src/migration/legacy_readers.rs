//! Tolerant, read-only readers for all legacy desktop state.

use crate::persistence::legacy::{self, LegacyConfig};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning { pub path: PathBuf, pub message: String }

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LegacyState {
    pub config: Option<LegacyConfig>,
    pub playlist_urls: BTreeMap<String, String>,
    pub allowed_duplicate_keys: BTreeSet<String>,
    pub favorite_paths: Vec<String>,
    pub downloaded_ids: BTreeSet<String>,
    pub failed_ids: BTreeSet<String>,
    pub preview_files: Vec<PathBuf>,
    pub warnings: Vec<Warning>,
}

fn text_lines(path: &Path) -> Result<Vec<String>, String> {
    let text = String::from_utf8(fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(text.lines().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect())
}

/// Read marker IDs. Legacy `.downloaded.txt` lines use the final whitespace
/// token; current success markers use one ID per line and failures use ID<TAB>reason.
pub fn read_markers(path: &Path, legacy: bool) -> Result<BTreeSet<String>, String> {
    let mut result = BTreeSet::new();
    for line in text_lines(path)? {
        let id = if legacy { line.split_whitespace().last().unwrap_or("") } else { line.split('\t').next().unwrap_or("") };
        if !id.is_empty() && !id.contains('/') && !id.contains('\\') { result.insert(id.to_owned()); }
    }
    Ok(result)
}

fn contained(root: &Path, child: &Path) -> bool {
    let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let child = fs::canonicalize(child).unwrap_or_else(|_| child.to_path_buf());
    child.starts_with(root)
}

pub fn read_preview_inventory(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !dir.exists() { return Ok(Vec::new()); }
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if entry.file_type().map_err(|e| e.to_string())?.is_file() && contained(dir, &path) { files.push(path); }
    }
    files.sort();
    Ok(files)
}

/// Best-effort aggregate read. Missing files are normal; corrupt files produce
/// warnings while valid portions remain available to the migration planner.
pub fn read_legacy(root: &Path, preview_dir: Option<&Path>) -> LegacyState {
    let mut state = LegacyState::default();
    macro_rules! read_file { ($path:expr, $reader:expr) => {{ let path = $path; if path.exists() { if let Err(message) = $reader(&path) { state.warnings.push(Warning { path, message }); } } }}; }
    let config_path = root.join(".luno_config.json");
    read_file!(config_path, |p: &Path| { state.config = Some(legacy::read_config(p)?); if let Some(c) = &state.config { if let Some(v) = &c.favorite_songs { state.favorite_paths.extend(v.clone()); } } Ok(()) });
    read_file!(root.join(".luno_playlist_urls.json"), |p: &Path| { state.playlist_urls = legacy::read_string_map(p)?; Ok(()) });
    read_file!(root.join(".luno_allowed_dups.json"), |p: &Path| { state.allowed_duplicate_keys = legacy::read_string_set(p)?.into_iter().collect(); Ok(()) });
    read_file!(root.join(".downloaded_vids"), |p: &Path| { state.downloaded_ids.extend(read_markers(p, false)?); Ok(()) });
    read_file!(root.join(".sync_failed_vids"), |p: &Path| { state.failed_ids.extend(read_markers(p, false)?); Ok(()) });
    read_file!(root.join(".downloaded.txt"), |p: &Path| { state.downloaded_ids.extend(read_markers(p, true)?); Ok(()) });
    if let Some(dir) = preview_dir { match read_preview_inventory(dir) { Ok(v) => state.preview_files = v, Err(message) => state.warnings.push(Warning { path: dir.to_path_buf(), message }) } }
    state
}
