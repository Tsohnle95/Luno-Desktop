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
///
/// The query forms are searched CONCURRENTLY: `lookup` consumes them
/// sequentially in a fixed order, so all results are gathered up front and
/// served from buffers. Sequential forms paid 1–3 back-to-back yt-dlp
/// searches (~5–15 s each) before the download could even start — the wall
/// time now collapses to roughly ONE search regardless of how many forms the
/// relevance tiers end up needing.
pub fn find_sources(helper: &Path, artist: Option<&str>, title: &str) -> Vec<VideoSource> {
    let forms = crate::video::source::query_forms(artist, title);
    let buffered: Vec<Vec<VideoCandidate>> = thread::scope(|scope| {
        let handles: Vec<_> = forms
            .iter()
            .map(|query| scope.spawn(|| yt_search(helper, query)))
            .collect();
        handles.into_iter().map(|handle| handle.join().unwrap_or_default()).collect()
    });
    let mut form_index = usize::MAX;
    // Form order here MUST match `lookup`'s internal iteration order
    // (`query_forms`) so each buffered result list feeds its own form.
    lookup(artist, title, move |_query| {
        form_index = form_index.wrapping_add(1);
        buffered.get(form_index).cloned().unwrap_or_default()
    })
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

/// High-quality ladder for BACKGROUND work (next-track prefetch and the
/// current-track quality UPGRADE pass): same H.264-first codec preferences
/// capped at 720p. The former ≤1080p ceiling produced single staged files of
/// 118–197 MB; at 720p they land roughly 3–5× smaller while WKWebView still
/// hardware-decodes them, so one song switch can no longer saturate disk,
/// network, and CPU for minutes. ensure_playable_audio transcodes non-AAC
/// sound afterwards, so audio stays compatible.
pub const FORMAT_LADDER_HIGH: &str = "bv*[vcodec^=avc1][height<=720]+ba[acodec^=mp4a]/bv*[vcodec^=avc1][height<=720]/bv*[height<=720][vcodec^=vp09]+ba/bv*[height<=720]+ba/bv*[height<=720]/b";

/// Quick-tier ladder for INTERACTIVE current-track staging: same codec
/// preferences capped at 480p, so the file is a fraction of the size and the
/// picture appears seconds after the search instead of after a full-quality
/// pull. A background pass then replaces it with the ≤720p version
/// (`upgrade_staged_video`). WKWebView plays 480p H.264/AAC natively.
/// Quick-tier ladder for INTERACTIVE current-track staging: same codec
/// preferences capped at **360p** (2026-08-26 audit — a 480p pull of long
/// music films still meant hundreds of MB and multi-minute first frames;
/// 360p typically lands in seconds-to-tens-of-seconds). A background pass
/// then replaces it with the ≤720p version (`upgrade_staged_video`).
/// WKWebView plays 360p H.264/AAC natively.
pub const FORMAT_LADDER_QUICK: &str = "bv*[vcodec^=avc1][height<=360]+ba[acodec^=mp4a]/bv*[vcodec^=avc1][height<=360]/bv*[height<=360][vcodec^=vp09]+ba/bv*[height<=360]+ba/bv*[height<=360]/b";

/// Single download implementation behind both tiers. `ladder` selects the
/// yt-dlp `-f` chain; `progress` (when given) receives live percentages.
///
/// NOTE: yt-dlp writes its progress lines to STDOUT (--newline), not stderr —
/// the first progress implementation read stderr and could never see a
/// percentage.
fn download_ladder(
    helper: &Path,
    ffmpeg_dir: Option<&Path>,
    url: &str,
    destination: &Path,
    cancel: &Arc<AtomicBool>,
    ladder: &str,
    progress: Option<&Arc<VideoProgress>>,
) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) { return Err("cancelled".into()); }
    let mut command = Command::new(helper);
    let report_progress = progress.is_some();
    if report_progress {
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
    } else {
        command.stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::piped());
    }
    command.args(["--newline",
        // NO --extractor-args client overrides here: forcing specific clients
        // (tv/web) changes which format ladder YouTube serves and can match
        // NOTHING. The stock rotation works; per-video refusals are handled
        // by falling through to the next search candidate.
        "-f", ladder,
        // Parallel DASH fragment pulls — cuts long-video download times hard
        // (single-stream YouTube throttling was the 50s+ wait on a 15-min film).
        "--concurrent-fragments", "8",
        "--merge-output-format", "mp4",
        "-o",
    ]);
    command.arg(destination);
    if let Some(dir) = ffmpeg_dir { command.arg("--ffmpeg-location").arg(dir); }
    command.arg(url);
    let mut child = command.spawn().map_err(|error| format!("video downloader could not start: {error}"))?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let reporter = Arc::clone(progress.unwrap_or(&Arc::new(VideoProgress::new())));
    // STDERR is ALWAYS drained by a reader thread — an unread pipe fills its
    // fixed OS buffer, the child blocks on write and never exits.
    let stderr_reader = std::thread::spawn(move || {
        let mut collected = String::new();
        if let Some(pipe) = stderr.as_mut() {
            use std::io::BufRead;
            for line in std::io::BufReader::new(pipe).lines().map_while(Result::ok) {
                collected.push_str(&line);
                collected.push('\n');
                if let Some(percent) = extract_percent(&line) {
                    reporter.percent.store(percent, Ordering::Release);
                    reporter.started.store(true, Ordering::Release);
                }
            }
        }
        collected
    });
    // Progress lines flow on STDOUT; ERROR lines on stderr. The join handle
    // is dropped here — the loop below only needs the stderr reader's result.
    if report_progress {
        let progress_reader = Arc::clone(progress.unwrap_or(&Arc::new(VideoProgress::new())));
        std::thread::spawn(move || {
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
    }
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
            Err(error) => return Err(format!("video downloader wait failed: {error}")),
        }
    }
}


/// Opus, which yt-dlp happily muxes into MP4 but WKWebView cannot decode
/// (black stage). The Opus chain remains as availability fallback;
/// `ensure_playable_audio` transcodes those cases during staging.
///
/// Prefetch runs the ≤720p HIGH ladder (not the old ≤1080p one): a prefetched
/// track is downloaded BEFORE the user reaches it, and a single 1080p pull of
/// a long video starved disk/network long enough to freeze the whole window.
pub fn download_video(helper: &Path, ffmpeg_dir: Option<&Path>, url: &str, destination: &Path, cancel: &Arc<AtomicBool>) -> Result<(), String> {
    download_ladder(helper, ffmpeg_dir, url, destination, cancel, FORMAT_LADDER_HIGH, None)
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
        // A Current-slot hit means the video is already staged for viewing;
        // recording nothing would make a later restage re-search anyway.
        if stager.index_lookup(key, Slot::Current).is_some() { return true; }
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
        if !cached {
            // DEDUPE (2026-08-26 audit): the SAME video may already sit in the
            // CURRENT slot from an earlier play. Re-downloading it into Next
            // duplicated full files (an 88 MB video existed as BOTH
            // current-<id> and next-<id>) and stole bandwidth from the track
            // actually playing. Index the existing copy instead.
            let current_copy = stager.external_destination(Slot::Current, &source.id).ok()
                .filter(|path| path.is_file() && fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false));
            if current_copy.is_some() {
                if let Some(key) = &track_key {
                    stager.record_index_tier(key, &source.id, stager.tier_for_video_id(&source.id));
                }
                eprintln!("[luno-video] prefetch {} already staged as current; skipping download", source.id);
                return true;
            }
        }
        if cached {
            if let Some(key) = &track_key {
                stager.record_index_tier(key, &source.id, stager.tier_for_video_id(&source.id));
            }
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
        // RECORD THE INDEX on the fresh-download path too. Missing this line
        // was the cued-handoff bug: the Next-slot MP4 existed, but stage_track
        // consults the index BEFORE searching — with no entry it paid the
        // whole multi-search YouTube lookup again before finding the very
        // file prefetch had just parked (the "skipped song still spun ~30s"
        // report). With the entry, index_lookup(Current) auto-promotes
        // next-<id>.mp4 and the handoff is instant and network-free.
        if let Some(key) = &track_key { stager.record_index(key, &source.id); }
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

#[derive(Debug)]
pub enum VideoOutcome {
    Staged {
        source_url: String,
        staged_path: PathBuf,
        /// True when the file was just downloaded through the QUICK (≤480p)
        /// ladder: a background pass should replace it with the high ≤720p
        /// version. Cached/promoted files are already full quality.
        needs_upgrade: bool,
    },
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
        if let Some((id, cached_path, tier)) = stager.index_lookup_with_tier(&track_key, Slot::Current) {
            eprintln!("[luno-video] index cache hit {id} for {track_key:?} (tier {tier})");
            let slot_generation = stager.begin_generation();
            return match stager.adopt(Slot::Current, slot_generation, &cached_path) {
                Ok(staged) => {
                    eprintln!("[luno-video] staged -> {}", staged.path.clone().unwrap_or_else(|| cached_path.clone()).display());
                    Ok(VideoOutcome::Staged {
                        source_url: crate::video::source::canonical_url(&id),
                        staged_path: staged.path.unwrap_or(cached_path),
                        // "quick"-tier entries owe their HD upgrade — it was
                        // interrupted (app restart / superseded by a skip).
                        // Re-run it instead of serving 480p as final forever.
                        needs_upgrade: tier == crate::video::staging::INDEX_TIER_QUICK,
                    })
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
        // TWO-TIER STAGING (2026-08-24): interactive current-track downloads
        // use the QUICK ≤480p ladder so the picture appears in seconds; the
        // worker then upgrades the file to ≤720p in the background.
        // Cached/promoted candidates are already full quality.
        let needs_upgrade = !cached;
        if !cached {
            let result = match progress {
                Some(reporter) => {
                    reporter.percent.store(0, Ordering::Release);
                    download_ladder(helper, ffmpeg_dir, &source.url, &destination, cancel, FORMAT_LADDER_QUICK, Some(reporter))
                }
                None => download_ladder(helper, ffmpeg_dir, &source.url, &destination, cancel, FORMAT_LADDER_QUICK, None),
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
                // Record the tier with the id: "quick" when the HD upgrade is
                // still pending (so an interrupted upgrade re-runs on the
                // next play instead of 480p becoming permanent), "high" for
                // cached/promoted final-quality files.
                if let Some(key) = &track_key {
                    stager.record_index_tier(
                        key,
                        &source.id,
                        if needs_upgrade { crate::video::staging::INDEX_TIER_QUICK } else { crate::video::staging::INDEX_TIER_HIGH },
                    );
                }
                eprintln!("[luno-video] staged -> {}", staged.path.clone().unwrap_or_else(|| destination.clone()).display());
                Ok(VideoOutcome::Staged {
                    source_url: source.url.clone(),
                    staged_path: staged.path.unwrap_or(destination),
                    needs_upgrade,
                })
            }
            Err(error) => {
                eprintln!("[luno-video] adopt failed: {error:?}");
                Err(error)
            }
        };
    }
    Ok(VideoOutcome::Unavailable(last_error))
}

/// Replace a quick-tier staged file with its high-quality (≤720p) version IN PLACE.
/// Downloads to a hidden temp file NEXT TO the staged MP4 (so startup sweeps
/// keep working), verifies playable audio, then atomically renames over it —
/// a webview already streaming the old file keeps its open handle and simply
/// finishes on it.
///
/// `destination` must come from `VideoStage::external_destination(Current, …)`
/// resolved by the caller UNDER A SHORT LOCK — this function deliberately
/// takes no locks so the prefetch worker can use the store concurrently.
/// The video id (and therefore the staged filename) does not change, so the
/// cache index stays valid. Callers republish VideoState afterwards so the
/// UI can reload the element at the higher quality.
pub fn upgrade_staged_video(
    helper: &Path,
    ffmpeg_dir: Option<&Path>,
    destination: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) { return Err("cancelled".into()); }
    if !(destination.is_file() && fs::metadata(destination).map(|m| m.len() > 0).unwrap_or(false)) {
        return Err("staged video vanished before upgrade".into());
    }
    let file_name = destination.file_name().and_then(|n| n.to_str()).ok_or("staged video has unusable name")?;
    let temporary = destination.with_file_name(format!(".upgrading-{file_name}"));
    let _ = fs::remove_file(&temporary);
    let result = download_ladder(helper, ffmpeg_dir, &crate::video::source::canonical_url(video_id_from_destination(file_name)), &temporary, cancel, FORMAT_LADDER_HIGH, None)
        .and_then(|_| {
            if let Some(dir) = ffmpeg_dir {
                ensure_playable_audio(dir, &temporary)?;
            }
            Ok(())
        });
    match result {
        Ok(()) => {
            // Atomic swap: readers of the old inode finish undisturbed; the
            // next open sees the upgraded file.
            fs::rename(&temporary, destination).map_err(|error| format!("upgrade replace failed: {error}"))?;
            eprintln!("[luno-video] upgraded {} to full quality", video_id_from_destination(file_name));
            Ok(())
        }
        Err(message) => {
            let _ = fs::remove_file(&temporary);
            eprintln!("[luno-video] upgrade skipped for {}: {message}", video_id_from_destination(file_name));
            Err(message)
        }
    }
}

/// Recover the YouTube id from a staged file name (`current-<id>.mp4`).
fn video_id_from_destination(file_name: &str) -> &str {
    file_name.strip_prefix("current-").and_then(|rest| rest.strip_suffix(".mp4")).unwrap_or(file_name)
}

/// Snapshot JSON shape shared by dispatch arms.
pub fn video_state_json(state: &crate::contracts::events::VideoState) -> serde_json::Value {
    json!({
        "track_id": state.track_id.as_ref().map(|id| id.0.clone()),
        "source_url": state.source_url,
        "staged_path": state.staged_path,
        "status": state.status,
        "position_ms": state.position_ms,
        "download_progress": state.download_progress,
        "revision": state.revision,
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

    #[test]
    fn stage_track_index_hit_needs_no_network_and_reports_full_quality() {
        // REGRESSION (cued-handoff): prefetch downloaded the video but the
        // fast path must adopt it with ZERO searches. A nonexistent helper
        // proves no yt-dlp process is spawned; any search attempt would fail
        // the outcome instead of staging.
        let root = std::env::temp_dir().join(format!("luno-stage-fast-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let stager = VideoStage::at(&root, None).expect("stager");
        let key = "artisttitle";
        let id = "abc123";
        stager.record_index(key, id);
        let destination = stager.external_destination(Slot::Current, id).unwrap();
        fs::write(&destination, b"fake mp4 bytes").expect("cached file");
        let mut stager = stager;
        let outcome = stage_track(
            Path::new("/nonexistent/yt-dlp"),
            None,
            &mut stager,
            Some("Artist"),
            "Title",
            &Arc::new(AtomicBool::new(false)),
            None,
        )
        .expect("index hit must stage");
        match outcome {
            VideoOutcome::Staged { staged_path, needs_upgrade, .. } => {
                assert_eq!(staged_path, destination);
                // Index-adopted files come from full-ladder runs.
                assert!(!needs_upgrade);
            }
            other => panic!("expected Staged, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// AUDIT (2026-08-25): a quick-tier file whose HD upgrade was interrupted
    /// (restart / superseded by a skip) is indexed with tier "quick"; the next
    /// stage MUST report needs_upgrade so the HD pass re-runs instead of
    /// serving 480p as final forever.
    #[test]
    fn index_hit_with_quick_tier_reports_needs_upgrade() {
        let root = std::env::temp_dir().join(format!("luno-stage-quick-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let stager = VideoStage::at(&root, None).expect("stager");
        stager.record_index_tier("artisttitle", "abc123", crate::video::staging::INDEX_TIER_QUICK);
        let destination = stager.external_destination(Slot::Current, "abc123").unwrap();
        fs::write(&destination, b"fake 480p bytes").expect("cached quick file");
        let mut stager = stager;
        let outcome = stage_track(
            Path::new("/nonexistent/yt-dlp"),
            None,
            &mut stager,
            Some("Artist"),
            "Title",
            &Arc::new(AtomicBool::new(false)),
            None,
        )
        .expect("index hit must stage");
        match outcome {
            VideoOutcome::Staged { needs_upgrade, .. } => assert!(needs_upgrade, "quick-tier adoption must schedule the HD upgrade"),
            other => panic!("expected Staged, got {other:?}"),
        }
        // And the HIGH-tier sibling stays final: no pointless re-upgrade.
        stager.record_index_tier("artisttitle", "abc123", crate::video::staging::INDEX_TIER_HIGH);
        let outcome = stage_track(Path::new("/nonexistent/yt-dlp"), None, &mut stager, Some("Artist"), "Title", &Arc::new(AtomicBool::new(false)), None).expect("second stage");
        match outcome {
            VideoOutcome::Staged { needs_upgrade, .. } => assert!(!needs_upgrade),
            other => panic!("expected Staged, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn quick_ladder_caps_height_below_high_ladder() {
        // The quick tier must never accidentally equal the high tier: its cap
        // is what makes first-frame latency small, and the high tier's 720p
        // cap is what keeps background prefetch/upgrades from pulling
        // 100 MB+ single files (the 2026-08-25 switch-song freeze).
        assert!(FORMAT_LADDER_QUICK.contains("height<=360"));
        assert!(!FORMAT_LADDER_QUICK.contains("height<=480"));
        assert!(FORMAT_LADDER_HIGH.contains("height<=720"));
        assert!(!FORMAT_LADDER_HIGH.contains("height<=1080"));
    }
}
