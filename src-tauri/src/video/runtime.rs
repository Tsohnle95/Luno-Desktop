//! Live-app video pipeline: YouTube relevance search through the discovered
//! yt-dlp binary, temporary staging through `video::staging`, and the
//! VideoState payload shared with dispatch. Process invocations are isolated
//! here so state.rs stays thin and tests can inject fakes.

use crate::video::source::{lookup, VideoCandidate, VideoSource};
use crate::video::staging::{Slot, StageError, VideoStage};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Search yt-dlp (`ytsearchN:`) and parse one JSON object per line.
pub fn yt_search(helper: &Path, query: &str) -> Vec<VideoCandidate> {
    let output = Command::new(helper)
        .args(["--dump-json", "--no-warnings", "--flat-playlist", "--playlist-items", "1:5", &format!("ytsearch5:{query}")])
        .output();
    let Ok(output) = output else { return Vec::new() };
    if !output.status.success() { return Vec::new(); }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
            let id = value.get("id")?.as_str()?.to_string();
            if id.is_empty() { return None; }
            let url = value.get("webpage_url").and_then(|u| u.as_str()).map(str::to_string).unwrap_or_else(|| crate::video::source::canonical_url(&id));
            Some(VideoCandidate {
                title: value.get("title").and_then(|t| t.as_str()).unwrap_or(&id).to_string(),
                artist: value.get("uploader").and_then(|u| u.as_str()).map(str::to_string),
                id,
                url,
            })
        })
        .collect()
}

/// Relevance-filtered candidates for an artist/title pair.
pub fn find_sources(helper: &Path, artist: Option<&str>, title: &str) -> Vec<VideoSource> {
    lookup(artist, title, |query| yt_search(helper, query))
}

/// Download the best ≤720p H.264 video for `url`. WKWebView plays H.264/AAC
/// natively (VP9/AV1 do not), and YouTube frequently serves ONLY segmented
/// HLS streams here — so codec preference matters more than container
/// filtering, and ffmpeg (via --ffmpeg-location) merges video+audio.
pub fn download_video(helper: &Path, ffmpeg_dir: Option<&Path>, url: &str, destination: &Path, cancel: &Arc<AtomicBool>) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) { return Err("cancelled".into()); }
    let mut command = Command::new(helper);
    command.args([
        "--no-warnings", "--no-playlist",
        "-f", "bv*[vcodec^=avc1][height<=720]+ba/bv*[vcodec^=avc1][height<=720]/b",
        "--merge-output-format", "mp4",
        "-o",
    ]);
    command.arg(destination);
    if let Some(dir) = ffmpeg_dir { command.arg("--ffmpeg-location").arg(dir); }
    command.arg(url);
    let mut child = command.spawn().map_err(|error| format!("video downloader could not start: {error}"))?;
    loop {
        if cancel.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(destination);
            return Err("cancelled".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let _ = fs::remove_file(destination);
                    return Err("video download failed".into());
                }
                if !destination.is_file() { return Err("video download produced no output".into()); }
                return Ok(());
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => return Err(format!("video download failed: {error}")),
        }
    }
}

pub enum VideoOutcome {
    Staged { source_url: String, staged_path: PathBuf },
    Unavailable(String),
}

/// Resolve + stage the best video for a track. Runs on a worker thread; the
/// stager is the app-lifetime store. Each run begins its OWN staging
/// generation — `adopt` rejects anything else (StaleGeneration), so callers
/// must never pass foreign counters here.
pub fn stage_track(
    helper: &Path,
    ffmpeg_dir: Option<&Path>,
    stager: &mut VideoStage,
    artist: Option<&str>,
    title: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<VideoOutcome, StageError> {
    if cancel.load(Ordering::Acquire) { return Ok(VideoOutcome::Unavailable("cancelled".into())); }
    let sources = find_sources(helper, artist, title);
    eprintln!("[luno-video] search for \"{artist:?} - {title}\" -> {} candidates", sources.len());
    let Some(source) = sources.into_iter().next() else {
        return Ok(VideoOutcome::Unavailable("no relevant video found".into()));
    };
    if cancel.load(Ordering::Acquire) { return Ok(VideoOutcome::Unavailable("cancelled".into())); }
    let slot_generation = stager.begin_generation();
    let destination = stager.external_destination(Slot::Current, &source.id)?;
    // A previous completed download of the SAME video is reused as-is.
    if destination.is_file() && fs::metadata(&destination).map(|m| m.len() > 0).unwrap_or(false) {
        eprintln!("[luno-video] reusing cached download {}", destination.display());
    } else {
        eprintln!("[luno-video] downloading {}", source.url);
        if let Err(message) = download_video(helper, ffmpeg_dir, &source.url, &destination, cancel) {
            let _ = fs::remove_file(&destination);
            eprintln!("[luno-video] download failed: {message}");
            return Ok(VideoOutcome::Unavailable(message));
        }
    }
    match stager.adopt(Slot::Current, slot_generation, &destination) {
        Ok(staged) => {
            eprintln!("[luno-video] staged -> {}", staged.path.clone().unwrap_or_else(|| destination.clone()).display());
            Ok(VideoOutcome::Staged { source_url: source.url.clone(), staged_path: staged.path.unwrap_or(destination) })
        }
        Err(error) => {
            eprintln!("[luno-video] adopt failed: {error:?}");
            Err(error)
        }
    }
}

/// Snapshot JSON shape shared by dispatch arms.
pub fn video_state_json(state: &crate::contracts::events::VideoState) -> serde_json::Value {
    json!({
        "track_id": state.track_id.as_ref().map(|id| id.0.clone()),
        "source_url": state.source_url,
        "staged_path": state.staged_path,
        "status": state.status,
        "position_ms": state.position_ms,
        "fullscreen": state.fullscreen,
        "generation": state.generation.0,
    })
}
