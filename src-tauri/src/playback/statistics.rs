//! Durable listening counts written under Luno's runtime home.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const FILE_NAME: &str = ".luno_play_statistics.json";
const MAX_COUNT: u64 = crate::contracts::types::MAX_SAFE_EVENT_ID;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayStatistics {
    #[serde(default = "current_format")]
    pub format_version: u16,
    #[serde(default)]
    pub track_plays: BTreeMap<String, u64>,
    #[serde(default)]
    pub playlist_plays: BTreeMap<String, u64>,
    #[serde(default)]
    pub legacy_track_counts_imported: bool,
}

impl Default for PlayStatistics {
    fn default() -> Self {
        Self {
            format_version: current_format(),
            track_plays: BTreeMap::new(),
            playlist_plays: BTreeMap::new(),
            legacy_track_counts_imported: false,
        }
    }
}

const fn current_format() -> u16 { 1 }

impl PlayStatistics {
    pub fn load(home: &Path) -> Self {
        let path = home.join(FILE_NAME);
        let backup = PathBuf::from(format!("{}.luno-backup", path.display()));
        for candidate in [&path, &backup] {
            match fs::read(candidate) {
                Ok(bytes) => match serde_json::from_slice::<Self>(&bytes) {
                    Ok(mut stats) if stats.format_version == current_format() => {
                        stats.track_plays.retain(|id, plays| !id.trim().is_empty() && *plays > 0 && *plays <= MAX_COUNT);
                        stats.playlist_plays.retain(|id, plays| !id.trim().is_empty() && *plays > 0 && *plays <= MAX_COUNT);
                        return stats;
                    }
                    Ok(_) => eprintln!("[luno-stats] unsupported play statistics format in {}", candidate.display()),
                    Err(error) => eprintln!("[luno-stats] could not read play statistics {}: {error}", candidate.display()),
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => eprintln!("[luno-stats] could not read play statistics {}: {error}", candidate.display()),
            }
        }
        Self::default()
    }

    pub fn save(&self, home: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        crate::persistence::atomic_write::write_atomic(home.join(FILE_NAME), &bytes).map(|_| ())
    }

    pub fn record_track(&mut self, id: &str) {
        increment(&mut self.track_plays, id);
    }

    pub fn record_playlist(&mut self, id: &str) {
        increment(&mut self.playlist_plays, id);
    }

    /// Import the old WebView journal once. The durable marker makes retries
    /// idempotent, and addition preserves starts recorded while import was in flight.
    pub fn import_legacy_track_counts(&mut self, counts: &BTreeMap<String, u64>) -> bool {
        if self.legacy_track_counts_imported { return false; }
        for (id, plays) in counts {
            if id.trim().is_empty() || *plays == 0 { continue; }
            let value = *plays;
            let current = self.track_plays.entry(id.clone()).or_insert(0);
            *current = current.saturating_add(value).min(MAX_COUNT);
        }
        self.legacy_track_counts_imported = true;
        true
    }
}

fn increment(counts: &mut BTreeMap<String, u64>, id: &str) {
    if id.trim().is_empty() { return; }
    let plays = counts.entry(id.to_owned()).or_insert(0);
    *plays = plays.saturating_add(1).min(MAX_COUNT);
}
