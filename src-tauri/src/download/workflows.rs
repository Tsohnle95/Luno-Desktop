//! Acquisition workflow policy, independent of yt-dlp and the UI.

use crate::download::csv::CsvTrack;
use crate::runtime::task_registry::CancellationToken;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

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
    let mut command = Command::new(helper);
    command.arg(temporary).env("HOME", worker_home).stdout(Stdio::piped()).stderr(Stdio::piped());
    if !source.is_empty() { command.arg(source).env("LUNO_DOWNLOAD_SOURCE", source); }
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
            let _ = child.kill();
            let _ = child.wait();
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
