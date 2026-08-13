//! First-run migration boundary for the macOS desktop rewrite.
//!
//! This module is deliberately additive. Legacy files are inputs and are never
//! removed or rewritten. A migration can therefore be retried, or the Python
//! release can be launched again, after any failure.

use crate::migration::legacy_readers::{read_legacy, LegacyState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MIGRATION_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct MigrationPaths {
    pub legacy_root: PathBuf,
    pub target_root: PathBuf,
    pub preview_dir: PathBuf,
    pub current_preview: Vec<PathBuf>,
    pub promoted_assets: Vec<PathBuf>,
}

impl MigrationPaths {
    pub fn version_record(&self) -> PathBuf { self.target_root.join("migration-version.json") }
    pub fn state_file(&self) -> PathBuf { self.target_root.join("settings-v1.json") }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashStage { Preflight, Snapshotted, Converted, VersionRecorded, PreviewCleaned }

#[derive(Debug, Clone, Default)]
pub struct MigrationOptions { pub crash_at: Option<CrashStage> }

pub trait Keychain {
    fn import_lastfm_key(&mut self, key: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionRecord { pub migration: String, pub version: u32, pub completed: bool }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigratedSettings {
    pub schema_version: u32,
    pub music_root: Option<String>,
    pub auto_discovery: Option<bool>,
    pub volume: Option<f64>,
    pub sleep_timer: Option<serde_json::Value>,
    pub global_keys: Option<bool>,
    pub favorites: Vec<String>,
    pub playlist_urls: BTreeMap<String, String>,
    pub allowed_duplicate_keys: Vec<String>,
    pub downloaded_ids: Vec<String>,
    pub failed_ids: Vec<String>,
    pub history: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReceipt {
    pub version: PathBuf,
    pub state: PathBuf,
    pub snapshot: PathBuf,
    pub removed_previews: Vec<PathBuf>,
    pub warnings: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationError { UnsupportedPlatform, UnsafePath(String), AlreadyComplete, Io(String), Keychain(String), InjectedCrash(CrashStage) }
impl From<io::Error> for MigrationError { fn from(e: io::Error) -> Self { Self::Io(e.to_string()) } }

/// Perform a complete first-run migration. `keychain` is injected so the real
/// macOS adapter and a deterministic test adapter share the same safety path.
pub fn migrate(paths: &MigrationPaths, options: &MigrationOptions, keychain: &mut dyn Keychain) -> Result<MigrationReceipt, MigrationError> {
    if !cfg!(target_os = "macos") { return Err(MigrationError::UnsupportedPlatform); }
    preflight(paths)?;
    crash(options, CrashStage::Preflight)?;
    if paths.version_record().exists() {
        let record: Option<VersionRecord> = read_json(&paths.version_record());
        if record.is_some_and(|r| r.version == MIGRATION_VERSION && r.completed) { return Err(MigrationError::AlreadyComplete); }
    }
    let snapshot = snapshot_legacy(paths)?;
    crash(options, CrashStage::Snapshotted)?;
    let legacy = read_legacy(&paths.legacy_root, Some(&paths.preview_dir));
    if let Some(key) = legacy.config.as_ref().and_then(|c| c.lastfm_key.as_deref()).filter(|k| !k.trim().is_empty()) {
        keychain.import_lastfm_key(key).map_err(MigrationError::Keychain)?;
    }
    let state = convert(&legacy);
    crash(options, CrashStage::Converted)?;
    create_once(&paths.state_file(), &serde_json::to_vec_pretty(&state).map_err(|e| MigrationError::Io(e.to_string()))?)?;
    let version = VersionRecord { migration: "desktop-first-run".into(), version: MIGRATION_VERSION, completed: true };
    if let Err(error) = create_once(&paths.version_record(), &serde_json::to_vec_pretty(&version).unwrap()) {
        let _ = fs::remove_file(paths.state_file());
        return Err(error);
    }
    if let Err(error) = crash(options, CrashStage::VersionRecorded) {
        let _ = fs::remove_file(paths.state_file());
        let _ = fs::remove_file(paths.version_record());
        return Err(error);
    }
    let removed_previews = cleanup_previews(paths)?;
    crash(options, CrashStage::PreviewCleaned)?;
    Ok(MigrationReceipt { version: paths.version_record(), state: paths.state_file(), snapshot, removed_previews, warnings: legacy.warnings.len() })
}

fn preflight(p: &MigrationPaths) -> Result<(), MigrationError> {
    if p.legacy_root == p.target_root { return Err(MigrationError::UnsafePath("legacy and target roots must differ".into())); }
    if p.preview_dir == p.legacy_root || p.preview_dir.starts_with(&p.legacy_root) { return Err(MigrationError::UnsafePath("preview directory overlaps legacy data".into())); }
    if p.target_root.starts_with(&p.legacy_root) { return Err(MigrationError::UnsafePath("target cannot be inside legacy data".into())); }
    fs::create_dir_all(&p.target_root)?;
    Ok(())
}

fn snapshot_legacy(p: &MigrationPaths) -> Result<PathBuf, MigrationError> {
    let dir = p.target_root.join(format!("migration-backup-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()));
    fs::create_dir_all(&dir)?;
    for name in [".luno_config.json", ".luno_playlist_urls.json", ".luno_allowed_dups.json", ".downloaded_vids", ".sync_failed_vids", ".downloaded.txt"] {
        let from = p.legacy_root.join(name); if from.is_file() { fs::copy(&from, dir.join(name))?; }
    }
    Ok(dir)
}

fn convert(s: &LegacyState) -> MigratedSettings {
    let c = s.config.as_ref();
    MigratedSettings { schema_version: MIGRATION_VERSION, music_root: c.and_then(|x| x.music_root.clone()), auto_discovery: c.and_then(|x| x.auto_discovery), volume: c.and_then(|x| x.volume), sleep_timer: c.and_then(|x| x.sleep_timer.clone()), global_keys: c.and_then(|x| x.global_keys), favorites: s.favorite_paths.clone(), playlist_urls: s.playlist_urls.clone(), allowed_duplicate_keys: s.allowed_duplicate_keys.iter().cloned().collect(), downloaded_ids: s.downloaded_ids.iter().cloned().collect(), failed_ids: s.failed_ids.iter().cloned().collect(), history: Vec::new() }
}

fn cleanup_previews(p: &MigrationPaths) -> Result<Vec<PathBuf>, MigrationError> {
    if !p.preview_dir.is_dir() { return Ok(Vec::new()); }
    let mut removed = Vec::new();
    for path in read_dir_files(&p.preview_dir)? {
        if p.current_preview.iter().chain(p.promoted_assets.iter()).any(|keep| keep == &path) { continue; }
        fs::remove_file(&path)?; removed.push(path);
    }
    Ok(removed)
}
fn read_dir_files(dir: &Path) -> Result<Vec<PathBuf>, MigrationError> { Ok(fs::read_dir(dir)?.filter_map(Result::ok).filter_map(|e| e.file_type().ok().filter(|t| t.is_file()).map(|_| e.path())).collect()) }
fn read_json<T: for<'a> Deserialize<'a>>(p: &Path) -> Option<T> { fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok()) }
fn create_once(path: &Path, bytes: &[u8]) -> Result<(), MigrationError> { let mut f = OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| if e.kind() == io::ErrorKind::AlreadyExists { MigrationError::AlreadyComplete } else { e.into() })?; f.write_all(bytes)?; f.sync_all()?; if let Some(parent) = path.parent() { File::open(parent)?.sync_all()?; } Ok(()) }
fn crash(o: &MigrationOptions, stage: CrashStage) -> Result<(), MigrationError> { if o.crash_at == Some(stage) { Err(MigrationError::InjectedCrash(stage)) } else { Ok(()) } }
