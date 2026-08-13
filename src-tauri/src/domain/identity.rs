//! Stable track/playlist identity and conservative migration matching.

use serde::{Deserialize, Serialize};
use super::normalization::{duplicate_metadata_key, split_track_name, sync_key};

pub const DURATION_TOLERANCE_MS: u64 = 2_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderIdentity { pub provider: String, pub id: String }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub duration_ms: u64,
    pub provider: Option<ProviderIdentity>,
}

pub fn track_id(metadata: &TrackMetadata) -> String {
    if let Some(provider) = &metadata.provider {
        return format!("provider:{}:{}", provider.provider.to_lowercase(), provider.id);
    }
    format!("metadata:{}:{}", sync_key(&metadata.artist, &metadata.title), metadata.duration_ms)
}

pub fn playlist_id(name: &str) -> String { format!("playlist:{:016x}", stable_hash(name)) }

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100000001b3))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchResult<T> { Match(T), Ambiguous(Vec<T>), Unresolved }

pub fn compatible_duration(expected: u64, actual: u64) -> bool {
    expected == 0 || actual == 0 || expected.abs_diff(actual) <= DURATION_TOLERANCE_MS
}

/// Provider matches are exclusive; metadata matches are duration-filtered and
/// retain every candidate so callers can require per-record confirmation.
pub fn match_track<'a>(wanted: &TrackMetadata, candidates: &'a [(String, TrackMetadata)]) -> MatchResult<String> {
    let mut options: Vec<String> = if let Some(provider) = &wanted.provider {
        candidates.iter().filter(|(_, item)| item.provider.as_ref() == Some(provider)).map(|(path, _)| path.clone()).collect()
    } else {
        let key = sync_key(&wanted.artist, &wanted.title);
        candidates.iter().filter(|(_, item)| sync_key(&item.artist, &item.title) == key && compatible_duration(wanted.duration_ms, item.duration_ms)).map(|(path, _)| path.clone()).collect()
    };
    options.sort();
    match options.len() { 0 => MatchResult::Unresolved, 1 => MatchResult::Match(options.remove(0)), _ => MatchResult::Ambiguous(options) }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelocationMapping { pub old_path: String, pub new_path: String, pub track_id: String, pub evidence: String }

pub fn map_relocation(old_path: &str, candidates: &[(String, TrackMetadata)], metadata: &TrackMetadata) -> MatchResult<RelocationMapping> {
    match match_track(metadata, candidates) {
        MatchResult::Match(path) => MatchResult::Match(RelocationMapping { old_path: old_path.to_owned(), new_path: path, track_id: track_id(metadata), evidence: "provider-or-metadata-duration".into() }),
        MatchResult::Ambiguous(paths) => MatchResult::Ambiguous(paths.into_iter().map(|path| RelocationMapping { old_path: old_path.to_owned(), new_path: path, track_id: track_id(metadata), evidence: "competing-candidate".into() }).collect()),
        MatchResult::Unresolved => MatchResult::Unresolved,
    }
}

pub fn filename_metadata(path: &str) -> TrackMetadata {
    let (title, artist) = split_track_name(path);
    TrackMetadata { title, artist, duration_ms: 0, provider: None }
}

pub fn duplicate_key_for(metadata: &TrackMetadata) -> String { duplicate_metadata_key(&metadata.artist, &metadata.title) }
