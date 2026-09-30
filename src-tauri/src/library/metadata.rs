//! Portable per-file metadata stored beside the music library.
//!
//! Paths are relative to the selected library root, so the sidecar moves with
//! the music folder and never embeds a device-specific absolute path.

use crate::domain::identity::TrackMetadata;
use crate::persistence::atomic_write::write_atomic;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const FILE_NAME: &str = ".luno_track_metadata.json";
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackRecord {
    pub added_ms: Option<u64>,
    pub song: TrackMetadata,
    #[serde(default)]
    pub album: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataFile {
    format_version: u32,
    tracks: BTreeMap<String, TrackRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackMetadataStore {
    tracks: BTreeMap<String, TrackRecord>,
}

impl TrackMetadataStore {
    pub fn load(root: &Path) -> io::Result<Self> {
        let path = root.join(FILE_NAME);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self { tracks: BTreeMap::new() });
            }
            Err(error) => return Err(error),
        };
        let file: MetadataFile = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if file.format_version != FORMAT_VERSION
            || file.tracks.keys().any(|key| !valid_relative_key(key))
        {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "unsupported or unsafe library metadata"));
        }
        Ok(Self { tracks: file.tracks })
    }

    pub fn get(&self, root: &Path, path: &Path) -> Option<&TrackRecord> {
        relative_key(root, path).ok().and_then(|key| self.tracks.get(&key))
    }

    pub fn set(&mut self, root: &Path, path: &Path, record: TrackRecord) -> io::Result<()> {
        self.tracks.insert(relative_key(root, path)?, record);
        Ok(())
    }

    pub fn remove(&mut self, root: &Path, path: &Path) -> io::Result<bool> {
        Ok(self.tracks.remove(&relative_key(root, path)?).is_some())
    }

    pub fn copy_path(&mut self, root: &Path, source: &Path, destination: &Path) -> io::Result<()> {
        let destination_key = relative_key(root, destination)?;
        let source_key = relative_key(root, source).ok();
        if source_key.as_deref() == Some(destination_key.as_str()) {
            return Ok(());
        }
        let record = source_key
            .as_ref()
            .and_then(|key| self.tracks.get(key))
            .cloned()
            .unwrap_or_else(|| TrackRecord {
                added_ms: file_modified_ms(destination).or_else(|| file_modified_ms(source)),
                song: crate::domain::identity::filename_metadata(&source.to_string_lossy()),
                album: None,
            });
        if let Some(source_key) = source_key {
            self.tracks.entry(source_key).or_insert_with(|| record.clone());
        }
        self.tracks.insert(destination_key, record);
        Ok(())
    }

    pub fn move_path(
        &mut self,
        root: &Path,
        source: &Path,
        destination: &Path,
        fallback: TrackRecord,
    ) -> io::Result<()> {
        let source_key = relative_key(root, source)?;
        let destination_key = relative_key(root, destination)?;
        if source_key == destination_key {
            self.tracks.insert(destination_key, fallback);
        } else {
            let previous = self.tracks.remove(&source_key).unwrap_or_else(|| fallback.clone());
            let record = TrackRecord {
                added_ms: previous.added_ms.or(fallback.added_ms),
                song: fallback.song,
                album: fallback.album.or(previous.album),
            };
            self.tracks.insert(destination_key, record);
        }
        Ok(())
    }

    pub fn retain_paths<'a>(&mut self, root: &Path, paths: impl IntoIterator<Item = &'a PathBuf>) {
        let present: BTreeSet<String> = paths
            .into_iter()
            .filter_map(|path| relative_key(root, path).ok())
            .collect();
        self.tracks.retain(|path, _| present.contains(path));
    }

    pub fn save(&self, root: &Path) -> io::Result<()> {
        let file = MetadataFile { format_version: FORMAT_VERSION, tracks: self.tracks.clone() };
        let bytes = serde_json::to_vec_pretty(&file).map_err(io::Error::other)?;
        write_atomic(root.join(FILE_NAME), &bytes).map(|_| ())
    }
}

pub fn file_modified_ms(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_millis().min(u64::MAX as u128) as u64)
}

fn relative_key(root: &Path, path: &Path) -> io::Result<String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "track is outside the library root")
    })?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "track path is not root-relative")),
        }
    }
    if parts.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "track path is empty"));
    }
    Ok(parts.join("/"))
}

fn valid_relative_key(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && value.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}
