//! Live-app video pipeline: YouTube relevance search through the discovered
//! yt-dlp binary, temporary staging through `video::staging`, and the
//! VideoState payload shared with dispatch. Process invocations are isolated
//! here so state.rs stays thin and tests can inject fakes.

use crate::video::source::{lookup, VideoCandidate, VideoSource};
use crate::video::staging::{Slot, StageError, VideoStage};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Run a child process to completion with a hard wall-clock deadline.
/// Every helper invocation here used to block forever on a wedged network or
/// subprocess, leaving the UI on "Loading video preview..." permanently —
/// a staged pipeline must always settle, even if only with an error.
/// Pipes are drained by reader threads so a chatty child can never fill the
/// OS pipe buffer and deadlock against our poll loop.
fn run_with_timeout_cancel(
    mut command: Command,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
) -> Result<std::process::Output, String> {
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
        if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cancelled".into());
        }
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

fn run_with_timeout(command: Command, timeout: Duration) -> Result<std::process::Output, String> {
    run_with_timeout_cancel(command, timeout, None)
}

const YT_SEARCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Search yt-dlp (`ytsearchN:`) and parse one JSON object per line. Eight
/// raw results per form keep the relaxed fallback tier in `video::source::lookup`
/// supplied — five left one-refused-upload dead ends with no alternates.
pub fn yt_search(helper: &Path, query: &str) -> Vec<VideoCandidate> {
    yt_search_cancellable(helper, query, None)
}

fn yt_search_cancellable(helper: &Path, query: &str, cancel: Option<&AtomicBool>) -> Vec<VideoCandidate> {
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
    let output = match run_with_timeout_cancel(command, YT_SEARCH_TIMEOUT, cancel) {
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
/// Search fallback forms sequentially. The bundled yt-dlp is a heavyweight
/// PyInstaller executable; launching every form concurrently starves the
/// interactive audio-preview resolver and leaves it waiting before any bytes
/// exist to report. Cancellation is checked by every individual search.
pub fn find_sources(helper: &Path, artist: Option<&str>, title: &str) -> Vec<VideoSource> {
    find_sources_cancellable(helper, artist, title, None)
}

fn find_sources_cancellable(helper: &Path, artist: Option<&str>, title: &str, cancel: Option<&AtomicBool>) -> Vec<VideoSource> {
    // Search lazily. `lookup` stops after three strict candidates, which is
    // also the maximum staging attempts; pre-running every fallback form made
    // a normal video wait through three heavyweight yt-dlp startups.
    let production_helper = helper.file_name().and_then(|name| name.to_str()) == Some("yt-dlp");
    lookup(artist, title, |query| {
        if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) { return Vec::new(); }
        if production_helper {
            match crate::download::search::search_youtube_web(query, 8) {
                Ok(results) if !results.is_empty() => {
                    return results
                        .into_iter()
                        .map(|result| VideoCandidate {
                            id: String::new(),
                            title: result.title,
                            artist: result.artist,
                            url: result.url.unwrap_or(result.query),
                        })
                        .collect();
                }
                Ok(_) => eprintln!("[luno-video] fast search returned no candidates; trying yt-dlp"),
                Err(error) => eprintln!("[luno-video] fast search unavailable: {error}; trying yt-dlp"),
            }
        }
        yt_search_cancellable(helper, query, cancel)
    })
}

/// Download the best ≤720p H.264 video for `url`. WKWebView plays H.264/AAC
/// natively (VP9/AV1 do not), and YouTube frequently serves ONLY segmented
/// HLS streams here — so codec preference matters more than container
/// filtering, and ffmpeg (via --ffmpeg-location) merges video+audio.
///
/// Audio prefers AAC (`acodec^=mp4a`): YouTube's plain `ba` best-audio is
pub struct VideoProgress {
    /// Latest byte-derived percentage (0–100), clamped.
    pub percent: AtomicU8,
    /// Set only after a byte sample with a non-zero downloaded count and a
    /// known total has arrived. Process spawn and extractor setup do not make
    /// a determinate 0% state.
    pub started: AtomicBool,
    samples: Mutex<ProgressAccumulator>,
}

impl VideoProgress {
    pub fn new() -> Self {
        Self {
            percent: AtomicU8::new(0),
            started: AtomicBool::new(false),
            samples: Mutex::new(ProgressAccumulator::default()),
        }
    }

    fn reset(&self) {
        self.percent.store(0, Ordering::Release);
        self.started.store(false, Ordering::Release);
        if let Ok(mut samples) = self.samples.lock() {
            *samples = ProgressAccumulator::default();
        }
    }

    fn record(&self, sample: DownloadProgressSample) {
        let Ok(mut samples) = self.samples.lock() else { return };
        let Some(percent) = samples.record(sample) else { return };
        self.percent.store(percent, Ordering::Release);
        self.started.store(true, Ordering::Release);
    }

    fn complete(&self) {
        // 100 is emitted only after the quick file has passed playable-audio
        // checks and `VideoStage::adopt` has published it into its slot.
        self.percent.store(100, Ordering::Release);
        self.started.store(true, Ordering::Release);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DownloadProgressSample {
    /// The video id + selected format identify separate video/audio items.
    /// None is retained for older/helper output that cannot identify a stream.
    stream_key: Option<String>,
    format_id: Option<String>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    planned_streams: Vec<PlannedStream>,
    /// A combined format has one transfer item; a video-only or audio-only
    /// format is one half of the explicit `bv+ba` ladder.
    expected_streams: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedStream {
    format_id: String,
    total_bytes: Option<u64>,
}

#[derive(Debug, Default)]
struct ProgressAccumulator {
    streams: HashMap<String, StreamProgress>,
    planned_sizes: HashMap<String, Option<u64>>,
    expected_streams: usize,
    last_percent: u8,
    anonymous_stream: u64,
}

#[derive(Debug, Default)]
struct StreamProgress {
    format_id: Option<String>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
}

fn parse_bytes(value: &str) -> Option<u64> {
    match value.trim() {
        "" | "NA" | "None" | "none" | "null" | "N/A" | "-" => None,
        value => value.parse().ok(),
    }
}

fn parse_planned_streams(value: &str) -> Vec<PlannedStream> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(value.trim()) else { return Vec::new() };
    let Some(entries) = value.as_array() else { return Vec::new() };
    entries.iter().filter_map(|entry| {
        let format_id = entry.get("format_id").and_then(|value| value.as_str())?.to_owned();
        let total_bytes = entry.get("filesize").and_then(|value| value.as_u64())
            .or_else(|| entry.get("filesize_approx").and_then(|value| value.as_u64()));
        Some(PlannedStream { format_id, total_bytes })
    }).collect()
}

/// Parse the machine-readable line emitted by `--progress-template`.
///
/// The template deliberately carries downloaded and total byte counts rather
/// than yt-dlp's formatted percent string. A total-byte estimate is accepted
/// when an exact total is unavailable; an unknown denominator remains
/// indeterminate.
fn parse_download_progress(line: &str) -> Option<DownloadProgressSample> {
    let mut fields = line.strip_prefix("download-bytes:")?.split('\t');
    let video_id = fields.next().and_then(|value| parse_stream_key(value));
    let format_id = fields.next().and_then(|value| parse_stream_key(value));
    let video_codec = fields.next().and_then(|value| parse_stream_key(value));
    let audio_codec = fields.next().and_then(|value| parse_stream_key(value));
    let downloaded_bytes = parse_bytes(fields.next()?)?;
    let exact_total = fields.next()?;
    let estimated_total = fields.next().unwrap_or_default();
    let total_bytes = parse_bytes(exact_total).or_else(|| parse_bytes(estimated_total));
    let requested_downloads = fields.next().unwrap_or_default();
    let requested_formats = fields.next().unwrap_or_default();
    let planned_streams = {
        let requested_downloads = parse_planned_streams(requested_downloads);
        if requested_downloads.is_empty() { parse_planned_streams(requested_formats) } else { requested_downloads }
    };
    let expected_streams = planned_streams.len().max(if is_missing_codec(video_codec.as_deref()) || is_missing_codec(audio_codec.as_deref()) { 2 } else { 1 });
    let stream_key = match (video_id, format_id) {
        (Some(video_id), Some(format_id)) => Some(format!("{video_id}:{format_id}")),
        (Some(video_id), None) => Some(video_id),
        (None, Some(format_id)) => Some(format_id),
        (None, None) => None,
    };
    let format_id = stream_key.as_deref().and_then(|key| key.rsplit(':').next()).map(str::to_owned);
    Some(DownloadProgressSample { stream_key, format_id, downloaded_bytes, total_bytes, planned_streams, expected_streams })
}

fn parse_stream_key(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || matches!(value, "NA" | "None" | "none" | "null" | "N/A" | "-") {
        None
    } else {
        Some(value.to_string())
    }
}

fn is_missing_codec(codec: Option<&str>) -> bool {
    codec.is_none() || codec.is_some_and(|value| value.eq_ignore_ascii_case("none"))
}

impl ProgressAccumulator {
    fn record(&mut self, sample: DownloadProgressSample) -> Option<u8> {
        for planned in &sample.planned_streams {
            self.planned_sizes.entry(planned.format_id.clone()).and_modify(|known| {
                if known.is_none() || planned.total_bytes.is_some_and(|size| known.is_some_and(|previous| size > previous)) {
                    *known = planned.total_bytes;
                }
            }).or_insert(planned.total_bytes);
        }
        self.expected_streams = self.expected_streams.max(sample.expected_streams);
        let base_key = sample.stream_key.unwrap_or_else(|| "anonymous".into());
        let mut key = base_key.clone();
        let mut replace_stream = false;
        if let Some(previous) = self.streams.get(&key) {
            if sample.downloaded_bytes < previous.downloaded_bytes {
                // A reset with the same key means another transfer item (or a
                // retry after a changed denominator). Keep the first item in
                // the aggregate instead of letting the displayed ratio jump
                // backwards or count a reset as negative bytes.
                if sample.total_bytes != previous.total_bytes {
                    self.anonymous_stream = self.anonymous_stream.saturating_add(1);
                    key = format!("{base_key}#{}", self.anonymous_stream);
                    replace_stream = true;
                } else {
                    return self.reported_percent();
                }
            }
        }
        let stream = self.streams.entry(key).or_default();
        if replace_stream {
            stream.downloaded_bytes = 0;
            stream.total_bytes = None;
        }
        stream.format_id = sample.format_id;
        stream.downloaded_bytes = stream.downloaded_bytes.max(sample.downloaded_bytes);
        stream.total_bytes = match (stream.total_bytes, sample.total_bytes) {
            (Some(previous), Some(current)) => Some(previous.max(current)),
            (None, current) => current,
            (current, None) => current,
        };
        self.reported_percent()
    }

    fn reported_percent(&mut self) -> Option<u8> {
        // requested_formats is available before the first downloader byte and
        // carries both selected format sizes. Use it as the denominator so a
        // large video transfer remains visible before the audio item starts.
        let planned_total = if self.planned_sizes.is_empty() {
            None
        } else {
            self.planned_sizes.values().try_fold(0u64, |sum, total| Some(sum.checked_add((*total)?)?))
        };
        let total_bytes = match planned_total {
            Some(total) => total,
            None => {
                // If the selected plan has no usable sizes, fall back to live
                // totals, but only after every expected item has appeared.
                if self.streams.len() < self.expected_streams { return None; }
                self.streams.values().try_fold(0u64, |sum, stream| Some(sum.checked_add(stream.total_bytes?)?))?
            }
        };
        if total_bytes == 0 { return None; }
        let downloaded_bytes = self.streams.values().fold(0u64, |sum, stream| sum.saturating_add(stream.downloaded_bytes));
        if downloaded_bytes == 0 { return None; }
        let ratio = ((downloaded_bytes as u128 * 100) / total_bytes as u128).min(100) as u8;
        // A stream can report 100% before yt-dlp starts the second selected
        // item. Keep 100 reserved for successful publication; the state pump
        // never shows a false completed transfer while the child is running.
        let percent = ratio.min(99).max(self.last_percent);
        self.last_percent = percent;
        Some(percent)
    }
}

/// High-quality ladder for BACKGROUND work (next-track prefetch and the
/// current-track quality UPGRADE pass): same H.264-first codec preferences
/// capped at 720p. The former ≤1080p ceiling produced single staged files of
/// 118–197 MB; at 720p they land roughly 3–5× smaller while WKWebView still
/// hardware-decodes them, so one song switch can no longer saturate disk,
/// network, and CPU for minutes. ensure_playable_audio transcodes non-AAC
/// sound afterwards, so audio stays compatible.
pub const FORMAT_LADDER_HIGH: &str = "bv*[vcodec^=avc1][height<=720]+ba[acodec^=mp4a]/bv*[vcodec^=avc1][height<=720]+ba/bv*[vcodec^=avc1][height<=720]/b[ext=mp4][vcodec^=avc1][height<=720]";

/// Quick-tier ladder for INTERACTIVE current-track staging: same codec
/// preferences capped at 480p, so the file is a fraction of the size and the
/// picture appears seconds after the search instead of after a full-quality
/// pull. A background pass then replaces it with the ≤720p version
/// (`upgrade_staged_video`). WKWebView plays 480p H.264/AAC natively.
/// Quick-tier ladder for INTERACTIVE current-track staging: same codec
/// preferences capped at **240p**. This is the first playable tier, optimized
/// for time to first frame; a background pass
/// then replaces it with the ≤720p version (`upgrade_staged_video`).
/// WKWebView plays 240p H.264/AAC natively.
pub const FORMAT_LADDER_QUICK: &str = "bv*[vcodec^=avc1][height<=240]+ba[acodec^=mp4a]/bv*[vcodec^=avc1][height<=240]+ba/bv*[vcodec^=avc1][height<=240]/b[ext=mp4][vcodec^=avc1][height<=240]";

/// Single download implementation behind both tiers. `ladder` selects the
/// yt-dlp `-f` chain; `progress` (when given) receives live byte-derived percentages.
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
    timeout: Duration,
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
    command.args(["--newline", "--no-color",
        // NO --extractor-args client overrides here: forcing specific clients
        // (tv/web) changes which format ladder YouTube serves and can match
        // NOTHING. The stock rotation works; per-video refusals are handled
        // by falling through to the next search candidate.
        "-f", ladder,
        // Parallel DASH fragment pulls — cuts long-video download times hard
        // (single-stream YouTube throttling was the 50s+ wait on a 15-min film).
        "--concurrent-fragments", "8",
        // Stable machine-readable byte progress on stdout. Formatted
        // percentages are presentation strings and cannot be aggregated when
        // yt-dlp downloads separate video and audio items.
        "--progress-template", "download-bytes:%(info.id)s\t%(format_id)s\t%(info.vcodec)s\t%(info.acodec)s\t%(progress.downloaded_bytes)s\t%(progress.total_bytes)s\t%(progress.total_bytes_estimate)s\t%(info.requested_downloads)j\t%(info.requested_formats)j",
        "--merge-output-format", "mp4",
        "-o",
    ]);
    command.arg(destination);
    if let Some(dir) = ffmpeg_dir { command.arg("--ffmpeg-location").arg(dir); }
    command.arg(url);
    let mut child = command.spawn().map_err(|error| format!("video downloader could not start: {error}"))?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let reporter = progress.cloned();
    // STDERR is ALWAYS drained by a reader thread — an unread pipe fills its
    // fixed OS buffer, the child blocks on write and never exits.
    let mut stderr_reader = Some(std::thread::spawn(move || {
        let mut collected = String::new();
        if let Some(pipe) = stderr.as_mut() {
            use std::io::BufRead;
            for line in std::io::BufReader::new(pipe).lines().map_while(Result::ok) {
                collected.push_str(&line);
                collected.push('\n');
                if let Some(reporter) = &reporter {
                    if let Some(sample) = parse_download_progress(&line) {
                        reporter.record(sample);
                    }
                }
            }
        }
        collected
    }));
    // Progress lines flow on STDOUT; ERROR lines on stderr. Both readers are
    // joined before a successful return so the final byte sample cannot race
    // the staging worker's publication.
    let mut progress_reader = if report_progress {
        let progress_reader = progress.cloned().expect("progress is present when reporting is enabled");
        Some(std::thread::spawn(move || {
            if let Some(stdout) = stdout.as_mut() {
                use std::io::BufRead;
                for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Some(sample) = parse_download_progress(&line) {
                        progress_reader.record(sample);
                    }
                }
            }
        }))
    } else {
        None
    };
    // Overall wall-clock budget: a wedged download must fail the candidate
    // (and fall through to the next one) instead of staging forever.
    let started = std::time::Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = progress_reader.take().and_then(|reader| reader.join().ok());
            let _ = stderr_reader.take().and_then(|reader| reader.join().ok());
            let _ = fs::remove_file(destination);
            return Err("cancelled".into());
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = progress_reader.take().and_then(|reader| reader.join().ok());
            let _ = stderr_reader.take().and_then(|reader| reader.join().ok());
            let _ = fs::remove_file(destination);
            return Err(format!("video download timed out after {}s", timeout.as_secs()));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = progress_reader.take().and_then(|reader| reader.join().ok());
                let reason = stderr_reader.take().and_then(|reader| reader.join().ok()).unwrap_or_default();
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
    let sources = find_sources_cancellable(helper, artist, title, Some(cancel));
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
        // Successors use the quick tier. When one becomes current the index
        // fast path promotes it immediately, publishes it, then runs the high
        // tier replacement in the background.
        if let Err(message) = download_ladder(
            helper,
            ffmpeg_dir,
            &source.url,
            &destination,
            cancel,
            FORMAT_LADDER_QUICK,
            None,
            Duration::from_secs(120),
        ) {
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
        if let Some(key) = &track_key {
            stager.record_index_tier(key, &source.id, crate::video::staging::INDEX_TIER_QUICK);
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
    if let Some(reporter) = progress {
        // One progress accumulator spans the candidate ladder. Resetting per
        // candidate would make the visible ratio jump backwards after a
        // refused source; only a new user staging request starts at zero.
        reporter.reset();
    }
    // Fast path: an earlier attempt already mapped this track to a YouTube id
    // and downloaded it. Skip the (multi-second) search entirely.
    if let Some(track_key) = track_index_key(artist, title) {
        if let Some((id, cached_path, tier)) = stager.index_lookup_with_tier(&track_key, Slot::Current) {
            eprintln!("[luno-video] index cache hit {id} for {track_key:?} (tier {tier})");
            let slot_generation = stager.begin_generation();
            return match stager.adopt(Slot::Current, slot_generation, &cached_path) {
                Ok(staged) => {
                    if let Some(reporter) = progress {
                        reporter.complete();
                    }
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
    let sources = find_sources_cancellable(helper, artist, title, Some(cancel));
    eprintln!("[luno-video] search for \"{artist:?} - {title}\" -> {} candidates", sources.len());
    if sources.is_empty() {
        return Ok(VideoOutcome::Unavailable("no relevant video found".into()));
    }
    // Try candidates in relevance order: YouTube refuses SOME licensed videos
    // to every downloader client ("not available on this app"), so the first
    // hit is not always downloadable — fall through to alternates.
    let track_key = track_index_key(artist, title);
    let mut last_error = String::from("no candidate could be downloaded");
    // Interactive staging has a hard TOTAL budget. Previously every one of
    // three candidates could consume a five-minute timeout, so a bad result
    // list left the spinner running for up to fifteen minutes.
    const INTERACTIVE_TOTAL_BUDGET: Duration = Duration::from_secs(120);
    const INTERACTIVE_CANDIDATE_BUDGET: Duration = Duration::from_secs(75);
    let interactive_started = std::time::Instant::now();
    for source in sources.into_iter().take(3) {
        if cancel.load(Ordering::Acquire) { return Ok(VideoOutcome::Unavailable("cancelled".into())); }
        let remaining = INTERACTIVE_TOTAL_BUDGET.saturating_sub(interactive_started.elapsed());
        if remaining.is_zero() {
            last_error = "video staging timed out after 120s".into();
            break;
        }
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
                    download_ladder(helper, ffmpeg_dir, &source.url, &destination, cancel, FORMAT_LADDER_QUICK, Some(reporter), remaining.min(INTERACTIVE_CANDIDATE_BUDGET))
                }
                None => download_ladder(helper, ffmpeg_dir, &source.url, &destination, cancel, FORMAT_LADDER_QUICK, None, remaining.min(INTERACTIVE_CANDIDATE_BUDGET)),
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
                if let Some(reporter) = progress {
                    reporter.complete();
                }
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
    let result = download_ladder(helper, ffmpeg_dir, &crate::video::source::canonical_url(video_id_from_destination(file_name)), &temporary, cancel, FORMAT_LADDER_HIGH, None, Duration::from_secs(300))
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

    #[cfg(unix)]
    fn fake_video_helper(root: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let helper = root.join("fake-yt-dlp");
        fs::write(&helper, r#"#!/bin/sh
case " $* " in
  *" --dump-json "*)
    printf '%s\n' \
      '{"id":"video-a","title":"Artist - Title","uploader":"Artist"}' \
      '{"id":"video-b","title":"Artist - Title","uploader":"Artist"}' \
      '{"id":"video-c","title":"Artist - Title","uploader":"Artist"}'
    ;;
  *)
    output=''
    while [ "$#" -gt 0 ]; do
      if [ "$1" = '-o' ]; then shift; output="$1"; fi
      shift
    done
    printf 'download-bytes:video-a\t137\tavc1\tnone\t0\t1000\t1000\tNA\t[{"format_id":"137","filesize":1000},{"format_id":"140","filesize":200}]\n'
    printf 'download-bytes:video-a\t137\tavc1\tnone\t250\t1000\t1000\tNA\t[{"format_id":"137","filesize":1000},{"format_id":"140","filesize":200}]\n'
    printf 'fake mp4 bytes' > "$output"
    ;;
esac
"#).unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&helper, permissions).unwrap();
        helper
    }

    #[test]
    fn parses_machine_readable_byte_progress_and_estimated_totals() {
        assert_eq!(
            parse_download_progress("download-bytes:video-a\t137\tavc1\tnone\t250\t1000\t1000\tNA\t[{\"format_id\":\"137\",\"filesize\":1000},{\"format_id\":\"140\",\"filesize_approx\":200}]"),
            Some(DownloadProgressSample {
                stream_key: Some("video-a:137".into()),
                format_id: Some("137".into()),
                downloaded_bytes: 250,
                total_bytes: Some(1000),
                planned_streams: vec![
                    PlannedStream { format_id: "137".into(), total_bytes: Some(1000) },
                    PlannedStream { format_id: "140".into(), total_bytes: Some(200) },
                ],
                expected_streams: 2,
            })
        );
        assert_eq!(
            parse_download_progress("download-bytes:video-a\t140\tnone\tmp4a.40.2\t250\tNA\t500\tNA\t[{\"format_id\":\"137\",\"filesize\":1000},{\"format_id\":\"140\",\"filesize_approx\":200}]"),
            Some(DownloadProgressSample {
                stream_key: Some("video-a:140".into()),
                format_id: Some("140".into()),
                downloaded_bytes: 250,
                total_bytes: Some(500),
                planned_streams: vec![
                    PlannedStream { format_id: "137".into(), total_bytes: Some(1000) },
                    PlannedStream { format_id: "140".into(), total_bytes: Some(200) },
                ],
                expected_streams: 2,
            })
        );
        assert_eq!(parse_download_progress("download-bytes:video-a\t137\tavc1\tavc1\t0\tNA\tNA\tNA\t[{\"format_id\":\"137\",\"filesize\":1000}]"), Some(DownloadProgressSample {
            stream_key: Some("video-a:137".into()),
            format_id: Some("137".into()),
            downloaded_bytes: 0,
            total_bytes: None,
            planned_streams: vec![PlannedStream { format_id: "137".into(), total_bytes: Some(1000) }],
            expected_streams: 1,
        }));
        assert_eq!(parse_download_progress("[download] 42.3% of 1000 bytes"), None);
    }

    #[test]
    fn byte_progress_is_indeterminate_until_bytes_exist_and_monotonic_across_streams() {
        let mut progress = ProgressAccumulator::default();
        assert_eq!(progress.record(DownloadProgressSample { stream_key: Some("video:137".into()), downloaded_bytes: 0, total_bytes: Some(1000), expected_streams: 1, ..Default::default() }), None);
        assert_eq!(progress.record(DownloadProgressSample { stream_key: Some("video:137".into()), downloaded_bytes: 250, total_bytes: Some(1000), expected_streams: 1, ..Default::default() }), Some(25));
        assert_eq!(progress.record(DownloadProgressSample { stream_key: Some("video:137".into()), downloaded_bytes: 1000, total_bytes: Some(1000), expected_streams: 1, ..Default::default() }), Some(99));

        let live = VideoProgress::new();
        live.record(DownloadProgressSample { stream_key: Some("video:137".into()), downloaded_bytes: 0, total_bytes: Some(1000), expected_streams: 1, ..Default::default() });
        assert!(!live.started.load(Ordering::Acquire));
        live.record(DownloadProgressSample { stream_key: Some("video:137".into()), downloaded_bytes: 250, total_bytes: Some(1000), expected_streams: 1, ..Default::default() });
        assert!(live.started.load(Ordering::Acquire));
        assert_eq!(live.percent.load(Ordering::Acquire), 25);

        let mut streams = ProgressAccumulator::default();
        // A video-only item finishing cannot claim near-completion while the
        // separately selected audio item has not appeared yet.
        assert_eq!(streams.record(DownloadProgressSample { stream_key: Some("video:137".into()), downloaded_bytes: 1000, total_bytes: Some(1000), expected_streams: 2, ..Default::default() }), None);
        assert_eq!(streams.record(DownloadProgressSample { stream_key: Some("video:140".into()), downloaded_bytes: 0, total_bytes: Some(200), expected_streams: 2, ..Default::default() }), Some(83));
        assert_eq!(streams.record(DownloadProgressSample { stream_key: Some("video:140".into()), downloaded_bytes: 200, total_bytes: Some(200), expected_streams: 2, ..Default::default() }), Some(99));
    }

    #[test]
    fn planned_combined_progress_advances_during_video_before_audio_starts() {
        let plan = vec![
            PlannedStream { format_id: "137".into(), total_bytes: Some(1000) },
            PlannedStream { format_id: "140".into(), total_bytes: Some(200) },
        ];
        let mut progress = ProgressAccumulator::default();
        let sample = |format_id: &str, downloaded_bytes: u64| DownloadProgressSample {
            stream_key: Some(format!("video:{format_id}")),
            format_id: Some(format_id.into()),
            downloaded_bytes,
            total_bytes: Some(if format_id == "137" { 1000 } else { 200 }),
            planned_streams: plan.clone(),
            expected_streams: 2,
        };
        assert_eq!(progress.record(sample("137", 250)), Some(20));
        assert_eq!(progress.record(sample("137", 500)), Some(41));
        assert_eq!(progress.record(sample("137", 1000)), Some(83));
        // The audio item starts at zero and then completes; whole-artifact
        // progress advances without ever claiming 99% for video alone.
        assert_eq!(progress.record(sample("140", 0)), Some(83));
        assert_eq!(progress.record(sample("140", 200)), Some(99));
    }

    #[test]
    fn unknown_total_stays_indeterminate() {
        let mut progress = ProgressAccumulator::default();
        assert_eq!(progress.record(DownloadProgressSample { stream_key: Some("video:137".into()), downloaded_bytes: 250, total_bytes: None, expected_streams: 1, ..Default::default() }), None);
        assert_eq!(progress.last_percent, 0);
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
    /// serving the quick tier as final forever.
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
        assert!(FORMAT_LADDER_QUICK.contains("height<=240"));
        assert!(!FORMAT_LADDER_QUICK.contains("height<=360"));
        assert!(!FORMAT_LADDER_QUICK.contains("height<=480"));
        assert!(FORMAT_LADDER_HIGH.contains("height<=720"));
        assert!(!FORMAT_LADDER_HIGH.contains("height<=1080"));
        assert!(!FORMAT_LADDER_QUICK.contains("vp09"));
        assert!(!FORMAT_LADDER_HIGH.contains("vp09"));
    }

    #[cfg(unix)]
    #[test]
    fn successor_prefetch_records_quick_tier_for_instant_promotion() {
        let root = std::env::temp_dir().join(format!("luno-prefetch-quick-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let helper = fake_video_helper(&root);
        let stager = VideoStage::at(root.join("store"), None).unwrap();
        assert!(prefetch_track(
            &helper,
            None,
            &stager,
            Some("Artist"),
            "Title",
            &Arc::new(AtomicBool::new(false)),
        ));
        let (_, path, tier) = stager.index_lookup_with_tier("artisttitle", Slot::Next).expect("prefetched index");
        assert!(path.is_file());
        assert_eq!(tier, crate::video::staging::INDEX_TIER_QUICK);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn current_download_does_not_fabricate_zero_before_byte_sample() {
        let root = std::env::temp_dir().join(format!("luno-current-progress-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let helper = fake_video_helper(&root);
        let mut stager = VideoStage::at(root.join("store"), None).unwrap();
        let progress = Arc::new(VideoProgress::new());
        let outcome = stage_track(
            &helper,
            None,
            &mut stager,
            Some("Artist"),
            "Title",
            &Arc::new(AtomicBool::new(false)),
            Some(&progress),
        ).unwrap();
        assert!(matches!(outcome, VideoOutcome::Staged { needs_upgrade: true, .. }));
        assert!(progress.started.load(Ordering::Acquire));
        // The complete marker is written only after the quick file is adopted
        // into the current slot, never when the helper merely exits.
        assert_eq!(progress.percent.load(Ordering::Acquire), 100);
        let _ = fs::remove_dir_all(&root);
    }
}
