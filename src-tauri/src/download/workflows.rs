//! Acquisition workflow policy, independent of yt-dlp and the UI.

use crate::download::csv::CsvTrack;
use crate::process::child::{isolate_process_group, terminate_process_tree, DEFAULT_KILL_GRACE};
use crate::runtime::task_registry::CancellationToken;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const WORKER_HOME_DIR: &str = ".luno-download-worker-home";
pub const LEGACY_WORKER_HOME_DIR: &str = ".luno-gate4-worker-home";
pub const DOWNLOAD_STAGING_DIR: &str = ".luno-download-staging";

pub fn is_intermediate_download(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase().contains(".part."))
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowError { InvalidUrl, EmptyUrl, EmptySearch, MissingDestination, Cancelled }

pub fn validate_http_url(value: &str) -> Result<&str, WorkflowError> {
    let value = value.trim();
    if value.is_empty() { return Err(WorkflowError::EmptyUrl); }
    let lower = value.as_bytes();
    let scheme = lower.iter().position(|b| *b == b':')
        .map(|at| &value[..at]).unwrap_or("");
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") { return Err(WorkflowError::InvalidUrl); }
    let rest = value[scheme.len() + 1..].strip_prefix("//").ok_or(WorkflowError::InvalidUrl)?;
    if rest.split('/').next().unwrap_or("").trim().is_empty() { return Err(WorkflowError::InvalidUrl); }
    Ok(value)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination { Explicit(PathBuf), Unsorted(PathBuf) }

impl Destination {
    pub fn resolve(&self) -> Result<PathBuf, WorkflowError> {
        match self { Self::Explicit(path) if !path.as_os_str().is_empty() => Ok(path.clone()), Self::Unsorted(path) if !path.as_os_str().is_empty() => Ok(path.clone()), _ => Err(WorkflowError::MissingDestination) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectUrl { pub url: String, pub destination: Destination, pub title: Option<String>, pub artist: Option<String> }

impl DirectUrl { pub fn validate(&self) -> Result<PathBuf, WorkflowError> { validate_http_url(&self.url)?; self.destination.resolve() } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchProvider { SoundCloud, YouTube }

pub fn validate_search(query: &str) -> Result<&str, WorkflowError> {
    let query = query.trim();
    if query.is_empty() { Err(WorkflowError::EmptySearch) } else { Ok(query) }
}

/// Return provider queries in legacy order: SoundCloud first, YouTube fallback.
pub fn search_sources(query: &str, try_soundcloud: bool) -> Result<Vec<(SearchProvider, String)>, WorkflowError> {
    let query = validate_search(query)?;
    let mut sources = Vec::with_capacity(if try_soundcloud { 2 } else { 1 });
    if try_soundcloud { sources.push((SearchProvider::SoundCloud, format!("scsearch1:{query}"))); }
    sources.push((SearchProvider::YouTube, format!("ytsearch1:{query}")));
    Ok(sources)
}

/// CSV's legacy policy is SoundCloud first when enabled, then YouTube search.
pub fn source_for_track(track: &CsvTrack, try_soundcloud: bool, soundcloud_url: Option<&str>) -> Result<(SearchProvider, String), WorkflowError> {
    let query = format!("{} {}", track.artist, track.title).trim().to_string();
    if query.is_empty() { return Err(WorkflowError::EmptySearch); }
    if let Some(url) = track.url.as_deref() { validate_http_url(url)?; return Ok((SearchProvider::YouTube, url.to_string())); }
    if try_soundcloud {
        if let Some(url) = soundcloud_url { validate_http_url(url)?; return Ok((SearchProvider::SoundCloud, url.to_string())); }
    }
    Ok((SearchProvider::YouTube, format!("ytsearch1:{query}")))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueEvent { Started { index: usize, total: usize, url: String }, Succeeded { index: usize }, Failed { index: usize, error: String }, Cancelled { completed: usize, total: usize }, Complete { succeeded: usize, failed: usize } }

/// Runs URL downloads sequentially. The callback must not detach work; returning
/// an error advances the queue, while cancellation produces one terminal event.
pub fn run_url_queue<F>(urls: &[String], cancelled: &AtomicBool, mut download: F) -> Vec<QueueEvent>
where F: FnMut(&str, &AtomicBool) -> Result<(), String> {
    let total = urls.len(); let mut events = Vec::new(); let mut succeeded = 0; let mut failed = 0;
    for (index, url) in urls.iter().enumerate() {
        if cancelled.load(Ordering::Acquire) { events.push(QueueEvent::Cancelled { completed: succeeded + failed, total }); return events; }
        if validate_http_url(url).is_err() { failed += 1; events.push(QueueEvent::Failed { index, error: "invalid HTTP/HTTPS URL".into() }); continue; }
        events.push(QueueEvent::Started { index, total, url: url.clone() });
        match download(url, cancelled) {
            Ok(()) => { succeeded += 1; events.push(QueueEvent::Succeeded { index }); }
            Err(_error) if cancelled.load(Ordering::Acquire) => { events.push(QueueEvent::Cancelled { completed: succeeded + failed, total }); return events; }
            Err(error) => { failed += 1; events.push(QueueEvent::Failed { index, error }); }
        }
    }
    events.push(QueueEvent::Complete { succeeded, failed }); events
}

pub fn destination_path(destination: Option<&Path>, unsorted: &Path) -> Result<PathBuf, WorkflowError> {
    match destination { Some(path) if !path.as_os_str().is_empty() => Ok(path.to_path_buf()), _ if !unsorted.as_os_str().is_empty() => Ok(unsorted.to_path_buf()), _ => Err(WorkflowError::MissingDestination) }
}

pub fn parse_download_progress(line: &str) -> Option<f32> {
    let value = line.trim().strip_prefix("LUNO_PROGRESS=")?.trim().trim_end_matches('%').trim();
    value.parse::<f32>().ok().filter(|value| value.is_finite()).map(|value| (value / 100.0).clamp(0.0, 1.0))
}

/// Run the bundled helper while continuously draining both pipes. yt-dlp's
/// machine-readable progress lines are mapped to 0–95%; the remaining slice
/// represents extraction, validation, and atomic promotion. The coordinator
/// publishes 100% only after the final audio file is safely in place.
pub fn run_download_helper(helper: &Path, temporary: &Path, source: &str, worker_home: &Path, token: &CancellationToken, progress: &dyn Fn(f32)) -> Result<(), String> {
    let first = run_download_helper_attempt(helper, temporary, source, worker_home, token, progress, None);
    let outcome = match first {
        Ok(()) => Ok(()),
        Err(error) => {
            if !token.is_cancelled() && error.to_ascii_lowercase().contains("http error 403") {
                run_download_helper_attempt(
                    helper,
                    temporary,
                    source,
                    worker_home,
                    token,
                    progress,
                    Some("bestaudio[protocol^=m3u8]/bestaudio/best"),
                ).map_err(|fallback| format!("direct audio was forbidden; HLS fallback failed: {fallback}"))
            } else {
                Err(error)
            }
        }
    };
    // A structurally valid file can still be content-corrupt: some source
    // uploads serve an audio rendition that is mostly digital silence with a
    // few seconds of music per interval (a "3 s on / 12 s off" mix). Such a
    // file decodes cleanly and passes the container signature check, then
    // plays as near-silent glitching. Reject it here, before promotion, so the
    // job fails honestly instead of publishing broken audio.
    outcome.and_then(|()| reject_corrupt_download(helper, temporary))
}

/// Approximate fraction of a file that is digital silence, used to detect a
/// source rendition that is structurally valid but content-broken. `None`
/// means the check could not run (no bundled ffmpeg, unreadable output); the
/// caller then keeps the download rather than blocking a successful transfer.
fn silent_fraction(ffmpeg: &Path, output: &Path) -> Option<f64> {
    // Decode to low-rate mono PCM and measure peak energy per 0.5 s window.
    // A window at or below `SILENCE_PEAK` (~-60 dBFS) counts as silent; real
    // audio (even quiet passages) is far above that floor.
    const ANALYSIS_RATE: u32 = 8_000;
    const WINDOW_SAMPLES: usize = ANALYSIS_RATE as usize / 2;
    const SILENCE_PEAK: u16 = 32;
    let mut child = Command::new(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"])
        .arg(output)
        .args(["-map", "0:a:0", "-ac", "1", "-ar"])
        .arg(ANALYSIS_RATE.to_string())
        .args(["-f", "s16le", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let mut chunk = vec![0u8; 64 * 1024];
    let mut pending: Vec<u8> = Vec::new();
    let mut windows = 0u64;
    let mut silent = 0u64;
    loop {
        match stdout.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => pending.extend_from_slice(&chunk[..read]),
            Err(_) => { let _ = child.wait(); return None; }
        }
        while pending.len() >= WINDOW_SAMPLES * 2 {
            let bytes = WINDOW_SAMPLES * 2;
            let peak = pending[..bytes]
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs())
                .max()
                .unwrap_or(0);
            windows += 1;
            if peak <= SILENCE_PEAK {
                silent += 1;
            }
            pending.drain(..bytes);
        }
    }
    let _ = child.wait();
    // Too short to judge (or nothing decoded): do not block the download.
    if windows < 120 {
        return None;
    }
    Some(silent as f64 / windows as f64)
}

fn reject_corrupt_download(helper: &Path, temporary: &Path) -> Result<(), String> {
    const CORRUPT_SILENCE_FRACTION: f64 = 0.5;
    let Some(ffmpeg) = helper.parent().map(|dir| dir.join("ffmpeg")).filter(|path| path.is_file()) else {
        return Ok(());
    };
    match silent_fraction(&ffmpeg, temporary) {
        Some(fraction) if fraction >= CORRUPT_SILENCE_FRACTION => Err(format!(
            "the downloaded audio is {:.0}% silent, so the source track is incomplete or corrupt and was not saved; try a different result",
            fraction * 100.0
        )),
        _ => Ok(()),
    }
}

fn run_download_helper_attempt(helper: &Path, temporary: &Path, source: &str, worker_home: &Path, token: &CancellationToken, progress: &dyn Fn(f32), format: Option<&str>) -> Result<(), String> {
    let mut command = Command::new(helper);
    command.arg(temporary).env("HOME", worker_home).stdout(Stdio::piped()).stderr(Stdio::piped());
    if !source.is_empty() { command.arg(source).env("LUNO_DOWNLOAD_SOURCE", source); }
    if let Some(format) = format { command.env("LUNO_DOWNLOAD_FORMAT", format); }
    isolate_process_group(&mut command);
    let mut child = command.spawn().map_err(|error| format!("download helper could not start: {error}"))?;
    let stdout = child.stdout.take().ok_or_else(|| "download helper output was unavailable".to_string())?;
    let stderr = child.stderr.take().ok_or_else(|| "download helper error output was unavailable".to_string())?;
    let (sender, receiver) = mpsc::channel::<(bool, String)>();
    let stdout_sender = sender.clone();
    let stdout_reader = std::thread::spawn(move || for line in BufReader::new(stdout).lines().map_while(Result::ok) { let _ = stdout_sender.send((false, line)); });
    let stderr_reader = std::thread::spawn(move || for line in BufReader::new(stderr).lines().map_while(Result::ok) { let _ = sender.send((true, line)); });
    let mut errors = Vec::new();
    let mut handle_line = |is_error: bool, line: String| {
        if let Some(fraction) = parse_download_progress(&line) { progress(fraction * 0.95); }
        else if is_error && !line.trim().is_empty() {
            if errors.len() >= 8 { errors.remove(0); }
            errors.push(line.trim().to_owned());
        }
    };
    let status = loop {
        if token.is_cancelled() {
            terminate_process_tree(&mut child, DEFAULT_KILL_GRACE);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err("cancelled".into());
        }
        while let Ok(line) = receiver.try_recv() { handle_line(line.0, line.1); }
        match child.try_wait().map_err(|error| format!("download helper wait failed: {error}"))? {
            Some(status) => break status,
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    };
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    while let Ok(line) = receiver.try_recv() { handle_line(line.0, line.1); }
    if status.success() { progress(0.97); Ok(()) }
    else if errors.is_empty() { Err(format!("download helper exited with {status}")) }
    else { Err(errors.join(" | ")) }
}

#[cfg(test)]
mod progress_tests {
    use super::parse_download_progress;

    #[test]
    fn parses_machine_readable_ytdlp_percentages() {
        assert_eq!(parse_download_progress("LUNO_PROGRESS= 42.5%"), Some(0.425));
        assert_eq!(parse_download_progress("[download] 42.5%"), None);
        assert_eq!(parse_download_progress("LUNO_PROGRESS=125%"), Some(1.0));
    }
}

#[cfg(all(test, unix))]
mod helper_quoting_tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// Regression: the bundled `luno-download-helper` expanded its
    /// `--ffmpeg-location` flag UNQUOTED, so any bundle path containing a
    /// space (notably `/Applications/Luno Desktop.app/…`) split the flag
    /// into a truncated directory plus a phantom source URL. yt-dlp then ran
    /// without ffmpeg: every preview/download postprocessing step failed
    /// with exit 1, surfacing as "preview download helper failed" and
    /// marking every recommendation unavailable (2026-09-09).
    ///
    /// This test runs the REAL shipped helper script — not a fixture copy —
    /// from a directory whose name contains a space, against a stub yt-dlp
    /// that records its argv. The old script passed a split flag and a
    /// phantom URL; the fixed script must pass exactly one intact
    /// `--ffmpeg-location <dir>` pair and the real query as the final arg.
    #[test]
    fn helper_passes_ffmpeg_location_intact_from_space_containing_paths() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let shipped = manifest_dir.join("resources").join("bin").join("luno-download-helper");
        assert!(shipped.is_file(), "shipped helper script must exist: {}", shipped.display());

        let sandbox = std::env::temp_dir().join(format!("luno space probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sandbox);
        std::fs::create_dir_all(&sandbox).unwrap();
        for name in ["ffmpeg", "yt-dlp", "luno-download-helper"] {
            let path = sandbox.join(name);
            if name == "luno-download-helper" {
                std::fs::copy(&shipped, &path).unwrap();
            } else if name == "yt-dlp" {
                std::fs::write(
                    &path,
                    "#!/bin/sh\nLOG=\"${YTDLP_ARG_LOG:?}\"\nOUT=\"\"\nPREV=\"\"\nfor a in \"$@\"; do\n  printf '%s\\n' \"$a\" >> \"$LOG\"\n  if [ \"$PREV\" = \"-o\" ]; then OUT=\"$a\"; fi\n  PREV=\"$a\"\n  if [ \"$PREV\" = \"--ffmpeg-location\" ]; then :; fi\ndone\nFILE=$(printf '%s' \"$OUT\" | sed 's/%(ext)s/m4a/')\nprintf 'ID3fake-bytes' > \"$FILE\"\nexit 0\n",
                ).unwrap();
            } else {
                std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
            }
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
        }
        let log = sandbox.join("argv.log");
        let target = sandbox.join("out.m4a");
        let status = std::process::Command::new(sandbox.join("luno-download-helper"))
            .arg(&target)
            .env("LUNO_DOWNLOAD_SOURCE", "ytsearch1:spaceboy probe")
            .env("YTDLP_ARG_LOG", &log)
            .status()
            .expect("helper must run");
        assert!(status.success(), "helper must succeed from a space-containing dir");
        assert!(target.is_file(), "helper must produce the target file");

        let argv: Vec<String> = std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        let flag_positions: Vec<usize> = argv.iter().enumerate()
            .filter(|(_, arg)| *arg == "--ffmpeg-location")
            .map(|(index, _)| index)
            .collect();
        assert_eq!(flag_positions.len(), 1, "exactly one --ffmpeg-location flag: {argv:?}");
        let dir = &argv[flag_positions[0] + 1];
        assert_eq!(dir, &sandbox.to_string_lossy(), "--ffmpeg-location value must be the intact dir: {argv:?}");
        assert_eq!(argv.last().map(String::as_str), Some("ytsearch1:spaceboy probe"), "final arg must be the real query: {argv:?}");
        assert!(!argv.iter().any(|arg| arg.contains("Desktop.app") && *arg != *dir), "no phantom split-path URL may reach yt-dlp: {argv:?}");
        let format = argv.windows(2)
            .find(|pair| pair[0] == "-f")
            .map(|pair| pair[1].as_str());
        assert_eq!(format, Some("bestaudio/best"), "normal downloads must retain the provider-neutral quality selector: {argv:?}");
        assert!(argv.iter().any(|arg| arg == "--ignore-config"), "ambient yt-dlp configuration must not override the helper's output contract: {argv:?}");
        let postprocessor_args = argv.windows(2)
            .find(|pair| pair[0] == "--ppa")
            .map(|pair| pair[1].as_str());
        assert!(postprocessor_args.is_some_and(|args| args.contains("-c:a aac")), "HLS AAC must be re-encoded so loudness normalization is not combined with an invalid stream-copy: {argv:?}");
        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn download_retries_http_403_with_hls_audio() {
        use super::run_download_helper;
        use crate::runtime::task_registry::CancellationToken;

        let sandbox = std::env::temp_dir().join(format!("luno 403 retry probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sandbox);
        std::fs::create_dir_all(&sandbox).unwrap();
        let helper = sandbox.join("helper");
        let attempts = sandbox.join("attempts");
        std::fs::write(
            &helper,
            "#!/bin/sh\nset -eu\nprintf '%s\\n' \"${LUNO_DOWNLOAD_FORMAT:-default}\" >> \"$HOME/attempts\"\nif [ \"${LUNO_DOWNLOAD_FORMAT:-}\" = 'bestaudio[protocol^=m3u8]/bestaudio/best' ]; then printf 'ID3fallback' > \"$1\"; exit 0; fi\necho 'ERROR: unable to download video data: HTTP Error 403: Forbidden' >&2\nexit 1\n",
        ).unwrap();
        let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&helper, permissions).unwrap();
        let output = sandbox.join("out.m4a");
        let result = run_download_helper(&helper, &output, "https://example.test/video", &sandbox, &CancellationToken::new(), &|_| {});
        assert_eq!(result, Ok(()));
        assert_eq!(std::fs::read_to_string(&attempts).unwrap(), "default\nbestaudio[protocol^=m3u8]/bestaudio/best\n");
        assert!(output.is_file());
        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn helper_preview_uses_machine_readable_byte_progress() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let shipped = manifest_dir.join("resources").join("bin").join("luno-download-helper");
        assert!(shipped.is_file(), "shipped helper script must exist: {}", shipped.display());

        let sandbox = std::env::temp_dir().join(format!("luno preview progress probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sandbox);
        std::fs::create_dir_all(&sandbox).unwrap();
        for name in ["ffmpeg", "yt-dlp", "luno-download-helper"] {
            let path = sandbox.join(name);
            if name == "luno-download-helper" {
                std::fs::copy(&shipped, &path).unwrap();
            } else if name == "yt-dlp" {
                std::fs::write(
                    &path,
                    "#!/bin/sh\nLOG=\"${YTDLP_ARG_LOG:?}\"\nOUT=\"\"\nPREV=\"\"\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> \"$LOG\"; if [ \"$PREV\" = \"-o\" ]; then OUT=\"$a\"; fi; PREV=\"$a\"; done\nFILE=$(printf '%s' \"$OUT\" | sed 's/%(ext)s/m4a/')\nprintf 'ID3fake-bytes' > \"$FILE\"\nexit 0\n",
                )
                .unwrap();
            } else {
                std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
            }
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
        }

        let log = sandbox.join("argv.log");
        let target = sandbox.join("out.m4a");
        let status = std::process::Command::new(sandbox.join("luno-download-helper"))
            .arg(&target)
            .env("LUNO_DOWNLOAD_SOURCE", "ytsearch1:preview probe")
            .env("LUNO_PREVIEW", "1")
            .env("YTDLP_ARG_LOG", &log)
            .status()
            .expect("helper must run");
        assert!(status.success(), "helper must succeed in preview mode");

        let argv: Vec<String> = std::fs::read_to_string(&log).unwrap().lines().map(str::to_owned).collect();
        let template = argv
            .windows(2)
            .find(|pair| pair[0] == "--progress-template")
            .map(|pair| pair[1].as_str())
            .expect("preview helper must pass a progress template");
        assert!(template.contains("LUNO_BYTES=%(progress.downloaded_bytes)s"), "byte counter missing: {template}");
        assert!(template.contains("LUNO_TOTAL=%(progress.total_bytes)s"), "exact total missing: {template}");
        assert!(template.contains("LUNO_ESTIMATE=%(progress.total_bytes_estimate)s"), "estimated total missing: {template}");
        assert!(template.contains("LUNO_STATUS=%(progress.status)s"), "status missing: {template}");
        assert!(!template.contains("LUNO_PROGRESS="), "preview must not use formatted percentage output: {template}");
        let _ = std::fs::remove_dir_all(&sandbox);
    }
}

#[cfg(all(test, unix))]
mod corrupt_download_tests {
    use super::reject_corrupt_download;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    const RATE: usize = 8_000;

    fn sandbox(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("luno-corrupt-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// A stand-in for the bundled ffmpeg that ignores its argv and streams a
    /// fixed raw PCM file, so the silence analysis is exercised without a real
    /// encoder.
    fn ffmpeg_stub(root: &Path, pcm: &Path) {
        let path = root.join("ffmpeg");
        std::fs::write(&path, format!("#!/bin/sh\ncat \"{}\"\n", pcm.display())).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
    }

    /// `(silent_seconds, loud_seconds)` of 8 kHz mono s16 PCM.
    fn pcm_file(label: &str, silent_seconds: usize, loud_seconds: usize) -> PathBuf {
        let path = std::env::temp_dir().join(format!("luno-pcm-{label}-{}.raw", std::process::id()));
        let mut bytes = Vec::with_capacity((silent_seconds + loud_seconds) * RATE * 2);
        for _ in 0..silent_seconds * RATE {
            bytes.extend_from_slice(&0i16.to_le_bytes());
        }
        for _ in 0..loud_seconds * RATE {
            bytes.extend_from_slice(&10_000i16.to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn guard(label: &str, silent_seconds: usize, loud_seconds: usize) -> Result<(), String> {
        let root = sandbox(label);
        let pcm = pcm_file(label, silent_seconds, loud_seconds);
        ffmpeg_stub(&root, &pcm);
        let helper = root.join("luno-download-helper");
        let result = reject_corrupt_download(&helper, &root.join("out.m4a"));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&pcm);
        result
    }

    /// Regression: a source rendition that is mostly digital silence (the
    /// "3 s music / 12 s silence" corruption) decodes cleanly and passes the
    /// container signature check, but the user hears near-silent glitching.
    /// The download must fail instead of being promoted.
    #[test]
    fn rejects_a_predominantly_silent_rendition() {
        let error = guard("silent", 61, 0).expect_err("mostly-silent audio must be rejected");
        assert!(error.contains("silent"), "error should explain the cause: {error}");
    }

    /// The real corruption is ~80 % silence; the threshold must catch it while
    /// leaving normal audio (with quiet passages) untouched.
    #[test]
    fn accepts_healthy_audio_and_normal_quiet_passages() {
        assert!(guard("loud", 0, 61).is_ok(), "continuous audio must be accepted");
        assert!(guard("mostly-loud", 10, 51).is_ok(), "a normal quiet passage must be accepted");
    }

    /// Short clips (previews, stingers) are not judged: a few silent seconds
    /// are not evidence of corruption.
    #[test]
    fn skips_files_too_short_to_judge() {
        assert!(guard("short", 30, 0).is_ok(), "a short silent clip must not be rejected");
    }

    /// Without a bundled ffmpeg the check fails open so an otherwise
    /// successful transfer is never blocked by a missing tool.
    #[test]
    fn fails_open_without_a_bundled_ffmpeg() {
        let root = sandbox("no-ffmpeg");
        let result = reject_corrupt_download(&root.join("luno-download-helper"), &root.join("out.m4a"));
        let _ = std::fs::remove_dir_all(&root);
        assert!(result.is_ok(), "a missing tool must not block the download");
    }
}
