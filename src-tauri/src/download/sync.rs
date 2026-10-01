//! Playlist synchronization policy, independent of provider/UI plumbing.

use crate::domain::normalization::{duplicate_key, split_track_name, sync_key};
use crate::download::markers::Markers;
use crate::download::promotion::{validate_output, Validation};
use crate::runtime::task_registry::CancellationToken;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::fs::OpenOptions;
use std::sync::atomic::{AtomicU64, Ordering};

const SYNC_LOG: &str = ".luno-sync-log.jsonl";
pub const SYNC_SOURCE_STATE: &str = ".luno-sync-source-state.json";
const SYNC_SOURCE_STATE_VERSION: u8 = 1;
pub const SYNC_REVIEW_STATE: &str = ".luno-sync-review-state.json";
const SYNC_REVIEW_STATE_VERSION: u8 = 1;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncLogEntry {
    pub title: String,
    pub video_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_video_id: Option<String>,
    pub at_ms: u64,
    pub status: String,
    pub reason: Option<String>,
}

pub fn read_log(folder: &Path) -> Result<Vec<SyncLogEntry>, String> {
    let file = match fs::File::open(folder.join(SYNC_LOG)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("sync log could not be opened: {error}")),
    };
    let mut entries = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| format!("sync log could not be read: {error}"))?;
        entries.push(serde_json::from_str(&line).map_err(|error| format!("sync log is malformed: {error}"))?);
    }
    entries.reverse();
    Ok(entries)
}

pub fn append_log(folder: &Path, entry: &PlaylistEntry, outcome: &ItemOutcome) -> Result<(), String> {
    append_log_with_source(folder, entry, entry.id.as_deref(), outcome)
}

pub fn append_log_with_source(folder: &Path, entry: &PlaylistEntry, source_video_id: Option<&str>, outcome: &ItemOutcome) -> Result<(), String> {
    let (status, reason) = match outcome {
        ItemOutcome::Downloaded => ("downloaded", None),
        ItemOutcome::Unavailable(reason) => ("unavailable", Some(reason.clone())),
        ItemOutcome::Failed(reason) | ItemOutcome::PermanentFailure(reason) => ("failed", Some(reason.clone())),
        ItemOutcome::Cancelled => ("cancelled", None),
        ItemOutcome::Matched => return Ok(()),
    };
    let at_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?.as_millis() as u64;
    let row = SyncLogEntry {
        title: entry.title.clone(), video_id: entry.id.clone(),
        source_video_id: source_video_id.map(str::to_owned), at_ms, status: status.into(), reason,
    };
    let mut file = OpenOptions::new().create(true).append(true).open(folder.join(SYNC_LOG))
        .map_err(|error| format!("sync log could not be written: {error}"))?;
    serde_json::to_writer(&mut file, &row).map_err(|error| format!("sync log could not be written: {error}"))?;
    writeln!(file).map_err(|error| format!("sync log could not be written: {error}"))?;
    file.sync_data().map_err(|error| format!("sync log could not be saved: {error}"))
}

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "wav", "flac", "webm"];
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistEntry {
    pub id: Option<String>,
    pub title: String,
    pub channel: Option<String>,
    pub duration_secs: Option<u64>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SelectedSource {
    pub video_id: String,
    pub url: String,
    #[serde(default)]
    pub retry_pending: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UnresolvedEntry {
    pub video_id: String,
    pub title: String,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub duration_secs: Option<u64>,
    pub reason: String,
    #[serde(default)]
    pub attempted_source_id: Option<String>,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlaylistSyncState {
    pub version: u8,
    #[serde(default)]
    pub sources: BTreeMap<String, SelectedSource>,
    #[serde(default)]
    pub unresolved: BTreeMap<String, UnresolvedEntry>,
}

impl Default for PlaylistSyncState {
    fn default() -> Self {
        Self { version: SYNC_SOURCE_STATE_VERSION, sources: BTreeMap::new(), unresolved: BTreeMap::new() }
    }
}

impl PlaylistSyncState {
    pub fn load(folder: &Path) -> Result<Self, String> {
        let path = folder.join(SYNC_SOURCE_STATE);
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(format!("playlist source state could not be read: {error}")),
        };
        let state: Self = serde_json::from_str(&content)
            .map_err(|error| format!("playlist source state is malformed: {error}"))?;
        if state.version != SYNC_SOURCE_STATE_VERSION {
            return Err(format!("unsupported playlist source state version {}", state.version));
        }
        for (original_id, source) in &state.sources {
            if !valid_marker_id(original_id) || youtube_watch_url(&source.video_id).as_deref() != Some(source.url.as_str()) {
                return Err("playlist source state contains an invalid video mapping".into());
            }
        }
        for (original_id, issue) in &state.unresolved {
            if original_id != &issue.video_id || !valid_marker_id(original_id) {
                return Err("playlist source state contains an invalid unresolved entry".into());
            }
        }
        Ok(state)
    }

    pub fn save(&self, folder: &Path) -> Result<(), String> {
        if self.version != SYNC_SOURCE_STATE_VERSION {
            return Err("cannot save an unsupported playlist source state version".into());
        }
        let json = serde_json::to_vec_pretty(self).map_err(|error| format!("playlist source state could not be encoded: {error}"))?;
        write_atomic(&folder.join(SYNC_SOURCE_STATE), &json)
            .map_err(|error| format!("playlist source state could not be saved: {error}"))
    }

    pub fn select_source(&mut self, original_id: &str, source_id: &str) -> Result<(), String> {
        if !valid_marker_id(original_id) {
            return Err("original playlist video ID is invalid".into());
        }
        let url = youtube_watch_url(source_id).ok_or_else(|| "choose a valid YouTube video ID".to_string())?;
        self.sources.insert(original_id.to_owned(), SelectedSource { video_id: source_id.to_owned(), url, retry_pending: true });
        Ok(())
    }

    fn record_unavailable(&mut self, entry: &PlaylistEntry, attempted_source_id: Option<&str>, reason: &str) {
        let Some(video_id) = entry.id.as_deref().filter(|id| valid_marker_id(id)) else { return; };
        self.unresolved.insert(video_id.to_owned(), UnresolvedEntry {
            video_id: video_id.to_owned(), title: entry.title.clone(), channel: entry.channel.clone(),
            duration_secs: entry.duration_secs, reason: reason.to_owned(),
            attempted_source_id: attempted_source_id.map(str::to_owned), updated_at_ms: now_ms(),
        });
    }

    pub fn migrate_legacy_unavailable(&mut self, folder: &Path, entries: &[PlaylistEntry], markers: &Markers) -> bool {
        let mut latest_unavailable = BTreeMap::new();
        if let Ok(log) = read_log(folder) {
            for row in log {
                if row.status == "failed" || row.status == "unavailable" {
                    if let (Some(id), Some(reason)) = (row.video_id, row.reason) {
                        if unavailable_failure(&reason) { latest_unavailable.entry(id).or_insert(reason); }
                    }
                }
            }
        }
        let mut changed = false;
        for entry in entries {
            let Some(id) = entry.id.as_deref() else { continue; };
            if self.unresolved.contains_key(id) || self.sources.contains_key(id) || !markers.failed.contains(id) || markers.successful.contains(id) {
                continue;
            }
            if let Some(reason) = latest_unavailable.get(id) {
                self.record_unavailable(entry, None, reason);
                changed = true;
            }
        }
        changed
    }

    pub fn prune_unresolved(&mut self, entries: &[PlaylistEntry], markers: &Markers) -> bool {
        let present: HashSet<&str> = entries.iter().filter_map(|entry| entry.id.as_deref()).collect();
        let before = self.unresolved.len();
        self.unresolved.retain(|id, _| present.contains(id.as_str()) && !markers.successful.contains(id));
        before != self.unresolved.len()
    }
}

/// Explicit local replacements and dismissals are kept in a separate file so
/// existing v1 source-selection state remains readable by older app builds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlaylistSyncReviewState {
    pub version: u8,
    #[serde(default)]
    pub dismissed: BTreeSet<String>,
    #[serde(default)]
    pub replacements: BTreeMap<String, String>,
}

impl Default for PlaylistSyncReviewState {
    fn default() -> Self {
        Self { version: SYNC_REVIEW_STATE_VERSION, dismissed: BTreeSet::new(), replacements: BTreeMap::new() }
    }
}

impl PlaylistSyncReviewState {
    pub fn load(folder: &Path) -> Result<Self, String> {
        let path = folder.join(SYNC_REVIEW_STATE);
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(format!("playlist review state could not be read: {error}")),
        };
        let state: Self = serde_json::from_str(&content)
            .map_err(|error| format!("playlist review state is malformed: {error}"))?;
        if state.version != SYNC_REVIEW_STATE_VERSION
            || state.dismissed.iter().any(|id| !valid_marker_id(id))
            || state.replacements.iter().any(|(id, path)| !valid_marker_id(id) || !valid_review_path(path))
        {
            return Err("playlist review state contains an unsupported version or unsafe mapping".into());
        }
        Ok(state)
    }

    pub fn save(&self, folder: &Path) -> Result<(), String> {
        if self.version != SYNC_REVIEW_STATE_VERSION
            || self.dismissed.iter().any(|id| !valid_marker_id(id))
            || self.replacements.iter().any(|(id, path)| !valid_marker_id(id) || !valid_review_path(path))
        {
            return Err("cannot save invalid playlist review state".into());
        }
        let json = serde_json::to_vec_pretty(self).map_err(|error| format!("playlist review state could not be encoded: {error}"))?;
        write_atomic(&folder.join(SYNC_REVIEW_STATE), &json)
            .map_err(|error| format!("playlist review state could not be saved: {error}"))
    }

    pub fn dismiss(&mut self, video_id: &str) -> Result<(), String> {
        if !valid_marker_id(video_id) { return Err("original playlist video ID is invalid".into()); }
        self.replacements.remove(video_id);
        self.dismissed.insert(video_id.to_owned());
        Ok(())
    }

    pub fn replace(&mut self, video_id: &str, relative_path: String) -> Result<(), String> {
        if !valid_marker_id(video_id) { return Err("original playlist video ID is invalid".into()); }
        if !valid_review_path(&relative_path) { return Err("replacement track path is invalid".into()); }
        self.dismissed.remove(video_id);
        self.replacements.insert(video_id.to_owned(), relative_path);
        Ok(())
    }

    pub fn prune(&mut self, entries: &[PlaylistEntry]) -> bool {
        let present: HashSet<&str> = entries.iter().filter_map(|entry| entry.id.as_deref()).collect();
        let before = self.dismissed.len() + self.replacements.len();
        self.dismissed.retain(|id| present.contains(id.as_str()));
        self.replacements.retain(|id, _| present.contains(id.as_str()));
        before != self.dismissed.len() + self.replacements.len()
    }

    pub fn is_dismissed(&self, video_id: &str) -> bool { self.dismissed.contains(video_id) }

    pub fn replacement_exists(&self, root: &Path, video_id: &str) -> bool {
        let Some(relative) = self.replacements.get(video_id) else { return false; };
        resolve_review_path(root, relative).is_some()
    }

    pub fn replacement_relative_path(root: &Path, path: &Path) -> Result<String, String> {
        let root = fs::canonicalize(root).map_err(|error| format!("library root could not be resolved: {error}"))?;
        let path = fs::canonicalize(path).map_err(|error| format!("replacement track is unavailable: {error}"))?;
        if !path.starts_with(&root) || !path.is_file() {
            return Err("replacement track is outside the library or is not a file".into());
        }
        let relative = path.strip_prefix(&root).map_err(|_| "replacement track is outside the library")?;
        let parts: Vec<String> = relative.components().map(|component| match component {
            std::path::Component::Normal(part) => Ok(part.to_string_lossy().into_owned()),
            _ => Err("replacement track path is not root-relative"),
        }).collect::<Result<_, _>>()?;
        if parts.is_empty() { return Err("replacement track path is empty".into()); }
        Ok(parts.join("/"))
    }
}

fn valid_review_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && value.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

fn resolve_review_path(root: &Path, relative: &str) -> Option<PathBuf> {
    if !valid_review_path(relative) { return None; }
    let root = fs::canonicalize(root).ok()?;
    let path = fs::canonicalize(root.join(relative)).ok()?;
    (path.starts_with(&root) && path.is_file()).then_some(path)
}

pub fn local_replacement_matches(
    entry_title: &str,
    channel: Option<&str>,
    target_playlist: &str,
    snapshot: &crate::contracts::types::LibrarySnapshot,
) -> Vec<crate::contracts::types::PlaylistSyncLocalMatch> {
    let (listed_artist, listed_title) = parse_title(entry_title);
    let wanted_title = duplicate_key(&listed_title);
    let full_title = duplicate_key(entry_title);
    if wanted_title.is_empty() && full_title.is_empty() { return Vec::new(); }
    let wanted_artist = duplicate_key(if listed_artist.is_empty() {
        channel.unwrap_or_default()
    } else {
        &listed_artist
    });
    let mut candidates: Vec<(bool, bool, crate::contracts::types::PlaylistSyncLocalMatch)> = snapshot.tracks.iter()
        .filter(|track| {
            let title = duplicate_key(&track.title);
            (!wanted_title.is_empty() && title == wanted_title) || (!full_title.is_empty() && title == full_title)
        })
        .map(|track| {
            let playlists: Vec<String> = snapshot.playlists.iter()
                .filter(|playlist| playlist.ordered_track_ids.contains(&track.id))
                .map(|playlist| playlist.name.clone())
                .collect();
            let in_target = playlists.iter().any(|name| name == target_playlist);
            let artist_match = !wanted_artist.is_empty()
                && track.artist.as_deref().is_some_and(|artist| duplicate_key(artist) == wanted_artist);
            let local = crate::contracts::types::PlaylistSyncLocalMatch {
                track_id: track.id.clone(), title: track.title.clone(), artist: track.artist.clone(),
                duration_ms: track.duration_ms, playlists,
            };
            (in_target, artist_match, local)
        })
        .collect();
    candidates.sort_by(|left, right| {
        right.0.cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.2.title.to_lowercase().cmp(&right.2.title.to_lowercase()))
            .then_with(|| left.2.track_id.0.cmp(&right.2.track_id.0))
    });
    candidates.into_iter().take(12).map(|(_, _, candidate)| candidate).collect()
}

/// Resolve previously unavailable playlist entries when their audio has since
/// appeared in the library through another download path. Keep this aligned
/// with the filename matching policy used by playlist planning.
pub fn reconcile_unresolved_local_matches(
    root: &Path,
    folder: &Path,
    entries: &[PlaylistEntry],
    state: &mut PlaylistSyncState,
    markers: &mut Markers,
) -> Result<usize, String> {
    let mut keys = HashSet::new();
    let mut titles = HashSet::new();
    collect_audio(root, &mut keys, &mut titles);
    let matched: Vec<String> = entries.iter().filter_map(|entry| {
        let id = entry.id.as_deref()?;
        if !state.unresolved.contains_key(id) || !existing_audio_matches(entry, &keys, &titles) {
            return None;
        }
        Some(id.to_owned())
    }).collect();
    if matched.is_empty() { return Ok(0); }

    for id in &matched {
        markers.record_success(folder, id).map_err(|error| format!("matched playlist entry could not be recorded: {error}"))?;
        state.unresolved.remove(id);
        state.sources.remove(id);
    }
    state.save(folder)?;
    Ok(matched.len())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |duration| duration.as_millis() as u64)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| std::io::Error::other("state path has no parent"))?;
    let file_name = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| std::io::Error::other("state filename is invalid"))?;
    let (temp, mut file) = loop {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp = parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), sequence));
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => break (temp, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        if let Ok(directory) = fs::File::open(parent) { directory.sync_all()?; }
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(temp); }
    result
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncItem {
    pub entry: PlaylistEntry,
    pub source_override: Option<SelectedSource>,
    pub destination: PathBuf,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOutcome {
    Matched,
    Downloaded,
    Unavailable(String),
    PermanentFailure(String),
    Failed(String),
    Cancelled,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncResult {
    pub outcomes: Vec<(String, ItemOutcome)>,
    pub cancelled: bool,
    pub state_warnings: Vec<String>,
}

pub fn playlist_url(input: &str) -> Option<String> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    if !input.contains('/') && !input.contains('?') {
        return valid_playlist_id(input).then(|| format!("https://www.youtube.com/playlist?list={input}"));
    }
    let lower = input.to_ascii_lowercase();
    let scheme_end = lower.find("://")?;
    if !matches!(&lower[..scheme_end], "http" | "https") { return None; }
    let authority = &lower[scheme_end + 3..];
    let host = authority.split(['/', '?', '#']).next()?.split('@').next_back()?.split(':').next()?;
    if !matches!(host, "youtube.com" | "www.youtube.com" | "music.youtube.com" | "m.youtube.com") { return None; }
    let list = input
        .split(['?', '&'])
        .find_map(|part| part.strip_prefix("list="))
        .and_then(|value| value.split('#').next())?;
    valid_playlist_id(list).then(|| format!("https://www.youtube.com/playlist?list={list}"))
}
fn valid_playlist_id(value: &str) -> bool { !value.is_empty() && value.len() <= 128 && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) }
pub fn parse_title(title: &str) -> (String, String) {
    title
        .split_once(" - ")
        .map_or((String::new(), title.trim().to_owned()), |(a, t)| {
            (a.trim().to_owned(), t.trim().to_owned())
        })
}

pub fn plan(
    root: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    entries: &[PlaylistEntry],
) -> Vec<SyncItem> {
    plan_with_state(root, destination, entries, &PlaylistSyncState::default())
}

pub fn plan_with_state(
    root: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    entries: &[PlaylistEntry],
    state: &PlaylistSyncState,
) -> Vec<SyncItem> {
    plan_with_review_state(root, destination, entries, state, &PlaylistSyncReviewState::default())
}

pub fn plan_with_review_state(
    root: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    entries: &[PlaylistEntry],
    state: &PlaylistSyncState,
    review: &PlaylistSyncReviewState,
) -> Vec<SyncItem> {
    let root = root.as_ref();
    let destination = destination.as_ref();
    let mut keys = HashSet::new();
    let mut titles = HashSet::new();
    collect_audio(root, &mut keys, &mut titles);
    let existing_keys = keys.clone();
    let existing_titles = titles.clone();
    entries
        .iter()
        .filter_map(|entry| {
            if let Some(id) = entry.id.as_deref() {
                if review.is_dismissed(id) || review.replacement_exists(root, id) { return None; }
            }
            if existing_audio_matches(entry, &existing_keys, &existing_titles) {
                return None;
            }
            if let Some(id) = entry.id.as_deref() {
                if state.unresolved.contains_key(id) && !state.sources.get(id).is_some_and(|source| source.retry_pending) {
                    return None;
                }
            }
            let (artist, title) = parse_title(&entry.title);
            let key = sync_key(&artist, &title);
            let title_key = sync_key("", &title);
            if (key.is_empty() && title_key.is_empty())
                || keys.contains(&key)
                || (artist.is_empty() && titles.contains(&title_key))
            {
                return None;
            }
            let safe = safe_name(&artist, &title);
            if AUDIO_EXTENSIONS
                .iter()
                .any(|ext| destination.join(format!("{safe}.{ext}")).exists())
            {
                return None;
            }
            keys.insert(key);
            titles.insert(title_key);
            Some(SyncItem {
                entry: entry.clone(),
                source_override: entry.id.as_ref().and_then(|id| state.sources.get(id)).cloned(),
                destination: destination.join(format!("{safe}.m4a")),
            })
        })
        .collect()
}

fn existing_audio_matches(entry: &PlaylistEntry, keys: &HashSet<String>, titles: &HashSet<String>) -> bool {
    let (artist, title) = parse_title(&entry.title);
    let key = sync_key(&artist, &title);
    let title_key = sync_key("", &title);
    !(key.is_empty() && title_key.is_empty())
        && (keys.contains(&key) || (artist.is_empty() && titles.contains(&title_key)))
}

pub fn sync_sequential<F>(
    folder: impl AsRef<Path>,
    items: &[SyncItem],
    token: &CancellationToken,
    download: F,
) -> SyncResult
where
    F: FnMut(&SyncItem, &CancellationToken) -> Result<(), String>,
{
    sync_sequential_observed(folder, items, token, download, |_, _| {})
}

pub fn sync_sequential_observed<F, O>(
    folder: impl AsRef<Path>,
    items: &[SyncItem],
    token: &CancellationToken,
    download: F,
    mut observe: O,
) -> SyncResult
where
    F: FnMut(&SyncItem, &CancellationToken) -> Result<(), String>,
    O: FnMut(&PlaylistEntry, &ItemOutcome),
{
    let mut state = PlaylistSyncState::load(folder.as_ref()).unwrap_or_default();
    sync_sequential_observed_with_state(folder, items, token, &mut state, download, |item, outcome| observe(&item.entry, outcome))
}

pub fn sync_sequential_observed_with_state<F, O>(
    folder: impl AsRef<Path>,
    items: &[SyncItem],
    token: &CancellationToken,
    state: &mut PlaylistSyncState,
    mut download: F,
    mut observe: O,
) -> SyncResult
where
    F: FnMut(&SyncItem, &CancellationToken) -> Result<(), String>,
    O: FnMut(&SyncItem, &ItemOutcome),
{
    let folder = folder.as_ref();
    let mut markers = Markers::load(folder);
    let mut result = SyncResult::default();
    for item in items {
        if token.is_cancelled() {
            result.cancelled = true;
            break;
        }
        let label = item.entry.id.clone().unwrap_or_else(|| item.entry.title.clone());
        if let Some(id) = item.entry.id.as_deref() {
            if markers.successful.contains(id) {
                result.outcomes.push((label, ItemOutcome::Matched));
                continue;
            }
        }
        if let Err(error) = consume_selected_source_retry(folder, state, item) {
            let outcome = ItemOutcome::Failed(format!("selected source retry could not be recorded: {error}"));
            observe(item, &outcome);
            result.outcomes.push((label, outcome));
            continue;
        }
        let outcome = match download(item, token) {
            Ok(()) if token.is_cancelled() => ItemOutcome::Cancelled,
            Ok(()) if validate_output(&item.destination, Validation::audio()).is_err() => {
                ItemOutcome::Failed("validated audio output is missing or invalid".into())
            }
            Ok(()) => {
                if let Some(id) = item.entry.id.as_deref() {
                    match markers.record_success(folder, id) {
                        Ok(()) => {
                            if state.unresolved.remove(id).is_some() {
                                if let Err(error) = state.save(folder) { result.state_warnings.push(error); }
                            }
                            ItemOutcome::Downloaded
                        }
                        Err(error) => ItemOutcome::Failed(format!("could not record success marker: {error}")),
                    }
                } else {
                    ItemOutcome::Downloaded
                }
            }
            Err(_) if token.is_cancelled() => ItemOutcome::Cancelled,
            Err(error) if unavailable_failure(&error) => {
                state.record_unavailable(&item.entry, item.source_override.as_ref().map(|source| source.video_id.as_str()).or(item.entry.id.as_deref()), &error);
                match state.save(folder) {
                    Ok(()) => {
                        if let Some(id) = item.entry.id.as_deref() { let _ = markers.record_failure(folder, id, &error); }
                        ItemOutcome::Unavailable(error)
                    }
                    Err(save_error) => ItemOutcome::Failed(format!("{error}; unresolved state could not be saved: {save_error}")),
                }
            }
            Err(error) if permanent_failure(&error) => {
                if let Some(id) = item.entry.id.as_deref() {
                    let _ = markers.record_failure(folder, id, &error);
                }
                ItemOutcome::PermanentFailure(error)
            }
            Err(error) => ItemOutcome::Failed(error),
        };
        let cancelled = matches!(outcome, ItemOutcome::Cancelled);
        observe(item, &outcome);
        result.outcomes.push((label, outcome));
        if cancelled {
            result.cancelled = true;
            break;
        }
    }
    result
}

fn consume_selected_source_retry(folder: &Path, state: &mut PlaylistSyncState, item: &SyncItem) -> Result<(), String> {
    let Some(id) = item.entry.id.as_deref() else { return Ok(()); };
    let Some(source) = state.sources.get_mut(id) else { return Ok(()); };
    if !source.retry_pending { return Ok(()); }
    source.retry_pending = false;
    if let Err(error) = state.save(folder) {
        if let Some(source) = state.sources.get_mut(id) { source.retry_pending = true; }
        return Err(error);
    }
    Ok(())
}
fn collect_audio(root: &Path, keys: &mut HashSet<String>, titles: &mut HashSet<String>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_audio(&path, keys, titles);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
        {
            let (title, artist) = split_track_name(&path.to_string_lossy());
            keys.insert(sync_key(&artist, &title));
            titles.insert(sync_key("", &title));
        }
    }
}
fn safe_name(artist: &str, title: &str) -> String {
    let raw = if artist.is_empty() {
        title.to_owned()
    } else {
        format!("{artist} - {title}")
    };
    let cleaned: String = raw
        .chars()
        .filter(|c| !r#"<>:"/\\|?*"#.contains(*c))
        .collect();
    if cleaned.trim().is_empty() {
        "Unknown".into()
    } else {
        cleaned.trim().into()
    }
}
fn permanent_failure(error: &str) -> bool {
    let e = error.to_ascii_lowercase();
    [
        "video unavailable",
        "sign in to confirm your age",
        "blocked due to the claimed content",
    ]
    .iter()
    .any(|m| e.contains(m))
}

fn unavailable_failure(error: &str) -> bool {
    error.to_ascii_lowercase().contains("video unavailable")
}

pub fn youtube_watch_url(id: &str) -> Option<String> {
    (id.len() == 11 && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'))
        .then(|| format!("https://www.youtube.com/watch?v={id}"))
}

fn valid_marker_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}
