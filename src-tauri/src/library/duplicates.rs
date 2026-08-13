//! Duplicate review and durable allowed-key management.
use crate::domain::normalization::{duplicate_key, split_track_name};
use crate::persistence::atomic_write::write_atomic;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateEntry { pub path: PathBuf, pub playlist: String, pub title: String, pub artist: String }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup { pub key: String, pub entries: Vec<DuplicateEntry> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuplicateDecision { Allow { key: String }, Delete { paths: Vec<PathBuf> } }
#[derive(Debug)]
pub enum BackupError { Io(io::Error), Json(serde_json::Error), Invalid(String) }
impl std::fmt::Display for BackupError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { match self { Self::Io(e) => write!(f, "backup I/O error: {e}"), Self::Json(e) => write!(f, "invalid backup JSON: {e}"), Self::Invalid(e) => write!(f, "invalid backup: {e}") } } }
impl std::error::Error for BackupError {}
impl From<io::Error> for BackupError { fn from(e: io::Error) -> Self { Self::Io(e) } }
impl From<serde_json::Error> for BackupError { fn from(e: serde_json::Error) -> Self { Self::Json(e) } }
fn valid_key(k: &str) -> bool { !k.is_empty() && k.len() <= 4096 && k.chars().all(|c| c.is_ascii_alphanumeric()) }

pub fn find_duplicates(library: &BTreeMap<String, Vec<PathBuf>>, allowed: &BTreeSet<String>) -> Vec<DuplicateGroup> {
    let mut buckets: BTreeMap<String, Vec<DuplicateEntry>> = BTreeMap::new();
    for (playlist, tracks) in library { for path in tracks {
        let (title, artist) = split_track_name(&path.to_string_lossy());
        let key = duplicate_key(&format!("{artist}{title}"));
        if key.is_empty() || allowed.contains(&key) { continue; }
        buckets.entry(key.clone()).or_default().push(DuplicateEntry { path: path.clone(), playlist: playlist.clone(), title, artist });
    } }
    buckets.into_iter().filter(|(_, e)| e.len() > 1).map(|(key, entries)| DuplicateGroup { key, entries }).collect()
}
pub fn allow_decision(key: impl Into<String>) -> Result<DuplicateDecision, BackupError> { let key = key.into(); if !valid_key(&key) { return Err(BackupError::Invalid("allow key must be normalized alphanumeric text".into())); } Ok(DuplicateDecision::Allow { key }) }
pub fn delete_decision(paths: Vec<PathBuf>) -> Result<DuplicateDecision, BackupError> { let mut seen = BTreeSet::new(); if paths.is_empty() || paths.iter().any(|p| !seen.insert(p.clone())) { return Err(BackupError::Invalid("delete decision requires unique paths".into())); } Ok(DuplicateDecision::Delete { paths }) }
pub fn load_allowed(root: &Path) -> Result<BTreeSet<String>, BackupError> { let path = root.join(".luno_allowed_dups.json"); if !path.exists() { return Ok(BTreeSet::new()); } parse_backup(&fs::read_to_string(path)?) }
pub fn save_allowed(root: &Path, keys: &BTreeSet<String>) -> Result<(), BackupError> { if keys.iter().any(|k| !valid_key(k)) { return Err(BackupError::Invalid("allowed keys must be normalized alphanumeric text".into())); } let bytes = serde_json::to_vec_pretty(&keys.iter().collect::<Vec<_>>())?; write_atomic(root.join(".luno_allowed_dups.json"), &bytes)?; Ok(()) }
pub fn export_backup(keys: &BTreeSet<String>) -> Result<String, BackupError> { if keys.iter().any(|k| !valid_key(k)) { return Err(BackupError::Invalid("allowed keys must be normalized alphanumeric text".into())); } Ok(serde_json::to_string_pretty(&keys.iter().collect::<Vec<_>>())?) }
pub fn import_backup(input: &str) -> Result<BTreeSet<String>, BackupError> { parse_backup(input) }
pub fn import_backup_into(input: &str, keys: &mut BTreeSet<String>) -> Result<usize, BackupError> { let imported = parse_backup(input)?; let before = keys.len(); keys.extend(imported); Ok(keys.len() - before) }
fn parse_backup(input: &str) -> Result<BTreeSet<String>, BackupError> { let values: Vec<String> = serde_json::from_str(input)?; if values.iter().any(|k| !valid_key(k)) { return Err(BackupError::Invalid("backup contains an invalid key".into())); } Ok(values.into_iter().collect()) }
