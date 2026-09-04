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

const SEARCH_TIMEOUT: Duration = Duration::from_secs(12);
const WEB_SEARCH_TIMEOUT: Duration = Duration::from_secs(7);
/// SoundCloud yields fewer relevant hits than YouTube; keep its cap modest.
const SC_LIMIT: usize = 5;

/// Run one `ytsearchN:`/`scsearchN:` query through the bundled yt-dlp and
/// return parsed results. Failures remain distinct from a legitimate empty
/// result so the UI never mislabels a provider outage as "no matches".
pub fn search_provider(helper: &Path, provider: &str, query: &str, requested_limit: usize) -> Result<Vec<SearchResult>, String> {
    let (prefix, limit, provider_label) = match provider {
        "soundcloud" => ("scsearch", requested_limit.clamp(1, SC_LIMIT), "soundcloud"),
        _ => ("ytsearch", requested_limit.clamp(1, crate::contracts::types::MAX_DOWNLOADER_SEARCH_LIMIT), "youtube"),
    };
    let raw = run_ytdlp(helper, prefix, query, limit)?;
    Ok(raw.into_iter()
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
                query: url.clone(),
                title: value.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                artist: value.get("uploader").and_then(|v| v.as_str()).or_else(|| value.get("channel").and_then(|v| v.as_str())).map(str::to_string),
                duration: (!duration.is_empty()).then_some(duration),
                thumbnail_url: (!thumbnail.is_empty()).then_some(thumbnail),
                url: (!url.is_empty()).then_some(url),
            }
        })
        .collect())
}

/// Search YouTube's public results page without launching the large bundled
/// PyInstaller yt-dlp executable. Starting that binary can take 10–20 seconds
/// on macOS before it performs any network work; this path normally returns in
/// one HTTP round trip, with yt-dlp retained as the compatibility fallback.
pub fn search_youtube_web(query: &str, requested_limit: usize) -> Result<Vec<SearchResult>, String> {
    let limit = requested_limit.clamp(1, crate::contracts::types::MAX_DOWNLOADER_SEARCH_LIMIT);
    let client = reqwest::blocking::Client::builder()
        .timeout(WEB_SEARCH_TIMEOUT)
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/124 Safari/537.36")
        .build()
        .map_err(|error| format!("YouTube search client failed: {error}"))?;
    let response = client
        .get("https://www.youtube.com/results")
        .query(&[("search_query", query)])
        .send()
        .map_err(|error| format!("YouTube search request failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("YouTube search returned an error: {error}"))?;
    let body = response.text().map_err(|error| format!("YouTube search response failed: {error}"))?;
    parse_youtube_initial_data(&body, limit)
}

fn parse_youtube_initial_data(body: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
    let json = ["var ytInitialData = ", "ytInitialData = "]
        .iter()
        .find_map(|marker| body.find(marker).and_then(|at| balanced_json_object(&body[at + marker.len()..])))
        .ok_or_else(|| "YouTube search response did not contain results".to_string())?;
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| format!("YouTube search results were malformed: {error}"))?;
    let mut renderers = Vec::new();
    collect_video_renderers(&value, &mut renderers, limit);
    Ok(renderers.into_iter().filter_map(video_renderer_result).collect())
}

fn balanced_json_object(input: &str) -> Option<&str> {
    let start = input.find('{')?;
    let bytes = input.as_bytes();
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (offset, byte) in bytes[start..].iter().enumerate() {
        if quoted {
            if escaped { escaped = false; }
            else if *byte == b'\\' { escaped = true; }
            else if *byte == b'"' { quoted = false; }
            continue;
        }
        match *byte {
            b'"' => quoted = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 { return Some(&input[start..start + offset + 1]); }
            }
            _ => {}
        }
    }
    None
}

fn collect_video_renderers<'a>(value: &'a serde_json::Value, found: &mut Vec<&'a serde_json::Value>, limit: usize) {
    if found.len() >= limit { return; }
    match value {
        serde_json::Value::Object(map) => {
            if let Some(renderer) = map.get("videoRenderer") { found.push(renderer); }
            for child in map.values() { collect_video_renderers(child, found, limit); }
        }
        serde_json::Value::Array(items) => for child in items { collect_video_renderers(child, found, limit); },
        _ => {}
    }
}

fn text_value(value: Option<&serde_json::Value>) -> Option<String> {
    let value = value?;
    value.get("simpleText").and_then(|v| v.as_str()).map(str::to_owned).or_else(|| {
        let text = value.get("runs")?.as_array()?.iter().filter_map(|run| run.get("text")?.as_str()).collect::<String>();
        (!text.is_empty()).then_some(text)
    })
}

fn video_renderer_result(value: &serde_json::Value) -> Option<SearchResult> {
    let id = value.get("videoId")?.as_str()?;
    let title = text_value(value.get("title"))?;
    let artist = text_value(value.get("ownerText")).or_else(|| text_value(value.get("longBylineText")));
    let duration = text_value(value.get("lengthText"));
    let thumbnail_url = value.get("thumbnail").and_then(|v| v.get("thumbnails")).and_then(|v| v.as_array()).and_then(|items| items.last()).and_then(|v| v.get("url")).and_then(|v| v.as_str()).map(str::to_owned);
    let url = format!("https://www.youtube.com/watch?v={id}");
    Some(SearchResult { provider: "youtube".into(), query: url.clone(), title, artist, duration, thumbnail_url, url: Some(url) })
}

/// Execute the yt-dlp search and parse JSON-per-line output. Each result is a
/// single JSON object on its own line (`--dump-json` with `--flat-playlist`).
fn run_ytdlp(helper: &Path, prefix: &str, query: &str, limit: usize) -> Result<Vec<serde_json::Value>, String> {
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
        Err(error) => return Err(format!("{prefix} search failed: {error}")),
    };
    if !output.status.success() {
        return Err(format!(
            "{prefix} search exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .collect::<Vec<_>>())
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

    #[test]
    fn parses_youtube_initial_data_and_ignores_braces_inside_strings() {
        let html = r#"<script>var ytInitialData = {"contents":[{"videoRenderer":{"videoId":"abc123","title":{"runs":[{"text":"Hello {World}"}]},"ownerText":{"runs":[{"text":"Eminem"}]},"lengthText":{"simpleText":"4:11"},"thumbnail":{"thumbnails":[{"url":"https://img.test/abc.jpg"}]}}}]};</script>"#;
        let results = parse_youtube_initial_data(html, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Hello {World}");
        assert_eq!(results[0].artist.as_deref(), Some("Eminem"));
        assert_eq!(results[0].url.as_deref(), Some("https://www.youtube.com/watch?v=abc123"));
    }

    #[test]
    #[ignore = "requires live YouTube network access"]
    fn live_fast_search_returns_hello_eminem_results() {
        let results = search_youtube_web("hello eminem", 10).expect("live fast search");
        assert!(!results.is_empty());
        assert!(results.iter().any(|result| result.title.to_lowercase().contains("hello")));
    }
}
