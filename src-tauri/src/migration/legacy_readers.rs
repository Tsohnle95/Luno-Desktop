//! Tolerant, read-only readers for all legacy desktop state.

use crate::persistence::legacy::{self, LegacyConfig};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_RETRY_ENTRIES: usize = 4096;
pub const MAX_RETRY_DEPTH: usize = 32;

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
    if !safe_directory(dir) { return Ok(Vec::new()); }
    let mut files = Vec::new();
    let mut inspected = 0;
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        if inspected >= MAX_RETRY_ENTRIES {
            return Err(format!("preview inventory exceeded {} entries", MAX_RETRY_ENTRIES));
        }
        inspected += 1;
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() { continue; }
        if metadata.is_file() && contained(dir, &path) { files.push(path); }
    }
    files.sort();
    Ok(files)
}

/// Inventory files under a supported legacy root without interpreting their
/// names. Completed migrations use this to notice preserved inputs that the
/// typed readers do not understand.
pub fn inventory_files(root: &Path, excluded_roots: &[&Path]) -> Result<Vec<PathBuf>, String> {
    if !root.exists() { return Ok(Vec::new()); }
    let mut files = Vec::new();
    inventory_files_inner(root, excluded_roots, &mut files)?;
    files.sort();
    Ok(files)
}

/// Inspect only the bounded legacy surfaces that can contain preserved input.
/// In particular, do not recursively treat HOME as a migration data root.
pub fn inventory_retry_files(legacy_root: &Path, music_root: Option<&Path>, preview_dir: &Path, target_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = BTreeSet::new();
    // Reserve half the inspection budget for each explicit discovery surface so
    // a large HOME envelope cannot starve the configured music root (or vice versa).
    let discovery_budget = MAX_RETRY_ENTRIES / 2;
    let music_budget = MAX_RETRY_ENTRIES - discovery_budget;
    let excluded = [preview_dir, target_root];
    let mut envelope_budget = discovery_budget / 2;
    let mut preview_budget = discovery_budget - envelope_budget;
    inventory_direct(legacy_root, &excluded, &mut files, &mut envelope_budget)?;
    inventory_preview(preview_dir, &mut files, &mut preview_budget)?;
    if let Some(root) = music_root.filter(|root| *root != legacy_root) {
        let mut budget = music_budget;
        inventory_direct(root, &[], &mut files, &mut budget)?;
        let mut marker_roots = Vec::new();
        find_marker_roots(root, 0, &mut marker_roots, &mut budget)?;
        for marker_root in marker_roots {
            inventory_direct(&marker_root, &[], &mut files, &mut budget)?;
        }
    }
    Ok(files.into_iter().collect())
}

fn inventory_preview(root: &Path, files: &mut BTreeSet<PathBuf>, budget: &mut usize) -> Result<(), String> {
    if !safe_directory(root) { return Ok(()); }
    let mut discovered = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        if *budget == 0 { return Err(format!("retry inspection exceeded {} entries", MAX_RETRY_ENTRIES)); }
        *budget -= 1;
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() { continue; }
        if metadata.is_file() && contained(root, &path) { discovered.push(path); }
    }
    discovered.sort();
    files.extend(discovered);
    Ok(())
}

fn inventory_direct(root: &Path, excluded: &[&Path], files: &mut BTreeSet<PathBuf>, budget: &mut usize) -> Result<(), String> {
    if !safe_directory(root) { return Ok(()); }
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        if *budget == 0 { return Err(format!("retry inspection exceeded {} entries", MAX_RETRY_ENTRIES)); }
        *budget -= 1;
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if excluded.iter().any(|excluded| path == *excluded || path.starts_with(excluded)) { continue; }
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() { continue; }
        if metadata.is_file() { files.insert(path); }
    }
    Ok(())
}

fn find_marker_roots(root: &Path, depth: usize, marker_roots: &mut Vec<PathBuf>, budget: &mut usize) -> Result<(), String> {
    if depth > MAX_RETRY_DEPTH || !safe_directory(root) { return Ok(()); }
    let mut has_marker = false;
    let mut directories = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        if *budget == 0 { return Err(format!("retry inspection exceeded {} entries", MAX_RETRY_ENTRIES)); }
        *budget -= 1;
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() { continue; }
        if metadata.is_dir() { directories.push(path); }
        else if matches!(path.file_name().and_then(|name| name.to_str()), Some(".downloaded_vids" | ".sync_failed_vids" | ".downloaded.txt")) { has_marker = true; }
    }
    if has_marker { marker_roots.push(root.to_path_buf()); }
    directories.sort();
    for directory in directories { find_marker_roots(&directory, depth + 1, marker_roots, budget)?; }
    Ok(())
}

fn safe_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).map(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink()).unwrap_or(false)
}

fn inventory_files_inner(root: &Path, excluded_roots: &[&Path], files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if excluded_roots.iter().any(|excluded| path == *excluded || path.starts_with(excluded)) { continue; }
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() { inventory_files_inner(&path, excluded_roots, files)?; }
        else { files.push(path); }
    }
    Ok(())
}

fn has_data_file(path: &Path) -> bool {
    path.is_file() && fs::metadata(path).map(|metadata| metadata.len() > 0).unwrap_or(false)
}

fn has_legacy_files(root: &Path, recursive: bool) -> bool {
    if [".luno_config.json", ".luno_playlist_urls.json", ".luno_allowed_dups.json"].iter().any(|name| has_data_file(&root.join(name))) { return true; }
    if recursive { marker_files(root).map(|files| files.into_iter().any(|path| has_data_file(&path))).unwrap_or(true) }
    else { [".downloaded_vids", ".sync_failed_vids", ".downloaded.txt"].iter().any(|name| has_data_file(&root.join(name))) }
}

/// Find marker files without following symlinks or recursing through an
/// unbounded directory tree. The entry budget is consumed before inspecting
/// each directory entry, so an oversized root fails deterministically.
pub fn marker_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !safe_directory(root) { return Ok(Vec::new()); }
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut inspected = 0;
    while let Some(directory) = pending.pop() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|e| e.to_string())? {
            if inspected >= MAX_RETRY_ENTRIES { return Err(format!("legacy marker inspection exceeded {} entries", MAX_RETRY_ENTRIES)); }
            inspected += 1;
            entries.push(entry.map_err(|e| e.to_string())?);
        }
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() || !contained(root, &path) { continue; }
            if metadata.is_dir() { pending.push(path); }
            else if matches!(path.file_name().and_then(|name| name.to_str()), Some(".downloaded_vids" | ".sync_failed_vids" | ".downloaded.txt")) { files.push(path); }
        }
    }
    files.sort();
    Ok(files)
}

/// Return whether any supported legacy surface contains data worth migrating.
/// Empty marker files and an empty preview directory are not migration triggers.
pub fn has_legacy_data(root: &Path, preview_dir: Option<&Path>) -> bool {
    let config_path = root.join(".luno_config.json");
    let config = legacy::read_config(&config_path).ok();
    if has_legacy_files(root, false) { return true; }
    let music_root = config.and_then(|config| config.music_root).map(PathBuf::from);
    if music_root.as_deref().is_some_and(|path| has_legacy_files(path, true)) { return true; }
    preview_dir.is_some_and(|dir| fs::symlink_metadata(dir).map(|metadata| metadata.file_type().is_symlink()).unwrap_or(false))
        || preview_dir.is_some_and(|dir| match read_preview_inventory(dir) {
            Ok(files) => !files.is_empty(),
            Err(_) => true,
        })
}

/// Best-effort aggregate read. Missing files are normal; corrupt files produce
/// warnings while valid portions remain available to the migration planner.
pub fn read_legacy(root: &Path, preview_dir: Option<&Path>) -> LegacyState {
    let mut state = LegacyState::default();
    macro_rules! read_file { ($path:expr, $reader:expr) => {{ let path = $path; if path.exists() { if let Err(message) = $reader(&path) { state.warnings.push(Warning { path, message }); } } }}; }
    let config_path = root.join(".luno_config.json");
    read_file!(config_path, |p: &Path| { state.config = Some(legacy::read_config(p)?); if let Some(c) = &state.config { if let Some(v) = &c.favorite_songs { state.favorite_paths.extend(v.clone()); } } Ok(()) });
    let roots = [Some(root.to_path_buf()), state.config.as_ref().and_then(|config| config.music_root.as_ref().map(PathBuf::from))];
    for data_root in roots.into_iter().flatten().filter(|candidate| candidate != root) {
        read_file!(data_root.join(".luno_playlist_urls.json"), |p: &Path| { state.playlist_urls.extend(legacy::read_string_map(p)?); Ok(()) });
        read_file!(data_root.join(".luno_allowed_dups.json"), |p: &Path| { state.allowed_duplicate_keys.extend(legacy::read_string_set(p)?.into_iter()); Ok(()) });
        let markers = match marker_files(&data_root) {
            Ok(markers) => markers,
            Err(message) => {
                state.warnings.push(Warning { path: data_root.clone(), message });
                Vec::new()
            }
        };
        for marker in markers {
            let legacy = marker.file_name().and_then(|name| name.to_str()) == Some(".downloaded.txt");
            read_file!(marker, |p: &Path| { if legacy { state.downloaded_ids.extend(read_markers(p, true)?); } else if p.file_name().and_then(|name| name.to_str()) == Some(".sync_failed_vids") { state.failed_ids.extend(read_markers(p, false)?); } else { state.downloaded_ids.extend(read_markers(p, false)?); } Ok(()) });
        }
    }
    read_file!(root.join(".luno_playlist_urls.json"), |p: &Path| { state.playlist_urls = legacy::read_string_map(p)?; Ok(()) });
    read_file!(root.join(".luno_allowed_dups.json"), |p: &Path| { state.allowed_duplicate_keys = legacy::read_string_set(p)?.into_iter().collect(); Ok(()) });
    read_file!(root.join(".downloaded_vids"), |p: &Path| { state.downloaded_ids.extend(read_markers(p, false)?); Ok(()) });
    read_file!(root.join(".sync_failed_vids"), |p: &Path| { state.failed_ids.extend(read_markers(p, false)?); Ok(()) });
    read_file!(root.join(".downloaded.txt"), |p: &Path| { state.downloaded_ids.extend(read_markers(p, true)?); Ok(()) });
    if let Some(dir) = preview_dir { match read_preview_inventory(dir) { Ok(v) => state.preview_files = v, Err(message) => state.warnings.push(Warning { path: dir.to_path_buf(), message }) } }
    state
}
