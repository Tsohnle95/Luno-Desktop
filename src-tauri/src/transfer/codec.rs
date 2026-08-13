//! Strict codec for the path-free `luno.library.export` v1 contract.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MANIFEST_FORMAT: &str = "luno.library.export";
pub const MANIFEST_VERSION: u32 = 1;
const MAX_TRACKS: usize = 50_000;
const MAX_PLAYLISTS: usize = 5_000;
const MAX_STRING_LENGTH: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub manifest_version: u32,
    pub scope: String,
    pub created_at: String,
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub unassigned_track_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub r#ref: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    #[serde(default)]
    pub source: Option<Source>,
    pub ambiguity_confirmation_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub provider: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Playlist {
    pub name: String,
    pub description: String,
    pub track_refs: Vec<String>,
}

#[derive(Debug)]
pub enum CodecError {
    Json(serde_json::Error),
    Invalid(String),
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid JSON: {error}"),
            Self::Invalid(message) => write!(formatter, "invalid manifest: {message}"),
        }
    }
}

impl std::error::Error for CodecError {}

impl From<serde_json::Error> for CodecError {
    fn from(error: serde_json::Error) -> Self { Self::Json(error) }
}

pub fn decode(input: &str) -> Result<Manifest, CodecError> {
    let manifest: Manifest = serde_json::from_str(input)?;
    validate(&manifest)?;
    Ok(manifest)
}

pub fn encode(manifest: &Manifest) -> Result<String, CodecError> {
    validate(manifest)?;
    Ok(serde_json::to_string_pretty(manifest)? + "\n")
}

pub fn validate(manifest: &Manifest) -> Result<(), CodecError> {
    if manifest.format != MANIFEST_FORMAT || manifest.manifest_version != MANIFEST_VERSION {
        return Err(invalid("unsupported manifest format or version"));
    }
    if !matches!(manifest.scope.as_str(), "selected_tracks" | "playlists" | "library") {
        return Err(invalid("unsupported manifest scope"));
    }
    validate_string(&manifest.created_at, "created_at", true)?;
    if !manifest.created_at.ends_with('Z') {
        return Err(invalid("created_at must be an ISO-8601 UTC timestamp"));
    }
    if manifest.tracks.len() > MAX_TRACKS || manifest.unassigned_track_refs.len() > MAX_TRACKS {
        return Err(invalid("track arrays exceed their bounds"));
    }
    if manifest.playlists.len() > MAX_PLAYLISTS {
        return Err(invalid("playlists exceeds its bound"));
    }

    let mut refs = HashSet::new();
    for (i, track) in manifest.tracks.iter().enumerate() {
        let field = format!("tracks[{i}]");
        validate_ref(&track.r#ref, &format!("{field}.ref"))?;
        if !refs.insert(track.r#ref.as_str()) {
            return Err(invalid(format!("duplicate {field}.ref")));
        }
        validate_string(&track.title, &format!("{field}.title"), true)?;
        validate_string(&track.artist, &format!("{field}.artist"), false)?;
        validate_string(&track.album, &format!("{field}.album"), false)?;
        if track.duration_ms > 86_400_000 {
            return Err(invalid(format!("{field}.duration_ms is invalid")));
        }
        if let Some(source) = &track.source {
            validate_source(source, &field)?;
        } else if !track.ambiguity_confirmation_required {
            return Err(invalid(format!("{field} requires ambiguity confirmation without a source id")));
        }
    }

    let mut playlist_names = HashSet::new();
    for (i, playlist) in manifest.playlists.iter().enumerate() {
        let field = format!("playlists[{i}]");
        validate_string(&playlist.name, &format!("{field}.name"), true)?;
        if playlist.name == "." || playlist.name == ".." || playlist.name.contains('/') || playlist.name.contains('\\') {
            return Err(invalid(format!("unsafe {field}.name")));
        }
        if playlist.name.eq_ignore_ascii_case("all music") || !playlist_names.insert(playlist.name.to_lowercase()) {
            return Err(invalid(format!("duplicate or virtual {field}.name")));
        }
        validate_string(&playlist.description, &format!("{field}.description"), false)?;
        if playlist.track_refs.len() > MAX_TRACKS {
            return Err(invalid(format!("{field}.track_refs exceeds its bound")));
        }
        let mut seen = HashSet::new();
        for reference in &playlist.track_refs {
            validate_ref(reference, &format!("{field}.track_refs"))?;
            if !refs.contains(reference.as_str()) || !seen.insert(reference.as_str()) {
                return Err(invalid(format!("invalid track reference in {field}")));
            }
        }
    }
    let mut seen = HashSet::new();
    for reference in &manifest.unassigned_track_refs {
        validate_ref(reference, "unassigned_track_refs")?;
        if !refs.contains(reference.as_str()) || !seen.insert(reference.as_str()) {
            return Err(invalid("invalid unassigned track reference"));
        }
    }
    Ok(())
}

fn validate_source(source: &Source, field: &str) -> Result<(), CodecError> {
    validate_string(&source.provider, &format!("{field}.source.provider"), true)?;
    validate_string(&source.id, &format!("{field}.source.id"), true)?;
    if !matches!(source.provider.as_str(), "youtube" | "soundcloud" | "deezer" | "musicbrainz") {
        return Err(invalid(format!("unsupported {field}.source.provider")));
    }
    if !matches_pattern(&source.id, |c| c.is_ascii_alphanumeric() || ".-_~:".contains(c), 256) {
        return Err(invalid(format!("invalid {field}.source.id")));
    }
    Ok(())
}

fn validate_ref(value: &str, field: &str) -> Result<(), CodecError> {
    if !matches_pattern(value, |c| c.is_ascii_alphanumeric() || "._:-".contains(c), 128) {
        return Err(invalid(format!("invalid {field}")));
    }
    Ok(())
}

fn validate_string(value: &str, field: &str, required: bool) -> Result<(), CodecError> {
    if value.len() > MAX_STRING_LENGTH || value.chars().any(|c| c.is_control()) || (required && value.trim().is_empty()) {
        return Err(invalid(format!("{field} is invalid")));
    }
    Ok(())
}

fn matches_pattern(value: &str, allowed: impl Fn(char) -> bool, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.chars().all(allowed)
}

fn invalid(message: impl Into<String>) -> CodecError { CodecError::Invalid(message.into()) }
