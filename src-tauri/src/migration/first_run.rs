//! First-run migration boundary for the macOS desktop rewrite.
//!
//! This module is deliberately additive. Legacy files are inputs and are never
//! removed or rewritten. A migration can therefore be retried, or the Python
//! release can be launched again, after any failure.

use crate::migration::legacy_readers::{read_legacy, LegacyState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MIGRATION_VERSION: u32 = 1;
pub const MAX_LASTFM_KEY_LEN: usize = 8 * 1024;

#[derive(Debug, Clone)]
pub struct MigrationPaths {
    pub legacy_root: PathBuf,
    /// `legacy_root` may be HOME, which is a discovery envelope rather than
    /// the actual legacy data root.
    pub legacy_data_root: Option<PathBuf>,
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
pub struct MigrationOptions { pub crash_at: Option<CrashStage>, pub fail_discard_quarantine: bool, pub fail_snapshot: bool, pub fail_state_create: bool, pub fail_preview_restore: bool }

pub trait Keychain {
    fn import_lastfm_key(&mut self, key: &str) -> Result<KeychainImport, String>;
    fn rollback_lastfm_key(&mut self, import: KeychainImport) -> Result<(), String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeychainImport { Created, Existing }

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
    pub warning_messages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationError { UnsupportedPlatform, UnsafePath(String), AlreadyComplete(Vec<String>), Io(String), Keychain(String), Cleanup(String), InjectedCrash(CrashStage) }
impl From<io::Error> for MigrationError { fn from(e: io::Error) -> Self { Self::Io(e.to_string()) } }

/// Perform a complete first-run migration. `keychain` is injected so the real
/// macOS adapter and a deterministic test adapter share the same safety path.
pub fn migrate(paths: &MigrationPaths, options: &MigrationOptions, keychain: &mut dyn Keychain) -> Result<MigrationReceipt, MigrationError> {
    if !cfg!(target_os = "macos") { return Err(MigrationError::UnsupportedPlatform); }
    preflight(paths)?;
    crash(options, CrashStage::Preflight)?;
    let valid_completed_record = read_json::<VersionRecord>(&paths.version_record()).filter(|record| {
        record.migration == "desktop-first-run" && record.version == MIGRATION_VERSION && record.completed
    });
    if valid_completed_record.is_some() {
        let mut warning_messages = reconcile_recovery_quarantine(&paths.target_root);
        let legacy = read_legacy(&paths.legacy_root, Some(&paths.preview_dir));
        if !legacy.warnings.is_empty() || !legacy.preview_files.is_empty() || legacy.config.is_some()
            || !legacy.playlist_urls.is_empty() || !legacy.allowed_duplicate_keys.is_empty()
            || !legacy.favorite_paths.is_empty() || !legacy.downloaded_ids.is_empty() || !legacy.failed_ids.is_empty()
        {
            warning_messages.push("migration already complete; detected preserved or newly added legacy inputs and did not overwrite target state".into());
        }
        return Ok(MigrationReceipt {
            version: paths.version_record(),
            state: paths.state_file(),
            snapshot: paths.target_root.clone(),
            removed_previews: Vec::new(),
            warnings: warning_messages.len(),
            warning_messages,
        });
    }
    let quarantined_records = quarantine_records(paths)?;
    if options.fail_snapshot {
        return Err(restore_after_snapshot_failure(MigrationError::Io("injected snapshot failure".into()), quarantined_records));
    }
    let snapshot = match snapshot_legacy(paths) {
        Ok(snapshot) => snapshot,
        Err(error) => return Err(restore_after_snapshot_failure(error, quarantined_records)),
    };
    if let Err(error) = crash(options, CrashStage::Snapshotted) {
        return Err(rollback(paths, snapshot, error, true, &quarantined_records, keychain, None));
    }
    let legacy = read_legacy(&paths.legacy_root, Some(&paths.preview_dir));
    let mut keychain_import = None;
    if let Some(key) = legacy.config.as_ref().and_then(|c| c.lastfm_key.as_deref()).filter(|k| !k.trim().is_empty()) {
        match keychain.import_lastfm_key(key) {
            Ok(import) => keychain_import = Some(import),
            Err(error) => return Err(rollback(paths, snapshot, MigrationError::Keychain(error), true, &quarantined_records, keychain, None)),
        }
    }
    let state = convert(&legacy);
    if let Err(error) = crash(options, CrashStage::Converted) {
        return Err(rollback(paths, snapshot, error, true, &quarantined_records, keychain, keychain_import));
    }
    if let Err(error) = create_once(&paths.state_file(), &serde_json::to_vec_pretty(&state).map_err(|e| MigrationError::Io(e.to_string()))?, options.fail_state_create) {
        return Err(rollback(paths, snapshot, error, true, &quarantined_records, keychain, keychain_import));
    }
    let version = VersionRecord { migration: "desktop-first-run".into(), version: MIGRATION_VERSION, completed: true };
    if let Err(error) = create_once(&paths.version_record(), &serde_json::to_vec_pretty(&version).unwrap(), false) {
        return Err(rollback(paths, snapshot, error, true, &quarantined_records, keychain, keychain_import));
    }
    if let Err(error) = crash(options, CrashStage::VersionRecorded) {
        return Err(rollback(paths, snapshot, error, true, &quarantined_records, keychain, keychain_import));
    }
    let cleanup = match cleanup_previews(paths) {
        Ok(removed) => removed,
        Err(error) => return Err(rollback(paths, snapshot, error, true, &quarantined_records, keychain, keychain_import)),
    };
    if let Err(error) = crash(options, CrashStage::PreviewCleaned) {
        let restore_error = cleanup.restore(options.fail_preview_restore).err();
        return Err(rollback_with_extra(paths, snapshot, error, true, &quarantined_records, restore_error, keychain, keychain_import));
    }
    let removed_previews = match cleanup.commit() {
        Ok(removed) => removed,
        Err((cleanup, error)) => {
            let restore_error = cleanup.restore(options.fail_preview_restore).err();
            return Err(rollback_with_extra(paths, snapshot, error, true, &quarantined_records, restore_error, keychain, keychain_import));
        }
    };
    let mut warning_messages: Vec<String> = legacy.warnings.iter().map(|warning| format!("{}: {}", warning.path.display(), warning.message)).collect();
    if let Err(error) = discard_quarantine(quarantined_records, options.fail_discard_quarantine) {
        warning_messages.push(format!("migration committed but recovery quarantine cleanup failed: {error}"));
    }
    Ok(MigrationReceipt { version: paths.version_record(), state: paths.state_file(), snapshot, removed_previews, warnings: warning_messages.len(), warning_messages })
}

fn preflight(p: &MigrationPaths) -> Result<(), MigrationError> {
    if p.legacy_root == p.target_root { return Err(MigrationError::UnsafePath("legacy and target roots must differ".into())); }
    if !p.preview_dir.starts_with(&p.legacy_root) || p.preview_dir == p.legacy_root {
        return Err(MigrationError::UnsafePath("preview directory must be explicitly contained by the legacy discovery root".into()));
    }
    if p.legacy_data_root.as_ref().is_some_and(|root| p.target_root.starts_with(root)) {
        return Err(MigrationError::UnsafePath("target cannot be inside legacy data".into()));
    }
    if let Ok(metadata) = fs::symlink_metadata(&p.preview_dir) {
        if metadata.file_type().is_symlink() { return Err(MigrationError::UnsafePath("preview directory must not be a symlink".into())); }
    }
    for (name, path) in [("legacy root", &p.legacy_root), ("target root", &p.target_root)] {
        if fs::symlink_metadata(path).map(|metadata| metadata.file_type().is_symlink()).unwrap_or(false) {
            return Err(MigrationError::UnsafePath(format!("{name} must not be a symlink")));
        }
    }
    fs::create_dir_all(&p.target_root)?;
    Ok(())
}

fn snapshot_legacy(p: &MigrationPaths) -> Result<PathBuf, MigrationError> {
    let dir = p.target_root.join(format!("migration-backup-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()));
    fs::create_dir_all(&dir)?;
    let result = (|| -> Result<(), MigrationError> { for root in [p.legacy_root.clone(), read_legacy(&p.legacy_root, None).config.and_then(|config| config.music_root.map(PathBuf::from)).unwrap_or_default()] {
        if root.as_os_str().is_empty() { continue; }
        for name in [".luno_config.json", ".luno_playlist_urls.json", ".luno_allowed_dups.json", ".downloaded_vids", ".sync_failed_vids", ".downloaded.txt"] {
            let from = root.join(name); if from.is_file() { let destination = if root == p.legacy_root { dir.join(name) } else { dir.join("music-root").join(name) }; if let Some(parent) = destination.parent() { fs::create_dir_all(parent)?; } if name == ".luno_config.json" { snapshot_config(&from, &destination, &dir)?; } else { fs::copy(&from, destination)?; } }
        }
        for path in walk_marker_files(&root) {
            if path == root.join(".downloaded_vids") || path == root.join(".sync_failed_vids") || path == root.join(".downloaded.txt") { continue; }
            let destination = dir.join(path.strip_prefix(&root).unwrap_or(&path));
            if let Some(parent) = destination.parent() { fs::create_dir_all(parent)?; }
            fs::copy(path, destination)?;
        }
    } Ok(()) })();
    if let Err(error) = result { let _ = fs::remove_dir_all(&dir); return Err(error); }
    Ok(dir)
}

fn snapshot_config(source: &Path, destination: &Path, backup: &Path) -> Result<(), MigrationError> {
    let bytes = fs::read(source)?;
    let mut value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) if value.is_object() => value,
        _ => {
            fs::write(backup.join("config-snapshot-omitted.txt"), "Legacy config snapshot omitted: malformed or non-object JSON; secret redaction could not be proven.\n")?;
            return Ok(());
        }
    };
    let object = value.as_object_mut().expect("object checked above");
    object.remove("lastfm_key");
    object.insert("lastfm_key".into(), Value::String("[REDACTED]".into()));
    fs::write(destination, serde_json::to_vec_pretty(&value).map_err(|e| MigrationError::Io(e.to_string()))?)?;
    Ok(())
}

fn walk_marker_files(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else { return Vec::new(); };
    entries.flatten().flat_map(|entry| {
        let path = entry.path();
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) { walk_marker_files(&path) }
        else if matches!(path.file_name().and_then(|name| name.to_str()), Some(".downloaded_vids" | ".sync_failed_vids" | ".downloaded.txt")) { vec![path] }
        else { Vec::new() }
    }).collect()
}

fn convert(s: &LegacyState) -> MigratedSettings {
    let c = s.config.as_ref();
    MigratedSettings { schema_version: MIGRATION_VERSION, music_root: c.and_then(|x| x.music_root.clone()), auto_discovery: c.and_then(|x| x.auto_discovery), volume: c.and_then(|x| x.volume), sleep_timer: c.and_then(|x| x.sleep_timer.clone()), global_keys: c.and_then(|x| x.global_keys), favorites: s.favorite_paths.clone(), playlist_urls: s.playlist_urls.clone(), allowed_duplicate_keys: s.allowed_duplicate_keys.iter().cloned().collect(), downloaded_ids: s.downloaded_ids.iter().cloned().collect(), failed_ids: s.failed_ids.iter().cloned().collect(), history: Vec::new() }
}

struct PreviewCleanup { removed: Vec<PathBuf>, quarantine: Option<PathBuf> }
impl PreviewCleanup {
    fn restore(&self, injected_failure: bool) -> Result<(), MigrationError> {
        if injected_failure { return Err(MigrationError::Cleanup("injected preview restoration failure".into())); }
        let Some(quarantine) = &self.quarantine else { return Ok(()); };
        let mut failures = Vec::new();
        for path in self.removed.iter().rev() {
            if let Err(error) = fs::rename(quarantine.join(path.file_name().unwrap()), path) { failures.push(format!("{}: {error}", path.display())); }
        }
        if let Err(error) = fs::remove_dir(quarantine) { if error.kind() != io::ErrorKind::NotFound { failures.push(format!("{}: {error}", quarantine.display())); } }
        if failures.is_empty() { Ok(()) } else { Err(MigrationError::Cleanup(format!("preview restoration failed: {}", failures.join("; ")))) }
    }
    fn commit(self) -> Result<Vec<PathBuf>, (Self, MigrationError)> {
        let Some(quarantine) = &self.quarantine else { return Ok(self.removed); };
        if let Err(error) = fs::remove_dir_all(quarantine) { return Err((self, MigrationError::Cleanup(format!("preview quarantine cleanup failed: {error}")))); }
        Ok(self.removed)
    }
}
fn cleanup_previews(p: &MigrationPaths) -> Result<PreviewCleanup, MigrationError> {
    let metadata = match fs::symlink_metadata(&p.preview_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(PreviewCleanup { removed: Vec::new(), quarantine: None }),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(MigrationError::Cleanup("preview directory is not a real directory".into()));
    }
    let mut removed = Vec::new();
    let entries = read_dir_files(&p.preview_dir)?;
    let quarantine = p.preview_dir.parent().unwrap_or(Path::new(".")).join(format!(".luno-preview-quarantine-{}", nonce()));
    fs::create_dir(&quarantine)?;
    let result = (|| -> Result<(), MigrationError> {
        let mut candidates = Vec::new();
        for path in entries {
            let metadata = fs::symlink_metadata(&path)?;
            if path.parent() != Some(p.preview_dir.as_path()) || metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                return Err(MigrationError::Cleanup(format!("preview cleanup failed: unsafe preview entry: {}", path.display())));
            }
            if p.current_preview.iter().chain(p.promoted_assets.iter()).any(|keep| keep == &path) { continue; }
            candidates.push(path);
        }
        for path in candidates {
            fs::rename(&path, quarantine.join(path.file_name().unwrap()))?;
            removed.push(path);
        }
        Ok(())
    })();
    if let Err(error) = result {
        for path in &removed { let _ = fs::rename(quarantine.join(path.file_name().unwrap()), path); }
        let _ = fs::remove_dir(&quarantine);
        return Err(error);
    }
    Ok(PreviewCleanup { removed, quarantine: Some(quarantine) })
}
fn read_dir_files(dir: &Path) -> Result<Vec<PathBuf>, MigrationError> { fs::read_dir(dir)?.map(|entry| entry.map(|e| e.path()).map_err(MigrationError::from)).collect() }
fn read_json<T: for<'a> Deserialize<'a>>(p: &Path) -> Option<T> { fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok()) }
fn rollback(p: &MigrationPaths, snapshot: PathBuf, error: MigrationError, records: bool, quarantined: &[(PathBuf, PathBuf)], keychain: &mut dyn Keychain, keychain_import: Option<KeychainImport>) -> MigrationError {
    let mut paths = vec![(snapshot, true)];
    if records { paths.extend([(p.state_file(), false), (p.version_record(), false)]); }
    let result = rollback_paths(error, paths);
    let result = match keychain_import.and_then(|import| keychain.rollback_lastfm_key(import).err()) {
        Some(error) => aggregate(result, MigrationError::Keychain(format!("Keychain rollback failed: {error}"))),
        None => result,
    };
    rollback_records(result, quarantined)
}
fn rollback_records(mut result: MigrationError, quarantined: &[(PathBuf, PathBuf)]) -> MigrationError {
    let mut failures = Vec::new();
    for (original, backup) in quarantined.into_iter().rev() {
        if let Err(error) = fs::rename(&backup, &original) { failures.push(format!("{}: {error}", original.display())); }
    }
    if failures.is_empty() { result } else { aggregate(result, MigrationError::Cleanup(format!("record restoration failed: {}", failures.join("; ")))) }
}
fn rollback_with_extra(p: &MigrationPaths, snapshot: PathBuf, error: MigrationError, records: bool, quarantined: &[(PathBuf, PathBuf)], extra: Option<MigrationError>, keychain: &mut dyn Keychain, keychain_import: Option<KeychainImport>) -> MigrationError {
    let result = rollback(p, snapshot, error, records, quarantined, keychain, keychain_import);
    extra.map_or(result, |extra| aggregate(result, MigrationError::Cleanup(format!("preview restoration failed: {extra:?}"))))
}
fn aggregate(primary: MigrationError, secondary: MigrationError) -> MigrationError {
    MigrationError::Cleanup(format!("migration failed: {primary:?}; additional rollback failure: {secondary:?}"))
}
fn restore_after_snapshot_failure(error: MigrationError, quarantined: Vec<(PathBuf, PathBuf)>) -> MigrationError {
    let mut failures = Vec::new();
    for (original, backup) in quarantined.into_iter().rev() {
        if let Err(restore) = fs::rename(&backup, &original) { failures.push(format!("{}: {restore}", original.display())); }
    }
    if failures.is_empty() { error } else { aggregate(error, MigrationError::Cleanup(format!("record restoration failed: {}", failures.join("; ")))) }
}
fn rollback_paths(error: MigrationError, paths: impl IntoIterator<Item = (PathBuf, bool)>) -> MigrationError {
    let mut failures = Vec::new();
    for (path, directory) in paths {
        let result = if directory { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
        if let Err(err) = result { if err.kind() != io::ErrorKind::NotFound { failures.push(format!("{}: {err}", path.display())); } }
    }
    if failures.is_empty() { error } else { MigrationError::Cleanup(format!("migration failed: {error:?}; rollback cleanup failed: {}", failures.join("; "))) }
}
fn create_once(path: &Path, bytes: &[u8], injected_failure: bool) -> Result<(), MigrationError> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| if e.kind() == io::ErrorKind::AlreadyExists { MigrationError::AlreadyComplete(Vec::new()) } else { e.into() })?;
    let result = (|| -> Result<(), MigrationError> {
        f.write_all(bytes)?;
        if injected_failure { return Err(MigrationError::Io("injected state create failure".into())); }
        f.sync_all()?;
        if let Some(parent) = path.parent() { File::open(parent)?.sync_all()?; }
        Ok(())
    })();
    if let Err(error) = result {
        drop(f);
        if let Err(cleanup) = fs::remove_file(path) {
            if cleanup.kind() != io::ErrorKind::NotFound { return Err(aggregate(error, MigrationError::Cleanup(format!("partial state-file cleanup failed: {cleanup}")))); }
        }
        return Err(error);
    }
    Ok(())
}
fn crash(o: &MigrationOptions, stage: CrashStage) -> Result<(), MigrationError> { if o.crash_at == Some(stage) { Err(MigrationError::InjectedCrash(stage)) } else { Ok(()) } }
fn nonce() -> u128 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() }
fn quarantine_records(p: &MigrationPaths) -> Result<Vec<(PathBuf, PathBuf)>, MigrationError> {
    let mut result = Vec::new();
    for original in [p.state_file(), p.version_record()] {
        if !original.exists() { continue; }
        let backup = p.target_root.join(format!(".migration-recovery-{}-{}", nonce(), original.file_name().unwrap().to_string_lossy()));
        if let Err(error) = fs::rename(&original, &backup) {
            let mut failures = Vec::new();
            for (restored, stranded) in result.into_iter().rev() {
                if let Err(restore) = fs::rename(stranded, &restored) {
                    failures.push(format!("{}: {restore}", restored.display()));
                }
            }
            let primary: MigrationError = error.into();
            return if failures.is_empty() { Err(primary) } else {
                Err(aggregate(primary, MigrationError::Cleanup(format!("record restoration failed: {}", failures.join("; ")))))
            };
        }
        result.push((original, backup));
    }
    Ok(result)
}
fn discard_quarantine(items: Vec<(PathBuf, PathBuf)>, injected_failure: bool) -> Result<(), MigrationError> {
    if injected_failure { return Err(MigrationError::Cleanup("injected quarantine disposal failure".into())); }
    for (_, backup) in items { fs::remove_file(backup)?; }
    Ok(())
}

fn reconcile_recovery_quarantine(target: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let entries = match fs::read_dir(target) { Ok(entries) => entries, Err(error) => { warnings.push(format!("recovery quarantine scan failed: {error}")); return warnings; } };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with(".migration-recovery-")) {
            if let Err(error) = fs::remove_file(&path) { warnings.push(format!("recovery quarantine cleanup failed for {}: {error}", path.display())); }
            else { warnings.push(format!("discarded leftover recovery quarantine {} after completed migration", path.display())); }
        }
    }
    warnings
}
