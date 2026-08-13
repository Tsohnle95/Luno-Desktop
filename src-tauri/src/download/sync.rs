//! Playlist synchronization policy, independent of provider/UI plumbing.

use crate::domain::normalization::{split_track_name, sync_key};
use crate::download::markers::Markers;
use crate::runtime::task_registry::CancellationToken;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const ITEM_TIMEOUT: Duration = Duration::from_secs(120);
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "wav", "flac", "webm"];
#[derive(Debug, Clone, PartialEq, Eq)] pub struct PlaylistEntry { pub id: Option<String>, pub title: String, pub url: Option<String> }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct SyncItem { pub entry: PlaylistEntry, pub destination: PathBuf }
#[derive(Debug, Clone, PartialEq, Eq)] pub enum ItemOutcome { Matched, Downloaded, PermanentFailure(String), Failed(String), TimedOut, Cancelled }
#[derive(Debug, Clone, Default, PartialEq, Eq)] pub struct SyncResult { pub outcomes: Vec<(String, ItemOutcome)>, pub cancelled: bool }

pub fn playlist_url(input: &str) -> Option<String> { let input = input.trim(); if input.is_empty() { return None; } let list = input.split(['?', '&']).find_map(|part| part.strip_prefix("list=")).or_else(|| (!input.contains("/") && !input.contains('?')).then_some(input))?; (!list.is_empty()).then(|| format!("https://www.youtube.com/playlist?list={list}")) }
pub fn parse_title(title: &str) -> (String, String) { title.split_once(" - ").map_or((String::new(), title.trim().to_owned()), |(a, t)| (a.trim().to_owned(), t.trim().to_owned())) }

pub fn plan(root: impl AsRef<Path>, destination: impl AsRef<Path>, entries: &[PlaylistEntry]) -> Vec<SyncItem> {
    let root = root.as_ref(); let destination = destination.as_ref(); let mut keys = HashSet::new(); let mut titles = HashSet::new(); collect_audio(root, &mut keys, &mut titles);
    entries.iter().filter_map(|entry| { let (artist, title) = parse_title(&entry.title); let key = sync_key(&artist, &title); let title_key = sync_key("", &title); if (key.is_empty() && title_key.is_empty()) || keys.contains(&key) || (artist.is_empty() && titles.contains(&title_key)) { return None; } let safe = safe_name(&artist, &title); if AUDIO_EXTENSIONS.iter().any(|ext| destination.join(format!("{safe}.{ext}")).exists()) { return None; } keys.insert(key); titles.insert(title_key); Some(SyncItem { entry: entry.clone(), destination: destination.join(format!("{safe}.mp3")) }) }).collect()
}

pub fn sync_sequential<F>(folder: impl AsRef<Path>, items: &[SyncItem], token: &CancellationToken, mut download: F) -> SyncResult where F: FnMut(&SyncItem, &CancellationToken) -> Result<(), String> {
    let folder = folder.as_ref(); let mut markers = Markers::load(folder); let mut result = SyncResult::default();
    for item in items { if token.is_cancelled() { result.cancelled = true; break; } let label = item.entry.id.clone().unwrap_or_else(|| item.entry.title.clone()); if let Some(id) = item.entry.id.as_deref() { if markers.successful.contains(id) { result.outcomes.push((label, ItemOutcome::Matched)); continue; } if markers.failed.contains(id) { result.outcomes.push((label, ItemOutcome::PermanentFailure("previously recorded failure".into()))); continue; } } let started = Instant::now(); let outcome = match download(item, token) { Ok(()) if token.is_cancelled() => ItemOutcome::Cancelled, Ok(()) if started.elapsed() > ITEM_TIMEOUT => ItemOutcome::TimedOut, Ok(()) if !is_audio_output(&item.destination) => ItemOutcome::Failed("validated audio output is missing".into()), Ok(()) => { if let Some(id) = item.entry.id.as_deref() { if let Err(error) = markers.record_success(folder, id) { return SyncResult { outcomes: vec![(label, ItemOutcome::Failed(format!("could not record success marker: {error}")))], cancelled: result.cancelled }; } } ItemOutcome::Downloaded }, Err(_) if token.is_cancelled() => ItemOutcome::Cancelled, Err(error) if started.elapsed() > ITEM_TIMEOUT => ItemOutcome::TimedOut, Err(error) if permanent_failure(&error) => { if let Some(id) = item.entry.id.as_deref() { let _ = markers.record_failure(folder, id, &error); } ItemOutcome::PermanentFailure(error) }, Err(error) => ItemOutcome::Failed(error) }; let cancelled = matches!(outcome, ItemOutcome::Cancelled); result.outcomes.push((label, outcome)); if cancelled { result.cancelled = true; break; } } result
}
fn collect_audio(root: &Path, keys: &mut HashSet<String>, titles: &mut HashSet<String>) { let Ok(entries) = fs::read_dir(root) else { return }; for entry in entries.flatten() { let path = entry.path(); if path.is_dir() { collect_audio(&path, keys, titles); } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e))) { let (title, artist) = split_track_name(&path.to_string_lossy()); keys.insert(sync_key(&artist, &title)); titles.insert(sync_key("", &title)); } } }
fn safe_name(artist: &str, title: &str) -> String { let raw = if artist.is_empty() { title.to_owned() } else { format!("{artist} - {title}") }; let cleaned: String = raw.chars().filter(|c| !r#"<>:"/\\|?*"#.contains(*c)).collect(); if cleaned.trim().is_empty() { "Unknown".into() } else { cleaned.trim().into() } }
fn is_audio_output(path: &Path) -> bool { path.is_file() && fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false) }
fn permanent_failure(error: &str) -> bool { let e = error.to_ascii_lowercase(); ["video unavailable", "sign in to confirm your age", "blocked due to the claimed content"].iter().any(|m| e.contains(m)) }
