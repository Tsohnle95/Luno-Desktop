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
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Run a child process to completion with a hard wall-clock deadline.
/// Every helper invocation here used to block forever on a wedged network or
/// subprocess, leaving the UI on "Loading video preview..." permanently —
/// a staged pipeline must always settle, even if only with an error.
/// Pipes are drained by reader threads so a chatty child can never fill the
/// OS pipe buffer and deadlock against our poll loop.
fn run_with_timeout(mut command: Command, timeout: Duration) -> Result<std::process::Output, String> {
    command.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|error| format!("process could not start: {error}"))?;
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stdout_pipe.as_mut() {
            use std::io::Read;
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stderr_pipe.as_mut() {
            use std::io::Read;
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(std::process::Output { status, stdout: stdout_reader.join().unwrap_or_default(), stderr: stderr_reader.join().unwrap_or_default() });
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("timed out after {}s", timeout.as_secs()));
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(format!("process wait failed: {error}")),
        }
    }
}

const YT_SEARCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Search yt-dlp (`ytsearchN:`) and parse one JSON object per line. Eight
/// raw results per form keep the relaxed fallback tier in `video::source::lookup`
/// supplied — five left one-refused-upload dead ends with no alternates.
pub fn yt_search(helper: &Path, query: &str) -> Vec<VideoCandidate> {
    let mut command = Command::new(helper);
    command.args([
        "--dump-json",
        "--no-warnings",
        "--socket-timeout",
        "15",
        "--flat-playlist",
        "--playlist-items",
        "1:8",
        &format!("ytsearch8:{query}"),
    ]);
    let output = match run_with_timeout(command, YT_SEARCH_TIMEOUT) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("[luno-video] yt search failed: {error}");
            return Vec::new();
        }
    };
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
///
/// Audio prefers AAC (`acodec^=mp4a`): YouTube's plain `ba` best-audio is
pub struct VideoProgress {
    /// Latest yt-dlp percentage (0–100), clamped.
    pub percent: AtomicU8,
    /// Set once when the first real progress arrives; lets the worker publish
    /// "downloading" state only when there is something to show.
    pub started: AtomicBool,
}

impl VideoProgress {
    pub fn new() -> Self {
        Self { percent: AtomicU8::new(0), started: AtomicBool::new(false) }
    }
}

fn extract_percent(line: &str) -> Option<u8> {
    // yt-dlp prints lines like "[download]  42.3% of ~150.00MiB at ...";
    // tolerate both fixed and unknown-total variants by taking what we get.
    let idx = line.find("%")?;
    let prefix = &line[..idx];
    let start = prefix
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_ascii_digit() || *c == '.'))
        .map(|(i, _)| i + 1)
        .unwrap_or(0);
    let number: f32 = prefix[start..].trim().parse().ok()?;
    if !(0.0..=100.0).contains(&number) { return None; }
    Some(number.round() as u8)
}

/// Download with live progress reporting into `progress`. Same behaviour as
/// `download_video` otherwise (candidate loop lives in the caller).
fn download_video_with_progress(
    helper: &Path,
    ffmpeg_dir: Option<&Path>,
    url: &str,
    destination: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<VideoProgress>,
) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) { return Err("cancelled".into()); }
    let mut command = Command::new(helper);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    // NOTE: yt-dlp writes its progress lines to STDOUT (--newline), not
    // stderr — the first progress implementation read stderr and could never
    // see a percentage.
    command.args(["--newline",
        // Quality ladder (2026-08-24): prefer H.264 up to 1080p (WKWebView
        // hardware-decodes it), then any codec <=1080p so VP9/AV1-only uploads
        // still land instead of dead-ending at 360p. ensure_playable_audio
        // transcodes non-AAC sound afterwards, so audio stays compatible.
        "-f", "bv*[vcodec^=avc1][height<=1080]+ba[acodec^=mp4a]/bv*[vcodec^=avc1][height<=1080]+ba/bv*[height<=1080][vcodec^=vp09]+ba/bv*[height<=1080]+ba/bv*[height<=1080]/b",
        // Parallel DASH fragment pulls — cuts long-video download times hard
        // (single-stream YouTube throttling was the 50s+ wait on a 15-min film).
        "--concurrent-fragments", "5",
        "--merge-output-format", "mp4",
        "-o",
    ]);
    command.arg(destination);
    if let Some(dir) = ffmpeg_dir { command.arg("--ffmpeg-location").arg(dir); }
    command.arg(url);
    let mut child = command.spawn().map_err(|error| format!("video downloader could not start: {error}"))?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let progress_reader = Arc::clone(progress);
    let progress_errcopy = Arc::clone(progress);
    // Progress lines flow on STDOUT; ERROR lines on stderr. Drain both.
    let stdout_reader = std::thread::spawn(move || {
        if let Some(stdout) = stdout.as_mut() {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(percent) = extract_percent(&line) {
                    progress_reader.percent.store(percent, Ordering::Release);
                    progress_reader.started.store(true, Ordering::Release);
                }
            }
        }
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut collected = String::new();
        if let Some(stderr) = stderr.as_mut() {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stderr).lines().map_while(Result::ok) {
                collected.push_str(&line);
                collected.push('\n');
                if let Some(percent) = extract_percent(&line) {
                    progress_errcopy.percent.store(percent, Ordering::Release);
                    progress_errcopy.started.store(true, Ordering::Release);
                }
            }
        }
        collected
    });
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let reason = stderr_reader.join().unwrap_or_default();
                if !status.success() {
                    let _ = fs::remove_file(destination);
                    let detail = reason.lines().rev().find(|line| line.contains("ERROR")).unwrap_or("video download failed");
                    return Err(detail.chars().take(200).collect());
                }
                if !destination.is_file() { return Err("video download produced no output".into()); }
                return Ok(());
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => return Err(format!("video downloader wait failed: {error}")),
        }
    }
}

/// Opus, which yt-dlp happily muxes into MP4 but WKWebView cannot decode
/// (black stage). The Opus chain remains as availability fallback;
/// `ensure_playable_audio` transcodes those cases during staging.
pub fn download_video(helper: &Path, ffmpeg_dir: Option<&Path>, url: &str, destination: &Path, cancel: &Arc<AtomicBool>) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) { return Err("cancelled".into()); }
    let mut command = Command::new(helper);
    command.stdout(std::process::Stdio::null());
    command.stderr(std::process::Stdio::piped());
    command.args([
        "--no-warnings", "--no-playlist",
        // NO --extractor-args client overrides here: forcing specific clients
        // (tv/web) changes which format ladder YouTube serves and can match
        // NOTHING. The stock rotation works; per-video refusals are handled
        // by falling through to the next search candidate.
        // Quality ladder (2026-08-24): prefer H.264 up to 1080p (WKWebView
        // hardware-decodes it), then any codec <=1080p so VP9/AV1-only uploads
        // still land instead of dead-ending at 360p. ensure_playable_audio
        // transcodes non-AAC sound afterwards, so audio stays compatible.
        "-f", "bv*[vcodec^=avc1][height<=1080]+ba[acodec^=mp4a]/bv*[vcodec^=avc1][height<=1080]+ba/bv*[height<=1080][vcodec^=vp09]+ba/bv*[height<=1080]+ba/bv*[height<=1080]/b",
        // Parallel DASH fragment pulls — cuts long-video download times hard
        // (single-stream YouTube throttling was the 50s+ wait on a 15-min film).
        "--concurrent-fragments", "5",
        "--merge-output-format", "mp4",
        "-o",
    ]);
    command.arg(destination);
    if let Some(dir) = ffmpeg_dir { command.arg("--ffmpeg-location").arg(dir); }
    command.arg(url);
    let mut child = command.spawn().map_err(|error| format!("video downloader could not start: {error}"))?;
    let mut stderr = child.stderr.take();
    let stderr_reader = std::thread::spawn(move || {
        let mut collected = String::new();
        if let Some(stderr) = stderr.as_mut() {
            use std::io::Read;
            let _ = stderr.read_to_string(&mut collected);
        }
        collected
    });
    // Overall wall-clock budget: a wedged download must fail the candidate
    // (and fall through to the next one) instead of staging forever.
    let started = std::time::Instant::now();
    const DOWNLOAD_BUDGET: Duration = Duration::from_secs(300);
    loop {
        if cancel.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(destination);
            return Err("cancelled".into());
        }
        if started.elapsed() >= DOWNLOAD_BUDGET {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(destination);
            return Err("video download timed out after 300s".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let reason = stderr_reader.join().unwrap_or_default();
                if !status.success() {
                    let _ = fs::remove_file(destination);
                    let detail = reason.lines().rev().find(|line| line.contains("ERROR")).unwrap_or("video download failed");
                    return Err(detail.chars().take(200).collect());
                }
                if !destination.is_file() { return Err("video download produced no output".into()); }
                return Ok(());
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => return Err(format!("video download failed: {error}")),
        }
    }
}

/// WKWebView decodes H.264/AAC in MP4 but NOT Opus-in-MP4. yt-dlp's fallback
/// chains can still land Opus audio, so every staged file is probed and, when
/// needed, has ONLY its audio stream transcoded to AAC (video copied) through
/// the bundled ffmpeg. Cheap no-op for files that are already playable.
pub fn ensure_playable_audio(ffmpeg_dir: &Path, path: &Path) -> Result<(), String> {
    let probe = match run_with_timeout(
        {
            let mut command = Command::new(ffmpeg_dir.join("ffprobe"));
            command.args([
                "-v", "error",
                "-select_streams", "a:0",
                "-show_entries", "stream=codec_name",
                "-of", "csv=p=0",
            ]);
            command.arg(path);
            command
        },
        Duration::from_secs(20),
    ) {
        Ok(output) => output,
        Err(error) => return Err(format!("ffprobe: {error}")),
    };
    if !probe.status.success() {
        return Err(format!("ffprobe failed: {}", String::from_utf8_lossy(&probe.stderr).chars().take(200).collect::<String>()));
    }
    let codec = String::from_utf8_lossy(&probe.stdout).trim().to_ascii_lowercase();
    if codec.is_empty() { return Err("staged video has no audio stream".into()); }
    if codec == "aac" || codec == "mp4a" { return Ok(()); }
    let file_name = path.file_name().and_then(|n| n.to_str()).ok_or("staged video has unusable name")?;
    let temporary = path.with_file_name(format!(".{file_name}.remuxing.mp4"));
    let _ = fs::remove_file(&temporary);
    let output = match run_with_timeout(
        {
            let mut command = Command::new(ffmpeg_dir.join("ffmpeg"));
            command.args(["-y", "-v", "error", "-i"]);
            command.arg(path);
            command.args(["-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"]);
            command.arg(&temporary);
            command
        },
        Duration::from_secs(180),
    ) {
        Ok(output) => output,
        Err(error) => return Err(format!("ffmpeg: {error}")),
    };
    if !output.status.success() || !temporary.is_file() {
        let _ = fs::remove_file(&temporary);
        return Err(format!("audio transcode failed: {}", String::from_utf8_lossy(&output.stderr).chars().take(200).collect::<String>()));
    }
    fs::rename(&temporary, path).map_err(|error| format!("remux replace failed: {error}"))?;
    Ok(())
}

/// Adopt a pre-downloaded video for `key`: if video mode previously staged it
/// into the Next slot, move it into the Current slot so this run needs no
/// network. Returns true when a prefetch was promoted.
pub fn promote_prefetched(stager: &VideoStage, key: &str) -> bool {
    let Ok(next) = stager.external_destination(crate::video::staging::Slot::Next, key) else { return false };
    if !(next.is_file() && fs::metadata(&next).map(|m| m.len() > 0).unwrap_or(false)) { return false; }
    let Ok(destination) = stager.external_destination(crate::video::staging::Slot::Current, key) else { return false };
    let _ = fs::remove_file(&destination);
    if fs::rename(&next, &destination).is_ok() {
        eprintln!("[luno-video] adopted prefetched video {key}");
        true
    } else {
        false
    }
}

/// Resolve + pre-download ONE track's best video into the Next slot so a
/// later `stage_track` call promotes it instantly. Never touches the Current
/// slot, so it cannot disturb the video being watched. Idempotent: an
/// already-cached candidate short-circuits.
pub fn prefetch_track(
    helper: &Path,
    ffmpeg_dir: Option<&Path>,
    stager: &VideoStage,
    artist: Option<&str>,
    title: &str,
    cancel: &Arc<AtomicBool>,
) -> bool {
    if cancel.load(Ordering::Acquire) { return false; }
    // Cache-first: if this track already maps to a downloaded video (either
    // slot), there is nothing to fetch — and no reason to pay for a search.
    let track_key = track_index_key(artist, title);
    if let Some(key) = &track_key {
        if stager.index_lookup(key, Slot::Next).is_some() { return true; }
    }
    let sources = find_sources(helper, artist, title);
    eprintln!("[luno-video] prefetch \"{artist:?} - {title}\" -> {} candidates", sources.len());
    let mut last_error = String::from("no candidate could be downloaded");
    for source in sources.into_iter().take(3) {
        if cancel.load(Ordering::Acquire) { return false; }
        let destination = match stager.external_destination(Slot::Next, &source.id) {
            Ok(path) => path,
            Err(_) => return false,
        };
        let cached = destination.is_file() && fs::metadata(&destination).map(|m| m.len() > 0).unwrap_or(false);
        if cached {
            if let Some(key) = &track_key { stager.record_index(key, &source.id); }
            return true;
        }
        if let Err(message) = download_video(helper, ffmpeg_dir, &source.url, &destination, cancel) {
            let _ = fs::remove_file(&destination);
            if message == "cancelled" { return false; }
            eprintln!("[luno-video] prefetch download failed for {}: {message}", source.id);
            last_error = message;
            continue;
        }
        if let Some(dir) = ffmpeg_dir {
            if let Err(message) = ensure_playable_audio(dir, &destination) {
                let _ = fs::remove_file(&destination);
                eprintln!("[luno-video] prefetch unplayable audio for {}: {message}", source.id);
                last_error = message;
                continue;
            }
        }
        eprintln!("[luno-video] prefetched {}", source.id);
        return true;
    }
    let _ = last_error;
    false
}

/// Stable cache-index key for an artist/title pair. Only consistency between
/// stage_track and prefetch_track matters — it never needs to match TrackId.
fn track_index_key(artist: Option<&str>, title: &str) -> Option<String> {
    // NFKC first: macOS metadata frequently carries FULLWIDTH Latin letters
    // (U+FF41-style). Without folding them to ASCII the key diverges between
    // writes and lookups and the cache fast-path misses files we ALREADY have.
    fn fold(value: &str) -> String {
        value
            .chars()
            .map(|c| {
                let code = c as u32;
                if (0xFF01..=0xFF5E).contains(&code) {
                    char::from_u32(code - 0xFEE0).unwrap_or(c)
                } else {
                    c
                }
            })
            .collect()
    }
    let artist = artist.map(fold);
    let title = fold(title);
    let mut key: String = artist.unwrap_or_default().chars().chain(title.chars()).collect();
    key.retain(|c| c.is_alphanumeric());
    let key = key.to_lowercase();
    if key.is_empty() { None } else { Some(key) }
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
    // When Some, CURRENT-slot downloads report live percentages into it so
    // the UI can show "Downloading video… N%" instead of a static spinner.
    progress: Option<&Arc<VideoProgress>>,
) -> Result<VideoOutcome, StageError> {
    if cancel.load(Ordering::Acquire) { return Ok(VideoOutcome::Unavailable("cancelled".into())); }
    // Fast path: an earlier attempt already mapped this track to a YouTube id
    // and downloaded it. Skip the (multi-second) search entirely.
    if let Some(track_key) = track_index_key(artist, title) {
        if let Some((id, cached_path)) = stager.index_lookup(&track_key, Slot::Current) {
            eprintln!("[luno-video] index cache hit {id} for {track_key:?}");
            let slot_generation = stager.begin_generation();
            return match stager.adopt(Slot::Current, slot_generation, &cached_path) {
                Ok(staged) => {
                    eprintln!("[luno-video] staged -> {}", staged.path.clone().unwrap_or_else(|| cached_path.clone()).display());
                    Ok(VideoOutcome::Staged { source_url: crate::video::source::canonical_url(&id), staged_path: staged.path.unwrap_or(cached_path) })
                }
                Err(error) => {
                    eprintln!("[luno-video] adopt failed: {error:?}");
                    stager.index_remove(&track_key);
                    Err(error)
                }
            };
        }
    }
    let sources = find_sources(helper, artist, title);
    eprintln!("[luno-video] search for \"{artist:?} - {title}\" -> {} candidates", sources.len());
    if sources.is_empty() {
        return Ok(VideoOutcome::Unavailable("no relevant video found".into()));
    }
    // Try candidates in relevance order: YouTube refuses SOME licensed videos
    // to every downloader client ("not available on this app"), so the first
    // hit is not always downloadable — fall through to alternates.
    let track_key = track_index_key(artist, title);
    let mut last_error = String::from("no candidate could be downloaded");
    for source in sources.into_iter().take(3) {
        if cancel.load(Ordering::Acquire) { return Ok(VideoOutcome::Unavailable("cancelled".into())); }
        let slot_generation = stager.begin_generation();
        let destination = match stager.external_destination(Slot::Current, &source.id) { Ok(path) => path, Err(error) => return Err(error) };
        let mut cached = destination.is_file() && fs::metadata(&destination).map(|m| m.len() > 0).unwrap_or(false);
        // Video-mode prefetch: an earlier pass may have downloaded this exact
        // candidate into the Next slot — promote it instead of the network.
        if !cached && promote_prefetched(stager, &source.id) {
            cached = true;
        }
        eprintln!("[luno-video] candidate {} (cached: {cached})", source.id);
        if !cached {
            let result = match progress {
                Some(reporter) => {
                    reporter.percent.store(0, Ordering::Release);
                    download_video_with_progress(helper, ffmpeg_dir, &source.url, &destination, cancel, reporter)
                }
                None => download_video(helper, ffmpeg_dir, &source.url, &destination, cancel),
            };
            if let Err(message) = result {
                let _ = fs::remove_file(&destination);
                eprintln!("[luno-video] download failed for {}: {message}", source.id);
                last_error = message;
                continue;
            }
        }
        // Cached files may predate the AAC-first policy — verify both paths.
        if let Some(dir) = ffmpeg_dir {
            if let Err(message) = ensure_playable_audio(dir, &destination) {
                let _ = fs::remove_file(&destination);
                eprintln!("[luno-video] unplayable audio for {}: {message}", source.id);
                last_error = message;
                continue;
            }
        }
        return match stager.adopt(Slot::Current, slot_generation, &destination) {
            Ok(staged) => {
                if let Some(key) = &track_key { stager.record_index(key, &source.id); }
                eprintln!("[luno-video] staged -> {}", staged.path.clone().unwrap_or_else(|| destination.clone()).display());
                Ok(VideoOutcome::Staged { source_url: source.url.clone(), staged_path: staged.path.unwrap_or(destination) })
            }
            Err(error) => {
                eprintln!("[luno-video] adopt failed: {error:?}");
                Err(error)
            }
        };
    }
    Ok(VideoOutcome::Unavailable(last_error))
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

#[cfg(test)]
mod runtime_tests {
    use super::*;

    #[test]
    fn extract_percent_parses_ytdlp_progress_lines() {
        assert_eq!(extract_percent("[download]  42.3% of ~150.00MiB at 2.10MiB/s"), Some(42));
        assert_eq!(extract_percent("[download]   0.0% of 12.34MiB"), Some(0));
        assert_eq!(extract_percent("[download] 100% of 12.34MiB"), Some(100));
        assert_eq!(extract_percent("[download] Destination: already downloaded"), None);
        assert_eq!(extract_percent("no numbers here %"), None);
        assert_eq!(extract_percent("[download] 150.0% of x"), None);
    }

    #[test]
    fn track_index_key_folds_fullwidth_latin_to_ascii() {
        // macOS metadata sometimes carries fullwidth letters (U+FF53 'ｓ').
        let folded = track_index_key(Some("1995 ｓｏｆｔｃｌｏｃｋ"), "Chillwave Memory Retro Loops").unwrap();
        assert_eq!(folded, "1995softclockchillwavememoryretroloops");
        let plain = track_index_key(Some("Metallica"), "I Disappear").unwrap();
        assert_eq!(plain, "metallicaidisappear");
    }
}
