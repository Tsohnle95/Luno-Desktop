//! Candidate index used by transfer import.  Paths are locations; provider
//! identities and normalized metadata are the matching keys.

use crate::domain::identity::{compatible_duration, MatchResult, ProviderIdentity, TrackMetadata};
use crate::domain::normalization::sync_key;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedTrack {
    pub path: PathBuf,
    pub metadata: TrackMetadata,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderIndex {
    by_provider: HashMap<ProviderIdentity, Vec<IndexedTrack>>,
    tracks: Vec<IndexedTrack>,
}

impl ProviderIndex {
    pub fn new(tracks: impl IntoIterator<Item = IndexedTrack>) -> Self {
        let mut index = Self::default();
        for track in tracks { index.insert(track); }
        index
    }

    pub fn insert(&mut self, track: IndexedTrack) {
        if let Some(provider) = &track.metadata.provider {
            self.by_provider.entry(provider.clone()).or_default().push(track.clone());
        }
        self.tracks.push(track);
    }

    /// Provider lookup is exclusive: a provider collision is ambiguous and is
    /// never silently resolved by falling back to metadata.
    pub fn matching(&self, wanted: &TrackMetadata) -> MatchResult<PathBuf> {
        let mut candidates: Vec<&IndexedTrack> = if let Some(provider) = &wanted.provider {
            self.by_provider.get(provider).map_or_else(Vec::new, |items| items.iter().collect())
        } else {
            let key = sync_key(&wanted.artist, &wanted.title);
            self.tracks.iter().filter(|item| {
                sync_key(&item.metadata.artist, &item.metadata.title) == key
                    && compatible_duration(wanted.duration_ms, item.metadata.duration_ms)
            }).collect()
        };
        candidates.sort_by(|a, b| a.path.cmp(&b.path));
        match candidates.len() {
            0 => MatchResult::Unresolved,
            1 => MatchResult::Match(candidates[0].path.clone()),
            _ => MatchResult::Ambiguous(candidates.into_iter().map(|item| item.path.clone()).collect()),
        }
    }

    pub fn len(&self) -> usize { self.tracks.len() }
    pub fn is_empty(&self) -> bool { self.tracks.is_empty() }
}
