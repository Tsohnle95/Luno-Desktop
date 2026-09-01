//! Real yt-dlp search for the downloader hub. The provider-neutral
//! `workflows::search_sources` returns raw provider QUERIES (`ytsearch1:`);
//! this module executes those queries against the bundled yt-dlp binary and
//! parses the JSON-per-line output into rich results (title, artist,
//! duration, thumbnail) so the UI shows real songs instead of raw query
//! strings. Process invocations are isolated here so `state.rs` stays thin
//! and tests can inject fakes.

use crate::contracts::types::SearchResult;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
/// SoundCloud yields fewer relevant hits than YouTube; keep its cap modest.
const SC_LIMIT: usize = 5;

/// Run one `ytsearchN:`/`scsearchN:` query through the bundled yt-dlp and
/// return parsed results. A failed or timed-out search yields an empty vec —
/// the downloader hub surfaces that as "no matches" rather than a hard error,
/// mirroring the legacy Python `do_search` which also tolerated provider
/// failures per-provider.
pub fn search_provider(helper: &Path, provider: &str, query: &str, requested_limit: usize) -> Vec<SearchResult> {
    let (prefix, limit, provider_label) = match provider {
        "soundcloud" => ("scsearch", requested_limit.clamp(1, SC_LIMIT), "soundcloud"),
        _ => ("ytsearch", requested_limit.clamp(1, crate::contracts::types::MAX_DOWNLOADER_SEARCH_LIMIT), "youtube"),
    };
    let raw = run_ytdlp(helper, prefix, query, limit);
    raw.into_iter()
        .map(|value| {
            let id = value.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let url = value
                .get("webpage_url")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}"));
            let duration = value.get("duration").and_then(|v| v.as_u64()).map(|seconds| format_duration(seconds)).unwrap_or_default();
            let thumbnail = value
                .get("thumbnail")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    value
                        .get("thumbnails")
                        .and_then(|v| v.as_array())
                        .and_then(|list| list.last())
                        .and_then(|v| v.get("url"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("")
                .to_string();
            SearchResult {
                provider: provider_label.to_string(),
                query: value.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                title: value.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                artist: value.get("uploader").and_then(|v| v.as_str()).or_else(|| value.get("channel").and_then(|v| v.as_str())).map(str::to_string),
                duration: (!duration.is_empty()).then_some(duration),
                thumbnail_url: (!thumbnail.is_empty()).then_some(thumbnail),
                url: (!url.is_empty()).then_some(url),
            }
        })
        .collect()
}

/// Execute the yt-dlp search and parse JSON-per-line output. Each result is a
/// single JSON object on its own line (`--dump-json` with `--flat-playlist`).
fn run_ytdlp(helper: &Path, prefix: &str, query: &str, limit: usize) -> Vec<serde_json::Value> {
    let mut command = Command::new(helper);
    command.args([
        "--dump-json",
        "--no-warnings",
        "--socket-timeout",
        "15",
        "--flat-playlist",
        "--playlist-items",
        &format!("1:{limit}"),
        &format!("{prefix}{limit}:{query}"),
    ]);
    let output = match run_with_timeout(command, SEARCH_TIMEOUT) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("[luno-downloader] {prefix} search failed: {error}");
            return Vec::new();
        }
    };
    if !output.status.success() {
        eprintln!(
            "[luno-downloader] {prefix} search exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .collect()
}

/// Run a child process to completion with a hard wall-clock deadline, draining
/// pipes via reader threads so a chatty child cannot fill the OS pipe buffer.
fn run_with_timeout(mut command: Command, timeout: Duration) -> Result<std::process::Output, String> {
    command.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|error| format!("process could not start: {error}"))?;
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stdout_pipe.as_mut() {
            use std::io::Read;
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
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
                return Ok(std::process::Output {
                    status,
                    stdout: stdout_reader.join().unwrap_or_default(),
                    stderr: stderr_reader.join().unwrap_or_default(),
                });
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("timed out after {}s", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(format!("process wait failed: {error}")),
        }
    }
}

fn format_duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    let secs = seconds % 60;
    format!("{minutes}:{secs:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_durations_like_the_legacy_ui() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(59), "0:59");
        assert_eq!(format_duration(268), "4:28");
        assert_eq!(format_duration(3600), "60:00");
    }
}
