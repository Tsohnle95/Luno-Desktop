//! Production preview preparation: fetch a bounded source through the
//! download helper, encode it to ADTS AAC, and publish it through the
//! preview store's generation-guarded, size-limited, atomic path.
//!
//! The bundled release ffmpeg is an LGPL-only build (no libmp3lame), so
//! previews are encoded as 44.1 kHz / stereo / 192 kbps AAC ADTS rather
//! than the legacy loudnorm MP3 profile.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::preview::assets::{AssetError, PreviewAssetStore};
use crate::process::child::{isolate_process_group, terminate_process_tree, DEFAULT_KILL_GRACE};
use crate::runtime::task_registry::CancellationToken;

pub const PREVIEW_SAMPLE_RATE: u32 = 44100;
pub const PREVIEW_CHANNELS: u32 = 2;
pub const PREVIEW_BITRATE: &str = "192k";

struct CancellationBridge {
    cancelled: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    watcher: Option<JoinHandle<()>>,
}

impl CancellationBridge {
    fn new(token: &CancellationToken) -> Result<Self, AssetError> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let token_watcher = token.clone();
        let cancelled_watcher = cancelled.clone();
        let done_watcher = done.clone();
        let watcher = thread::Builder::new()
            .name("luno-preview-cancellation".into())
            .spawn(move || {
                while !done_watcher.load(Ordering::Acquire) {
                    if token_watcher.is_cancelled() {
                        cancelled_watcher.store(true, Ordering::Release);
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            })
            .map_err(|error| AssetError::Failed(format!("preview cancellation watcher could not start: {error}")))?;
        Ok(Self { cancelled, done, watcher: Some(watcher) })
    }
}

impl Drop for CancellationBridge {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
    }
}

/// A worker's terminal report to the session driver.
#[derive(Debug)]
pub enum PreviewCompletion {
    Progress { key: String, percent: u8 },
    Prepared { key: String, path: PathBuf },
    Failed { key: String, error: String },
}

/// Prepares one recommendation preview.
///
/// Stage 1 (resolve, bounded by `RESOLVE_DEADLINE`): the download helper
/// fetches `query` into a system-temp file.  Stage 2 (download, bounded by
/// `DOWNLOAD_DEADLINE`): ffmpeg encodes the source to ADTS AAC, streamed
/// through the store's limited writer and atomically published under
/// `{safe_key}.aac`.  Both stages honor `token` and the store generation.
pub fn prepare_preview(
    store: &PreviewAssetStore,
    key: &str,
    generation: u64,
    query: &str,
    helper: &Path,
    ffmpeg: &Path,
    token: &CancellationToken,
) -> Result<PathBuf, AssetError> {
    prepare_preview_with_progress(store, key, generation, query, helper, ffmpeg, token, Arc::new(|_| {}))
}

pub fn prepare_preview_with_progress(
    store: &PreviewAssetStore, key: &str, generation: u64, query: &str,
    helper: &Path, ffmpeg: &Path, token: &CancellationToken,
    progress: Arc<dyn Fn(u8) + Send + Sync>,
) -> Result<PathBuf, AssetError> {
    // The token is the source of truth: a pre-cancelled token aborts before
    // any work is scheduled.  The bridge below only propagates cancellations
    // that arrive while a stage is already running.
    if token.is_cancelled() { return Err(AssetError::Cancelled); }
    let cancellation = CancellationBridge::new(token)?;
    let bridge = cancellation.cancelled.clone();

    let helper_owned = helper.to_path_buf();
    let ffmpeg_owned = ffmpeg.to_path_buf();
    let query_for_fetch = query.to_owned();
    let resolve_bridge = bridge.clone();
    let bridge_in_resolve = resolve_bridge.clone();
    let progress_for_resolve = progress.clone();
    let fetched = store.resolve(generation, &resolve_bridge, move || {
        let output = temp_output("m4a");
        fetch_source(&helper_owned, &query_for_fetch, &output, &bridge_in_resolve, progress_for_resolve)
            .map_err(|error| if bridge_in_resolve.load(Ordering::Acquire) { "cancelled".to_owned() } else { error })?;
        Ok::<PathBuf, String>(output)
    })?;
    let download_bridge = bridge.clone();
    let bridge_in_encode = download_bridge.clone();
    let fetched_for_encode = fetched.clone();
    let publish = store.download(key, generation, &download_bridge, move |writer| {
        encode_adts(&ffmpeg_owned, &fetched_for_encode, writer, &bridge_in_encode)
    });
    let _ = fs::remove_file(&fetched);
    publish
}

fn is_cancelled(cancel: &AtomicBool) -> bool { cancel.load(Ordering::Acquire) }

fn temp_output(extension: &str) -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    std::env::temp_dir().join(format!("luno-preview-{}-{}.{}", std::process::id(), stamp, extension))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MediaProgress {
    downloaded_bytes: u64,
    total_bytes: u64,
}

/// Parses the preview helper's byte protocol. yt-dlp emits progress lines only
/// for a transfer, but checking the status here also prevents a future helper
/// change from turning lookup or post-processing output into a number.
fn parse_media_progress(line: &str) -> Option<MediaProgress> {
    let remainder = line.split_once("LUNO_BYTES=")?.1;
    let (downloaded_value, remainder) = remainder.split_once(';')?;
    let fields = remainder.split(';').collect::<Vec<_>>();
    let field = |name: &str| fields.iter().find_map(|field| field.strip_prefix(name).map(str::trim));
    if field("LUNO_STATUS=")? != "downloading" {
        return None;
    }
    let downloaded_bytes = downloaded_value.trim().parse::<u64>().ok()?;
    if downloaded_bytes == 0 {
        return None;
    }
    let total_bytes = field("LUNO_TOTAL=")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .or_else(|| field("LUNO_ESTIMATE=").and_then(|value| value.parse::<u64>().ok()).filter(|value| *value > 0))?;
    Some(MediaProgress { downloaded_bytes, total_bytes })
}

/// Converts known media bytes into a UI percentage. A transfer can reach its
/// byte total before yt-dlp has finished post-processing and the preview has
/// been published, so 100 is deliberately reserved for `prepared_asset`.
fn media_percent(progress: MediaProgress) -> Option<u8> {
    let ratio = (u128::from(progress.downloaded_bytes) * 100) / u128::from(progress.total_bytes);
    Some(ratio.min(99) as u8).filter(|percent| *percent > 0)
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;

    #[test]
    fn cancellation_watcher_is_joined_when_bridge_drops() {
        let token = CancellationToken::new();
        let bridge = CancellationBridge::new(&token).unwrap();
        let done = bridge.done.clone();
        drop(bridge);
        assert!(done.load(Ordering::Acquire));
    }

    #[test]
    fn cancellation_watcher_propagates_task_cancellation() {
        let token = CancellationToken::new();
        let bridge = CancellationBridge::new(&token).unwrap();
        token.cancel();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !bridge.cancelled.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(bridge.cancelled.load(Ordering::Acquire));
    }
}

/// Runs the download helper in an isolated yt-dlp home and retries once with
/// the provider's HLS audio rendition after a direct-audio 403 or a successful
/// helper exit that produced no usable file.
fn fetch_source(helper: &Path, query: &str, output: &Path, cancel: &AtomicBool, progress: Arc<dyn Fn(u8) + Send + Sync>) -> Result<(), String> {
    fetch_source_with_search(helper, query, output, cancel, progress, crate::download::search::search_youtube_web)
}

fn fetch_source_with_search(
    helper: &Path,
    query: &str,
    output: &Path,
    cancel: &AtomicBool,
    progress: Arc<dyn Fn(u8) + Send + Sync>,
    search: impl FnOnce(&str, usize) -> Result<Vec<crate::contracts::types::SearchResult>, String>,
) -> Result<(), String> {
    if is_cancelled(cancel) { return Err("cancelled".into()); }
    let worker_home = temp_output("home");
    fs::create_dir_all(&worker_home).map_err(|error| format!("preview helper home could not be created: {error}"))?;
    let mut last_percent = 0;
    let first = fetch_source_with_format_fallback(helper, query, output, &worker_home, cancel, &progress, &mut last_percent);
    let result = match first {
        Err(error) if !is_cancelled(cancel) && is_age_restricted(&error) && query.starts_with("ytsearch1:") => {
            let search_query = query.trim_start_matches("ytsearch1:");
            match search(search_query, 6) {
                Ok(candidates) => {
                    let mut last_error = error.clone();
                    let mut attempted = std::collections::HashSet::new();
                    let mut recovered = None;
                    for candidate in candidates.into_iter().take(6) {
                        if is_cancelled(cancel) { break; }
                        let Some(url) = candidate.url else { continue };
                        let Some(id) = crate::download::search::youtube_video_id(&url) else { continue };
                        if error.contains(&id) || !attempted.insert(id.clone()) { continue; }
                        let source = crate::video::source::canonical_url(&id);
                        match fetch_source_with_format_fallback(helper, &source, output, &worker_home, cancel, &progress, &mut last_percent) {
                            Ok(()) => { recovered = Some(()); break; }
                            Err(next_error) => last_error = next_error,
                        }
                    }
                    if is_cancelled(cancel) { Err("cancelled".into()) }
                    else { recovered.ok_or_else(|| format!("age-restricted source; alternatives unavailable: {last_error}")) }
                }
                Err(search_error) => Err(format!("{error}; alternative source search failed: {search_error}")),
            }
        }
        result => result,
    };
    let _ = fs::remove_dir_all(&worker_home);
    result
}

fn fetch_source_with_format_fallback(
    helper: &Path,
    query: &str,
    output: &Path,
    worker_home: &Path,
    cancel: &AtomicBool,
    progress: &Arc<dyn Fn(u8) + Send + Sync>,
    last_percent: &mut u8,
) -> Result<(), String> {
    let first = fetch_source_attempt(helper, query, output, worker_home, cancel, progress, last_percent, None);
    match first {
        Err(error) if !is_cancelled(cancel) && should_retry_preview_source(&error) => {
            let _ = fs::remove_file(output);
            fetch_source_attempt(
                helper,
                query,
                output,
                worker_home,
                cancel,
                progress,
                last_percent,
                Some("bestaudio[protocol^=m3u8]/bestaudio/best"),
            ).map_err(|fallback| format!("direct audio failed ({error}); HLS fallback failed ({fallback})"))
        }
        result => result,
    }
}

fn is_age_restricted(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("sign in to confirm your age") || error.contains("age-restricted") || error.contains("age restricted")
}

fn should_retry_preview_source(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("http error 403") || error.contains("no output file") || error.contains("empty output file")
}

fn fetch_source_attempt(
    helper: &Path,
    query: &str,
    output: &Path,
    worker_home: &Path,
    cancel: &AtomicBool,
    progress: &Arc<dyn Fn(u8) + Send + Sync>,
    last_percent: &mut u8,
    format: Option<&str>,
) -> Result<(), String> {
    if is_cancelled(cancel) { return Err("cancelled".into()); }
    let mut command = Command::new(helper);
    command
        .arg(output)
        .env("HOME", worker_home)
        .env("XDG_CONFIG_HOME", worker_home.join(".config"))
        .env("LUNO_DOWNLOAD_SOURCE", query)
        .env("LUNO_PREVIEW", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(format) = format { command.env("LUNO_DOWNLOAD_FORMAT", format); }
    else { command.env_remove("LUNO_DOWNLOAD_FORMAT"); }
    isolate_process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|_| "preview download helper could not start".to_string())?;
    let stdout = child.stdout.take().ok_or_else(|| "preview download helper output unreadable".to_string())?;
    let stderr = child.stderr.take().ok_or_else(|| "preview download helper error output unreadable".to_string())?;
    let (progress_tx, progress_rx) = std::sync::mpsc::channel::<(bool, String)>();
    let progress_tx_stderr = progress_tx.clone();
    let stdout_reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().flatten() {
            let _ = progress_tx.send((false, line));
        }
    });
    let stderr_reader = thread::spawn(move || {
        for line in BufReader::new(stderr).lines().flatten() {
            let _ = progress_tx_stderr.send((true, line));
        }
    });
    let mut errors = Vec::new();
    let mut handle_line = |is_error: bool, line: String| {
        if let Some(media_progress) = parse_media_progress(&line) {
            if let Some(percent) = media_percent(media_progress) {
                // yt-dlp can print a fresh transfer stage after a stream has
                // completed. Keep visible progress monotonic and never let a
                // transfer line claim publication complete.
                let percent = percent.max(*last_percent);
                if percent > *last_percent {
                    *last_percent = percent;
                    progress(percent);
                }
            }
        } else if is_error && !line.trim().is_empty() {
            if errors.len() >= 6 { errors.remove(0); }
            errors.push(line.trim().chars().take(240).collect::<String>());
        }
    };
    loop {
        if is_cancelled(cancel) {
            terminate_process_tree(&mut child, DEFAULT_KILL_GRACE);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err("cancelled".into());
        }
        while let Ok((is_error, line)) = progress_rx.try_recv() {
            handle_line(is_error, line);
        }
        match child.try_wait() {
            Err(error) => {
                terminate_process_tree(&mut child, DEFAULT_KILL_GRACE);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                let _ = fs::remove_file(output);
                return Err(format!("preview download helper wait failed: {error}"));
            }
            Ok(Some(status)) => {
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                while let Ok((is_error, line)) = progress_rx.try_recv() {
                    handle_line(is_error, line);
                }
                if !status.success() {
                    let _ = fs::remove_file(output);
                    let detail = if errors.is_empty() { "no diagnostic output".to_string() } else { errors.join(" | ") };
                    return Err(format!("preview helper exited with {status}: {detail}"));
                }
                if !output.is_file() {
                    return Err("preview download helper produced no output file".into());
                }
                if fs::metadata(output).map(|metadata| metadata.len() == 0).unwrap_or(true) {
                    let _ = fs::remove_file(output);
                    return Err("preview download helper produced an empty output file".into());
                }
                return Ok(());
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
        }
    }
}

/// Streams an ADTS AAC encode of `input` into `writer`, killing the encoder
/// on cancellation and failing loudly when bytes exceed the store limit.
fn encode_adts(ffmpeg: &Path, input: &Path, writer: &mut dyn Write, cancel: &AtomicBool) -> Result<(), String> {
    if is_cancelled(cancel) { return Err("cancelled".into()); }
    let mut child = Command::new(ffmpeg)
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-ar")
        .arg(PREVIEW_SAMPLE_RATE.to_string())
        .arg("-ac")
        .arg(PREVIEW_CHANNELS.to_string())
        .arg("-b:a")
        .arg(PREVIEW_BITRATE)
        .arg("-c:a")
        .arg("aac")
        .arg("-f")
        .arg("adts")
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "preview encoder could not start".to_string())?;
    let mut stdout = child.stdout.take().ok_or("preview encoder output unreadable".to_string())?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if is_cancelled(cancel) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cancelled".into());
        }
        let read = stdout.read(&mut buffer).map_err(|_| "preview encoder output read failed".to_string())?;
        if read == 0 { break; }
        writer.write_all(&buffer[..read]).map_err(|error| error.to_string())?;
    }
    let status = child.wait().map_err(|_| "preview encoder failed".to_string())?;
    if !status.success() { return Err("preview encoder failed".into()); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use crate::process::executables::{discover, ExecutableKind};
    use crate::preview::assets::PreviewAssetStore;

    use super::*;

    #[test]
    fn parses_machine_readable_media_bytes_and_estimates() {
        assert_eq!(
            parse_media_progress("download:LUNO_BYTES=250;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=downloading"),
            Some(MediaProgress { downloaded_bytes: 250, total_bytes: 1000 }),
        );
        assert_eq!(
            parse_media_progress("download:LUNO_BYTES=250;LUNO_TOTAL=NA;LUNO_ESTIMATE=1000;LUNO_STATUS=downloading"),
            Some(MediaProgress { downloaded_bytes: 250, total_bytes: 1000 }),
        );
        assert_eq!(parse_media_progress("download:LUNO_BYTES=250;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=finished"), None);
        assert_eq!(parse_media_progress("download:LUNO_BYTES=0;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=downloading"), None);
        assert_eq!(parse_media_progress("LUNO_PROGRESS= 42.0%"), None);
    }

    #[test]
    fn media_percent_never_claims_publication_complete() {
        assert_eq!(media_percent(MediaProgress { downloaded_bytes: 250, total_bytes: 1000 }), Some(25));
        assert_eq!(media_percent(MediaProgress { downloaded_bytes: 1000, total_bytes: 1000 }), Some(99));
        assert_eq!(media_percent(MediaProgress { downloaded_bytes: 2, total_bytes: 1000 }), None);
    }

    #[test]
    fn fast_helper_progress_is_drained_before_completion() {
        let root = std::env::temp_dir().join(format!("luno-preview-progress-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        fs::write(&helper, "#!/bin/sh\nprintf 'resolving source\\nLUNO_BYTES=0;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=downloading\\nLUNO_BYTES=250;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=downloading\\nLUNO_BYTES=750;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=downloading\\nLUNO_BYTES=1000;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=finished\\n'\nprintf 'ID3preview bytes' > \"$1\"\n").unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&helper, permissions).unwrap();
        let output = root.join("source.m4a");
        let cancel = AtomicBool::new(false);
        let values = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = values.clone();
        fetch_source(&helper, "fixture", &output, &cancel, Arc::new(move |percent| captured.lock().unwrap().push(percent))).unwrap();
        assert_eq!(*values.lock().unwrap(), vec![25, 75]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn helper_startup_and_unknown_totals_stay_indeterminate() {
        let root = std::env::temp_dir().join(format!("luno-preview-indeterminate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        fs::write(&helper, "#!/bin/sh\nprintf 'source lookup\\nLUNO_BYTES=100;LUNO_TOTAL=NA;LUNO_ESTIMATE=NA;LUNO_STATUS=downloading\\nLUNO_BYTES=0;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=downloading\\n'\nprintf 'ID3preview bytes' > \"$1\"\n").unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&helper, permissions).unwrap();
        let output = root.join("source.m4a");
        let cancel = AtomicBool::new(false);
        let values = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = values.clone();
        fetch_source(&helper, "fixture", &output, &cancel, Arc::new(move |percent| captured.lock().unwrap().push(percent))).unwrap();
        assert!(values.lock().unwrap().is_empty(), "lookup and unknown/zero-byte stages must remain indeterminate");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn transfer_progress_is_monotonic_across_stage_resets() {
        let root = std::env::temp_dir().join(format!("luno-preview-monotonic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        fs::write(&helper, "#!/bin/sh\nprintf 'LUNO_BYTES=200;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=downloading\\nLUNO_BYTES=900;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=downloading\\nLUNO_BYTES=100;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=downloading\\nLUNO_BYTES=1000;LUNO_TOTAL=1000;LUNO_ESTIMATE=NA;LUNO_STATUS=finished\\n'\nprintf 'ID3preview bytes' > \"$1\"\n").unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&helper, permissions).unwrap();
        let output = root.join("source.m4a");
        let cancel = AtomicBool::new(false);
        let values = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = values.clone();
        fetch_source(&helper, "fixture", &output, &cancel, Arc::new(move |percent| captured.lock().unwrap().push(percent))).unwrap();
        assert_eq!(*values.lock().unwrap(), vec![20, 90], "a later transfer stage must not reset or complete the visible progress");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn helper_failure_includes_bounded_diagnostic_output() {
        let root = std::env::temp_dir().join(format!("luno-preview-error-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        fs::write(&helper, "#!/bin/sh\necho 'provider said: requested format is unavailable' >&2\nexit 7\n").unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&helper, permissions).unwrap();
        let output = root.join("source.m4a");
        let cancel = AtomicBool::new(false);
        let error = fetch_source(&helper, "ytsearch1:fixture", &output, &cancel, Arc::new(|_| {})).expect_err("helper must fail");
        assert!(error.contains("exit status: 7"), "status missing: {error}");
        assert!(error.contains("requested format is unavailable"), "stderr missing: {error}");
        assert!(!output.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn age_gated_search_result_tries_the_next_public_candidate() {
        let root = std::env::temp_dir().join(format!("luno-preview-age-fallback-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        fs::write(&helper, "#!/bin/sh\nprintf '%s\\n' \"$LUNO_DOWNLOAD_SOURCE\" >> \"$1.sources\"\ncase \"$LUNO_DOWNLOAD_SOURCE\" in\n  *abc123def45*) printf 'ID3playable bytes' > \"$1\" ;;\n  *) echo 'ERROR: [youtube] Iyy3YOpxL2k: Sign in to confirm your age.' >&2; exit 1 ;;\nesac\n").unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&helper, permissions).unwrap();
        let output = root.join("source.m4a");
        let source = |id: &str| {
            let url = format!("https://www.youtube.com/watch?v={id}");
            crate::contracts::types::SearchResult {
                provider: "youtube".into(), query: url.clone(), title: "Fixture".into(),
                artist: None, duration: None, thumbnail_url: None, url: Some(url),
            }
        };
        fetch_source_with_search(&helper, "ytsearch1:artist song", &output, &AtomicBool::new(false), Arc::new(|_| {}),
            |query, limit| {
                assert_eq!(query, "artist song");
                assert_eq!(limit, 6);
                Ok(vec![source("Iyy3YOpxL2k"), source("abc123def45")])
            })
            .expect("the second candidate should prepare the preview");
        assert_eq!(fs::read(output).unwrap(), b"ID3playable bytes");
        assert_eq!(fs::read_to_string(root.join("source.m4a.sources")).unwrap(),
            "ytsearch1:artist song\nhttps://www.youtube.com/watch?v=abc123def45\n");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn no_output_success_retries_once_with_hls_audio() {
        let root = std::env::temp_dir().join(format!("luno-preview-no-output-retry-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        fs::write(
            &helper,
            "#!/bin/sh\nprintf '%s\\n' \"${LUNO_DOWNLOAD_FORMAT:-default}\" >> \"$1.attempts\"\nif [ \"${LUNO_DOWNLOAD_FORMAT:-}\" = 'bestaudio[protocol^=m3u8]/bestaudio/best' ]; then printf 'ID3fallback bytes' > \"$1\"; fi\nexit 0\n",
        ).unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&helper, permissions).unwrap();
        let output = root.join("source.m4a");
        let fetched = fetch_source(&helper, "ytsearch1:fixture", &output, &AtomicBool::new(false), Arc::new(|_| {}));
        fetched.expect("a no-output success must retry the HLS selector");
        assert_eq!(fs::read_to_string(root.join("source.m4a.attempts")).unwrap(), "default\nbestaudio[protocol^=m3u8]/bestaudio/best\n");
        assert_eq!(fs::read(output).unwrap(), b"ID3fallback bytes");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn direct_audio_403_retries_once_with_hls_audio() {
        let root = std::env::temp_dir().join(format!("luno-preview-403-retry-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        fs::write(
            &helper,
            "#!/bin/sh\nprintf '%s\\n' \"${LUNO_DOWNLOAD_FORMAT:-default}\" >> \"$1.attempts\"\nif [ \"${LUNO_DOWNLOAD_FORMAT:-}\" = 'bestaudio[protocol^=m3u8]/bestaudio/best' ]; then printf 'ID3fallback bytes' > \"$1\"; exit 0; fi\necho 'ERROR: HTTP Error 403: Forbidden' >&2\nexit 1\n",
        ).unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&helper, permissions).unwrap();
        let output = root.join("source.m4a");
        fetch_source(&helper, "ytsearch1:fixture", &output, &AtomicBool::new(false), Arc::new(|_| {}))
            .expect("a direct-audio 403 must retry the HLS selector");
        assert_eq!(fs::read_to_string(root.join("source.m4a.attempts")).unwrap(), "default\nbestaudio[protocol^=m3u8]/bestaudio/best\n");
        assert_eq!(fs::read(output).unwrap(), b"ID3fallback bytes");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn cancelling_preview_kills_helper_process_group_and_descendants() {
        let root = std::env::temp_dir().join(format!("luno-preview-process-tree-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        fs::write(&helper, "#!/bin/sh\nsleep 30 &\nchild=$!\nprintf '%s\\n' \"$$\" > \"$1.group\"\nwait \"$child\"\n").unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&helper, permissions).unwrap();
        let output = root.join("source.m4a");
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker_helper = helper.clone();
        let worker_output = output.clone();
        let worker = thread::spawn(move || fetch_source(
            &worker_helper,
            "ytsearch1:fixture",
            &worker_output,
            &worker_cancel,
            Arc::new(|_| {}),
        ));
        let group_file = root.join("source.m4a.group");
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !group_file.is_file() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let group = fs::read_to_string(&group_file).expect("helper should publish its process group").trim().parse::<i32>().unwrap();
        cancel.store(true, Ordering::Release);
        assert_eq!(worker.join().unwrap(), Err("cancelled".into()));
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while unsafe { libc::kill(-group, 0) == 0 } && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_ne!(unsafe { libc::kill(-group, 0) }, 0, "cancel must terminate the shell and every helper descendant");
        let _ = fs::remove_dir_all(root);
    }

    struct TempToolchain { root: PathBuf }
    impl Drop for TempToolchain { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); } }

    fn fixture_scripts(label: &str) -> (TempToolchain, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("luno-preview-toolchain-{}-{}", std::process::id(), label));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        // The ID3 prefix keeps the fake payload past media-signature validation.
        fs::write(&helper, "#!/bin/sh\nprintf 'ID3synthetic preview audio source bytes' > \"$1\"\n").unwrap();
        let ffmpeg = root.join("ffmpeg");
        // `-f adts -` streams to stdout (`-` is the final argument); save
        // remuxes to a file named by the final argument.
        fs::write(&ffmpeg, "#!/bin/sh\nfor last do :; done\nif [ \"$last\" = \"-\" ]; then cat \"$3\"; else cat \"$3\" > \"$last\"; fi\n").unwrap();
        for script in [&helper, &ffmpeg] {
            let mut permissions = fs::metadata(script).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(script, permissions).unwrap();
        }
        (TempToolchain { root }, helper, ffmpeg)
    }

    fn wav_bytes() -> Vec<u8> {
        // One second of deterministic full-scale 16-bit noise at 44.1 kHz
        // (silence encodes to almost nothing under AAC).
        let mut bytes = Vec::with_capacity(44 + 44100 * 2);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36u32 + 44100 * 2).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&44100u32.to_le_bytes());
        bytes.extend_from_slice(&(44100u32 * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(44100u32 * 2).to_le_bytes());
        let mut state = 0x1234_5678u32;
        for _ in 0..44100 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            bytes.extend_from_slice(&((state >> 16) as u16).to_le_bytes());
        }
        bytes
    }

    #[test]
    fn prepare_publishes_a_generation_guarded_aac_preview() {
        let (toolchain, helper, ffmpeg) = fixture_scripts("publish");
        let root = toolchain.root.join("store");
        let mut store = PreviewAssetStore::at(&root).unwrap();
        let generation = store.begin_generation();
        let token = CancellationToken::new();
        let result = prepare_preview(&store, "artist-track", generation, "artist track", &helper, &ffmpeg, &token);
        let path = result.expect("preview preparation must succeed with fixture toolchain");
        assert_eq!(path.file_name().unwrap().to_string_lossy(), "artist-track.aac");
        let bytes = fs::metadata(&path).unwrap().len();
        assert!(bytes > 0, "published preview must not be empty");
        let stale = prepare_preview(&store, "artist-track", generation + 1, "artist track", &helper, &ffmpeg, &token);
        assert!(matches!(stale, Err(AssetError::StaleGeneration)), "stale generations must not publish: {stale:?}");
    }

    #[test]
    fn prepared_previews_are_local_only_and_cleaned_with_the_store() {
        let (toolchain, helper, ffmpeg) = fixture_scripts("local");
        let root = toolchain.root.join("store");
        let mut store = PreviewAssetStore::at(&root).unwrap();
        let generation = store.begin_generation();
        let prepared = prepare_preview(&store, "local-only", generation, "local only", &helper, &ffmpeg, &CancellationToken::new()).unwrap();
        assert!(prepared.starts_with(&root), "previews must stay inside the private store");
        store.cleanup_session(None).unwrap();
        assert!(!prepared.exists(), "cleanup must remove published previews");
    }

    #[test]
    fn cancelled_tokens_abort_preparation_before_any_work() {
        let (toolchain, helper, ffmpeg) = fixture_scripts("cancel");
        let mut store = PreviewAssetStore::at(toolchain.root.join("store")).unwrap();
        let generation = store.begin_generation();
        let token = CancellationToken::new();
        token.cancel();
        let result = prepare_preview(&store, "artist-track", generation, "artist track", &helper, &ffmpeg, &token);
        assert!(matches!(result, Err(AssetError::Cancelled)), "pre-cancelled token must abort: {result:?}");
    }

    #[test]
    fn real_bundled_ffmpeg_encodes_fetched_wav_to_adts() {
        let managed = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../release-input/bin");
        let Ok(ffmpeg) = discover(ExecutableKind::Ffmpeg, Some(&managed)) else {
            // The release binary is absent outside packaged fixtures; the
            // fixture-toolchain tests above already cover the pipeline.
            return;
        };
        let (toolchain, helper, _) = fixture_scripts("real");
        let root = toolchain.root.join("store");
        let mut store = PreviewAssetStore::at(&root).unwrap();
        fs::write(root.join("source.wav"), wav_bytes()).unwrap();
        std::fs::create_dir_all(root.join("..")).ok();
        let source = root.join("source.wav");
        fs::write(&helper, format!("#!/bin/sh\ncp '{}' \"$1\"\n", source.display())).unwrap();
        let mut permissions = fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&helper, permissions).unwrap();
        let generation = store.begin_generation();
        let prepared = prepare_preview(&store, "encoded", generation, "encoded", &helper, &ffmpeg, &CancellationToken::new())
            .expect("real encoder must produce a preview");
        let bytes = fs::metadata(&prepared).unwrap().len();
        assert!(bytes > 1024, "ADTS encode must be substantially populated, got {bytes} bytes");
        assert_ne!(bytes, fs::metadata(&source).unwrap().len(), "encode must not pass the source through verbatim");
    }

    #[test]
    #[ignore = "requires live YouTube network access"]
    fn live_bundled_toolchain_prepares_a_recommendation_preview() {
        let managed = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/bin");
        let helper = managed.join("luno-download-helper");
        let ffmpeg = managed.join("ffmpeg");
        assert!(helper.is_file() && ffmpeg.is_file());
        let result = crate::download::search::search_youtube_web("spaceboy smashing pumpkins", 1).expect("resolve preview source").into_iter().next().expect("preview result");
        let toolchain = TempToolchain { root: std::env::temp_dir().join(format!("luno-live-preview-{}", std::process::id())) };
        fs::create_dir_all(&toolchain.root).unwrap();
        let store = PreviewAssetStore::at(toolchain.root.join("store")).unwrap();
        let generation = store.current_generation();
        let path = prepare_preview(&store, "spaceboy-preview", generation, result.url.as_deref().unwrap(), &helper, &ffmpeg, &CancellationToken::new()).expect("prepare live preview");
        assert!(fs::metadata(path).unwrap().len() > 100_000);
    }
}
