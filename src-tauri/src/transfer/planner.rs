//! Additive transfer planning and application.

use super::codec::{Manifest, Track};
use super::provider_index::ProviderIndex;
use crate::domain::identity::{MatchResult, ProviderIdentity, TrackMetadata};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPlan {
    Matched(PathBuf),
    Ambiguous(Vec<PathBuf>),
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTrack {
    pub reference: String,
    pub match_plan: MatchPlan,
    pub destinations: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPlan {
    pub tracks: Vec<PlannedTrack>,
    pub no_deletions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyFailure { pub reference: String, pub destination: PathBuf, pub message: String }

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyResult { pub copied: Vec<PathBuf>, pub failures: Vec<CopyFailure> }

pub fn plan_import(manifest: &Manifest, index: &ProviderIndex, destination_root: impl AsRef<Path>) -> ImportPlan {
    let root = destination_root.as_ref();
    let playlist_paths: HashMap<&str, PathBuf> = manifest.playlists.iter()
        .map(|playlist| (playlist.name.as_str(), root.join(sanitize_name(&playlist.name))))
        .collect();
    let mut memberships: HashMap<&str, Vec<PathBuf>> = HashMap::new();
    for playlist in &manifest.playlists {
        for reference in &playlist.track_refs {
            if let Some(path) = playlist_paths.get(playlist.name.as_str()) {
                memberships.entry(reference.as_str()).or_default().push(path.clone());
            }
        }
    }
    let tracks = manifest.tracks.iter().map(|track| {
        let metadata = metadata(track);
        let match_plan = match index.matching(&metadata) {
            MatchResult::Match(path) => MatchPlan::Matched(path),
            MatchResult::Ambiguous(paths) => MatchPlan::Ambiguous(paths),
            MatchResult::Unresolved => MatchPlan::Unresolved,
        };
        let destinations = memberships.remove(track.r#ref.as_str()).unwrap_or_default()
            .into_iter().map(|folder| folder.join(file_name(track))).collect();
        PlannedTrack { reference: track.r#ref.clone(), match_plan, destinations }
    }).collect();
    ImportPlan { tracks, no_deletions: true }
}

/// Apply only uniquely matched records, or an explicitly selected candidate
/// for an ambiguous record.  The approval map is intentionally per reference.
pub fn apply_import_plan(plan: &ImportPlan, approvals: &HashMap<String, PathBuf>) -> ApplyResult {
    let mut result = ApplyResult::default();
    for record in &plan.tracks {
        let source = match &record.match_plan {
            MatchPlan::Matched(path) => Some(path),
            MatchPlan::Ambiguous(candidates) => approvals.get(&record.reference)
                .filter(|path| candidates.contains(path)),
            MatchPlan::Unresolved => None,
        };
        let Some(source) = source else { continue };
        for destination in &record.destinations {
            let target = collision_free(destination);
            if let Err(error) = copy_file(source, &target) {
                result.failures.push(CopyFailure { reference: record.reference.clone(), destination: target, message: error.to_string() });
            } else { result.copied.push(target); }
        }
    }
    result
}

fn copy_file(source: &Path, destination: &Path) -> io::Result<u64> {
    if let Some(parent) = destination.parent() { fs::create_dir_all(parent)?; }
    fs::copy(source, destination)
}

fn collision_free(path: &Path) -> PathBuf {
    if !path.exists() { return path.to_owned(); }
    let stem = path.file_stem().and_then(|v| v.to_str()).unwrap_or("track");
    let extension = path.extension().and_then(|v| v.to_str()).map(|v| format!(".{v}")).unwrap_or_default();
    for number in 1.. {
        let candidate = path.with_file_name(format!("{stem} ({number}){extension}"));
        if !candidate.exists() { return candidate; }
    }
    unreachable!()
}

fn metadata(track: &Track) -> TrackMetadata {
    TrackMetadata { title: track.title.clone(), artist: track.artist.clone(), duration_ms: track.duration_ms,
        provider: track.source.as_ref().map(|source| ProviderIdentity { provider: source.provider.clone(), id: source.id.clone() }) }
}

fn sanitize_name(value: &str) -> String {
    let clean: String = value.chars().filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c)).collect();
    let clean = clean.trim().trim_end_matches('.').trim();
    if clean.is_empty() || clean == "." || clean == ".." { "Unsorted".into() } else { clean.into() }
}

fn file_name(track: &Track) -> String {
    let title = if track.artist.trim().is_empty() { track.title.clone() } else { format!("{} - {}", track.artist, track.title) };
    format!("{}.mp3", sanitize_name(&title))
}
