//! Current/next temporary MP4 staging, isolated from the music library.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot { Current, Next }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageState { NotChecked, Downloading, Ready, Unavailable }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedVideo { pub slot: Slot, pub generation: u64, pub state: StageState, pub path: Option<PathBuf> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageError { Io(String), Cancelled, StaleGeneration, InvalidRoot, InvalidPath, Empty }
impl From<io::Error> for StageError { fn from(e: io::Error) -> Self { Self::Io(e.to_string()) } }

#[derive(Debug, Clone)]
pub struct VideoStage { root: PathBuf, library_root: Option<PathBuf>, generation: u64 }

/// Sidecar mapping track identity -> downloaded YouTube id, persisted next to
/// the MP4s. Without it every staging attempt had to re-run the multi-second
/// YouTube search BEFORE the cache could be checked (the filename needs the
/// id), so a cached video still sat behind minutes of network work.
///
/// Entries carry a QUALITY TIER: `"high"` (final ≤720p bytes — also how every
/// pre-tier legacy entry reads) or `"quick"` (the ≤480p first pass whose HD
/// upgrade was interrupted before it could replace the file — app restart,
/// track switch cancelling the upgrade, …). Adoption paths use the tier to
/// decide `needs_upgrade`, so an interrupted quick pass is RE-UPGRADED on the
/// next play instead of being served 480p forever.
const INDEX_FILE: &str = "index.json";

/// Final-quality tier marker (≤720p ladder or legacy full-ladder bytes).
pub const INDEX_TIER_HIGH: &str = "high";
/// Fast first-pass tier (≤480p) pending its background quality upgrade.
pub const INDEX_TIER_QUICK: &str = "quick";

fn index_entry(value: &serde_json::Value) -> Option<(String, &'static str)> {
    match value {
        // Legacy shape: a bare id string. Every legacy writer recorded
        // full-ladder runs only, so legacy entries are "high".
        serde_json::Value::String(id) => Some((id.clone(), INDEX_TIER_HIGH)),
        serde_json::Value::Object(map) => {
            let id = map.get("id")?.as_str()?.to_string();
            let tier = if map.get("tier").and_then(|t| t.as_str()) == Some(INDEX_TIER_QUICK) {
                INDEX_TIER_QUICK
            } else {
                INDEX_TIER_HIGH
            };
            Some((id, tier))
        }
        _ => None,
    }
}

fn index_value(id: &str, tier: &str) -> serde_json::Value {
    if tier == INDEX_TIER_HIGH {
        serde_json::Value::String(id.to_string())
    } else {
        serde_json::json!({ "id": id, "tier": INDEX_TIER_QUICK })
    }
}

fn index_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(())).lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl VideoStage {
    fn index_path(&self) -> PathBuf { self.root.join(INDEX_FILE) }
    fn load_index(&self) -> serde_json::Map<String, serde_json::Value> {
        let text = fs::read_to_string(self.index_path()).unwrap_or_default();
        serde_json::from_str::<serde_json::Value>(&text).ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    }
    fn save_index(&self, entries: serde_json::Map<String, serde_json::Value>) {
        let temp = self.root.join(format!(".{INDEX_FILE}.tmp"));
        if fs::write(&temp, serde_json::to_vec(&entries).unwrap_or_default()).and_then(|_| fs::rename(&temp, self.index_path())).is_err() {
            let _ = fs::remove_file(&temp);
        }
    }
    /// Indexed YouTube id for a track, if any.
    pub fn indexed_id(&self, track_key: &str) -> Option<String> {
        let _guard = index_lock();
        index_entry(self.load_index().get(track_key)?).map(|(id, _)| id)
    }
    /// Cached MP4 for an indexed track, plus its QUALITY TIER (see the index
    /// docs: "quick" entries still owe their HD upgrade).
    pub fn index_lookup_with_tier(&self, track_key: &str, slot: Slot) -> Option<(String, PathBuf, &'static str)> {
        let _guard = index_lock();
        let (id, tier) = index_entry(self.load_index().get(track_key)?)?;
        let name = match slot { Slot::Current => "current", Slot::Next => "next" };
        let destination = self.root.join(format!("{name}-{id}.mp4"));
        let usable = |path: &Path| path.is_file() && fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false);
        if !usable(&destination) {
            if slot != Slot::Current { return None; }
            let next = self.root.join(format!("next-{id}.mp4"));
            if !usable(&next) || fs::rename(&next, &destination).is_err() { return None; }
        }
        Some((id, destination, tier))
    }
    /// Cached MP4 for an indexed track. For the Current slot a hit in the Next
    /// slot (earlier prefetch of this exact video) is promoted by rename, so
    /// the caller never touches the network for an already-downloaded video.
    pub fn index_lookup(&self, track_key: &str, slot: Slot) -> Option<(String, PathBuf)> {
        self.index_lookup_with_tier(track_key, slot).map(|(id, path, _)| (id, path))
    }
    pub fn record_index(&self, track_key: &str, video_id: &str) {
        self.record_index_tier(track_key, video_id, INDEX_TIER_HIGH);
    }
    /// Record an id with an explicit quality tier ("quick" = its HD upgrade is
    /// still pending; the upgrade pass must flip it back to "high" on success
    /// or every later play re-adopts 480p as final).
    pub fn record_index_tier(&self, track_key: &str, video_id: &str, tier: &str) {
        let _guard = index_lock();
        let mut entries = self.load_index();
        entries.insert(track_key.to_string(), index_value(video_id, tier));
        self.save_index(entries);
    }
    /// Flip EVERY entry pointing at `video_id` to the given tier (the HD
    /// upgrade replaced that video's file in place — all keys mapping to it
    /// are now final quality). No-op when nothing maps to the id.
    pub fn set_tier_for_video_id(&self, video_id: &str, tier: &str) {
        let _guard = index_lock();
        let mut entries = self.load_index();
        let mut changed = false;
        for value in entries.values_mut() {
            if let Some((id, current)) = index_entry(value) {
                if id == video_id && current != tier {
                    *value = index_value(&id, tier);
                    changed = true;
                }
            }
        }
        if changed { self.save_index(entries); }
    }
    pub fn index_remove(&self, track_key: &str) {
        let _guard = index_lock();
        let mut entries = self.load_index();
        if entries.remove(track_key).is_some() { self.save_index(entries); }
    }
}

impl VideoStage {
    pub fn at(root: impl Into<PathBuf>, library_root: Option<&Path>) -> Result<Self, StageError> {
        let root = root.into();
        if let Some(library) = library_root {
            if root == library || root.starts_with(library) { return Err(StageError::InvalidRoot); }
        }
        fs::create_dir_all(&root)?;
        cleanup(&root)?;
        Ok(Self { root, library_root: library_root.map(Path::to_path_buf), generation: 0 })
    }
    pub fn root(&self) -> &Path { &self.root }
    /// Destination for an externally-produced MP4 (yt-dlp writes to a PATH,
    /// not a writer handle). Pair with `adopt` once the file exists.
    pub fn external_destination(&self, slot: Slot, key: &str) -> Result<PathBuf, StageError> { self.destination(slot, key) }
    pub fn begin_generation(&mut self) -> u64 { self.generation += 1; self.generation }
    pub fn state(&self, slot: Slot, generation: u64) -> StagedVideo { StagedVideo { slot, generation, state: StageState::NotChecked, path: None } }
    pub fn adopt(&self, slot: Slot, generation: u64, path: impl AsRef<Path>) -> Result<StagedVideo, StageError> {
        self.check_generation(generation)?;
        let path = path.as_ref();
        if !self.contained(path) || !path.is_file() || path.extension().and_then(|x| x.to_str()) != Some("mp4") { return Err(StageError::InvalidPath); }
        Ok(StagedVideo { slot, generation, state: StageState::Ready, path: Some(path.to_path_buf()) })
    }
    pub fn stage<F>(&self, slot: Slot, key: &str, generation: u64, cancel: &AtomicBool, write: F) -> Result<StagedVideo, StageError>
    where F: FnOnce(&mut dyn Write) -> Result<(), String> {
        self.check_generation(generation)?;
        if cancel.load(Ordering::Acquire) { return Err(StageError::Cancelled); }
        let destination = self.destination(slot, key)?;
        if destination.is_file() { return Ok(StagedVideo { slot, generation, state: StageState::Ready, path: Some(destination) }); }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let safe = safe_key(key)?;
        let partial = self.root.join(format!(".{safe}.{stamp}.part"));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&partial)?;
        let result = write(&mut file).map_err(StageError::Io).and_then(|_| {
            file.flush()?;
            if cancel.load(Ordering::Acquire) { return Err(StageError::Cancelled); }
            self.check_generation(generation)?;
            if fs::metadata(&partial)?.len() == 0 { return Err(StageError::Empty); }
            fs::rename(&partial, &destination)?;
            Ok(StagedVideo { slot, generation, state: StageState::Ready, path: Some(destination) })
        });
        if result.is_err() { let _ = fs::remove_file(&partial); }
        result
    }
    pub fn unavailable(&self, slot: Slot, generation: u64) -> StagedVideo { StagedVideo { slot, generation, state: StageState::Unavailable, path: None } }
    pub fn cleanup(&self, preserve: &[PathBuf]) -> Result<(), StageError> {
        let index = self.index_path();
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            if preserve.contains(&path) || path == index { continue; }
            if path.is_file() { let _ = fs::remove_file(path); }
        }
        Ok(())
    }
    fn check_generation(&self, generation: u64) -> Result<(), StageError> { if generation != self.generation { Err(StageError::StaleGeneration) } else { Ok(()) } }
    fn destination(&self, slot: Slot, key: &str) -> Result<PathBuf, StageError> { let name = match slot { Slot::Current => "current", Slot::Next => "next" }; let safe = safe_key(key)?; Ok(self.root.join(format!("{name}-{safe}.mp4"))) }
    fn contained(&self, path: &Path) -> bool { path.parent() == Some(self.root.as_path()) && self.library_root.as_ref().map(|r| !path.starts_with(r)).unwrap_or(true) }
}

fn cleanup(root: &Path) -> io::Result<()> {
    // Startup sweep: KEEP previously staged MP4s (current-*/next-*) so the
    // index fast-path can adopt them instantly across launches — wiping them
    // here turned every app start into a full re-download despite the index.
    // Only partial downloads and stray temp files are removed.
    let index = root.join(INDEX_FILE);
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path == index { continue; }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("current-") || name.starts_with("next-") { continue; }
        }
        if path.is_file() { let _ = fs::remove_file(path); }
    }
    Ok(())
}
fn safe_key(key: &str) -> Result<String, StageError> { let safe: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect(); if safe.is_empty() { Err(StageError::InvalidPath) } else { Ok(safe) } }

#[cfg(test)]
mod staging_index_tests {
    use super::*;

    #[test]
    fn index_roundtrips_tiers_and_reads_legacy_strings_as_high() {
        let root = std::env::temp_dir().join(format!("luno-index-tier-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let stager = VideoStage::at(&root, None).unwrap();
        // Lookups require a usable staged file on disk.
        for id in ["idHigh", "idQuick", "oldId"] {
            let path = stager.external_destination(Slot::Current, id).unwrap();
            fs::write(&path, b"x").unwrap();
        }
        stager.record_index_tier("k-high", "idHigh", INDEX_TIER_HIGH);
        stager.record_index_tier("k-quick", "idQuick", INDEX_TIER_QUICK);
        // Legacy shape: a bare string written by older builds.
        let mut entries = stager.load_index();
        entries.insert("legacy".to_string(), serde_json::Value::String("oldId".into()));
        stager.save_index(entries);

        assert_eq!(stager.index_lookup_with_tier("k-high", Slot::Current).map(|(_, _, t)| t), Some(INDEX_TIER_HIGH));
        assert_eq!(stager.index_lookup_with_tier("k-quick", Slot::Current).map(|(_, _, t)| t), Some(INDEX_TIER_QUICK));
        assert_eq!(stager.index_lookup_with_tier("legacy", Slot::Current).map(|(id, _, t)| (id == "oldId", t)), Some((true, INDEX_TIER_HIGH)));
        assert_eq!(stager.indexed_id("k-quick").as_deref(), Some("idQuick"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn set_tier_for_video_id_flips_every_matching_key() {
        let root = std::env::temp_dir().join(format!("luno-index-flip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let stager = VideoStage::at(&root, None).unwrap();
        for id in ["shared", "other"] {
            let path = stager.external_destination(Slot::Current, id).unwrap();
            fs::write(&path, b"x").unwrap();
        }
        stager.record_index_tier("a", "shared", INDEX_TIER_QUICK);
        stager.record_index_tier("b", "shared", INDEX_TIER_QUICK);
        stager.record_index_tier("c", "other", INDEX_TIER_QUICK);
        stager.set_tier_for_video_id("shared", INDEX_TIER_HIGH);
        assert_eq!(stager.index_lookup_with_tier("a", Slot::Current).map(|(_, _, t)| t), Some(INDEX_TIER_HIGH));
        assert_eq!(stager.index_lookup_with_tier("b", Slot::Current).map(|(_, _, t)| t), Some(INDEX_TIER_HIGH));
        assert_eq!(stager.index_lookup_with_tier("c", Slot::Current).map(|(_, _, t)| t), Some(INDEX_TIER_QUICK));
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod staging_tests {
    use super::*;
}
