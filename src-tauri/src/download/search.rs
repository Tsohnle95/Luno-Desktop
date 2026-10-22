//! Real yt-dlp search for the downloader hub. The provider-neutral
//! `workflows::search_sources` returns raw provider QUERIES (`ytsearch1:`);
//! this module executes those queries against the bundled yt-dlp binary and
//! parses the JSON-per-line output into rich results (title, artist,
//! duration, thumbnail) so the UI shows real songs instead of raw query
//! strings. Process invocations are isolated here so `state.rs` stays thin
//! and tests can inject fakes.

use crate::contracts::types::SearchResult;
use crate::download::sync::PlaylistEntry;
use crate::process::supervisor::{ProcessSpec, SuperviseError, Supervisor};
use crate::runtime::task_registry::CancellationToken;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const SEARCH_TIMEOUT: Duration = Duration::from_secs(12);
const WEB_SEARCH_TIMEOUT: Duration = Duration::from_secs(7);
const PLAYLIST_TIMEOUT: Duration = Duration::from_secs(60);
pub const YTDLP_COOKIE_BROWSERS: &[&str] = &["brave", "chrome", "chromium", "edge", "firefox", "opera", "safari", "vivaldi", "whale"];
/// SoundCloud yields fewer relevant hits than YouTube; keep its cap modest.
const SC_LIMIT: usize = 5;

pub fn is_supported_ytdlp_cookie_browser(browser: &str) -> bool {
    YTDLP_COOKIE_BROWSERS.contains(&browser)
}

fn playlist_access_error(cookies_browser: Option<&str>) -> String {
    match cookies_browser {
        Some(browser) => format!("YouTube could not access this playlist using cookies from {browser}. Check that this browser is signed in to the account that owns the playlist, then choose another browser and retry if needed."),
        None => "YouTube could not access this playlist. Choose a browser signed in to the account that owns it in the sync prompt, then retry. If this playlist should be public, check that the link is correct.".into(),
    }
}

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

/// URL lookup must enumerate the source itself, never search for its text.
/// Flat enumeration expands playlists into independent, cancellable download jobs.
pub fn resolve_download_url(supervisor: &Supervisor, ytdlp: &Path, url: &str) -> Result<Vec<SearchResult>, String> {
    crate::download::workflows::validate_http_url(url).map_err(|_| "invalid download URL".to_owned())?;
    let entries = playlist_entries(supervisor, ytdlp, url, None, &CancellationToken::new())?;
    Ok(entries.into_iter().map(|entry| {
        let source = entry.url.unwrap_or_default();
        SearchResult { provider: "youtube".into(), query: source.clone(), title: entry.title,
            artist: entry.channel, duration: entry.duration_secs.map(format_duration),
            thumbnail_url: None, url: Some(source) }
    }).collect())
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

/// Extract a concrete YouTube watch ID from an allowlisted watch URL. Search
/// rows may carry tracking query parameters; callers persist only the ID and
/// rebuild the canonical watch URL before using it as a download source.
pub fn youtube_video_id(input: &str) -> Option<String> {
    let rest = input.strip_prefix("https://")?;
    let (host, path_query) = rest.split_once('/')?;
    if !matches!(host, "youtube.com" | "www.youtube.com" | "music.youtube.com" | "m.youtube.com") {
        return None;
    }
    let (path, query) = path_query.split_once('?')?;
    if path != "watch" { return None; }
    let id = query.split('&').find_map(|part| part.strip_prefix("v="))?.split('#').next()?;
    crate::download::sync::youtube_watch_url(id).map(|_| id.to_owned())
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

/// Resolved metadata for one concrete media URL. Direct-URL and URL-queue
/// downloads arrive without a search row, so the filename would otherwise fall
/// back to the opaque job id and show up as "Unknown artist" (filename
/// metadata is the only production metadata source).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaProbe {
    pub title: String,
    pub artist: Option<String>,
    pub duration: Option<u64>,
    pub thumbnail_url: Option<String>,
}

/// Resolve a single URL's metadata through yt-dlp (`--dump-single-json`).
/// Callers treat this as best effort: a failure must never block the download,
/// it only degrades the chosen filename.
pub fn probe_media(ytdlp: &Path, url: &str) -> Result<MediaProbe, String> {
    let mut command = Command::new(ytdlp);
    command.args([
        "--ignore-config",
        "--dump-single-json",
        "--no-warnings",
        "--no-playlist",
        "--socket-timeout",
        "15",
        "--",
        url,
    ]);
    let output = run_with_timeout(command, SEARCH_TIMEOUT)
        .map_err(|error| format!("metadata probe failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "metadata probe exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("metadata probe returned malformed JSON: {error}"))?;
    Ok(media_probe_from_json(&value))
}

/// Resolve the stable IDs, titles and watch URLs for an entire YouTube
/// playlist without downloading media. The child is polled so cancelling the
/// owning sync task also kills enumeration instead of leaving yt-dlp detached.
#[derive(Debug)]
pub struct PlaylistListing {
    pub title: String,
    pub total: usize,
    pub unavailable: usize,
    pub entries: Vec<PlaylistEntry>,
}

pub fn playlist_entries(supervisor: &Supervisor, ytdlp: &Path, url: &str, cookies_browser: Option<&str>, token: &CancellationToken) -> Result<Vec<PlaylistEntry>, String> {
    let listing = playlist_listing(supervisor, ytdlp, url, cookies_browser, token)?;
    if listing.entries.is_empty() { Err("playlist contains no downloadable videos".into()) } else { Ok(listing.entries) }
}

pub fn playlist_listing_with_browser_fallback(supervisor: &Supervisor, ytdlp: &Path, url: &str, cookies_browser: Option<&str>, token: &CancellationToken) -> Result<PlaylistListing, String> {
    with_playlist_cookie_fallback(cookies_browser, |browser| playlist_listing(supervisor, ytdlp, url, browser, token))
}

pub fn playlist_listing(supervisor: &Supervisor, ytdlp: &Path, url: &str, cookies_browser: Option<&str>, token: &CancellationToken) -> Result<PlaylistListing, String> {
    let mut args = vec!["--ignore-config".to_owned()];
    if let Some(browser) = cookies_browser {
        if !is_supported_ytdlp_cookie_browser(browser) {
            return Err("unsupported browser for YouTube playlist sign-in".into());
        }
        args.extend(["--cookies-from-browser".to_owned(), browser.to_owned()]);
    }
    args.extend([
        "--dump-single-json".into(), "--flat-playlist".into(), "--yes-playlist".into(), "--no-warnings".into(),
        "--socket-timeout".into(), "15".into(), "--".into(), url.into(),
    ]);
    let mut spec = ProcessSpec::new(ytdlp, args, PLAYLIST_TIMEOUT);
    spec.output_limit = 8 * 1024 * 1024;
    let output = supervisor.run_cancellable(spec, token).map_err(|error| match error {
        SuperviseError::Cancelled => "cancelled".into(),
        SuperviseError::Timeout => "playlist lookup timed out".into(),
        SuperviseError::Spawn(error) | SuperviseError::Io(error) => format!("playlist lookup could not start: {error}"),
        SuperviseError::OutputThread => "playlist lookup output was unavailable".into(),
    })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        if detail.contains("YouTube said: The playlist does not exist") {
            return Err(playlist_access_error(cookies_browser));
        }
        return Err(format!("playlist lookup exited {}: {}", output.status, detail.trim().chars().take(800).collect::<String>()));
    }
    if output.truncated_stdout { return Err("Playlist lookup exceeded the safe response size. No partial playlist was returned.".into()); }
    parse_playlist_listing(&String::from_utf8_lossy(&output.stdout))
}

pub fn playlist_entries_with_browser_fallback(supervisor: &Supervisor, ytdlp: &Path, url: &str, cookies_browser: Option<&str>, token: &CancellationToken) -> Result<Vec<PlaylistEntry>, String> {
    with_playlist_cookie_fallback(cookies_browser, |browser| playlist_entries(supervisor, ytdlp, url, browser, token))
}

fn with_playlist_cookie_fallback<T>(cookies_browser: Option<&str>, mut lookup: impl FnMut(Option<&str>) -> Result<T, String>) -> Result<T, String> {
    match lookup(None) {
        Err(error) if error.starts_with("YouTube could not access this playlist") => match cookies_browser {
            Some(browser) => lookup(Some(browser)),
            None => Err(error),
        },
        result => result,
    }
}

fn parse_playlist_listing(output: &str) -> Result<PlaylistListing, String> {
    if output.trim().is_empty() { return Err("Playlist lookup returned no complete response. Retry the lookup.".into()); }
    let values = if let Ok(value) = serde_json::from_str::<serde_json::Value>(output.trim()) {
        vec![value]
    } else {
        output.lines().filter(|line| !line.trim().is_empty()).map(|line| serde_json::from_str(line)
            .map_err(|_| "Playlist lookup returned incomplete or invalid data. Retry the lookup.".to_owned())).collect::<Result<Vec<serde_json::Value>, String>>()?
    };
    let mut listing = PlaylistListing { title: "YouTube playlist".into(), total: 0, unavailable: 0, entries: Vec::new() };
    let mut seen = std::collections::HashSet::new();
    for value in values {
        let rows = if let Some(entries) = value.get("entries") {
            if let Some(title) = value.get("title").and_then(|v| v.as_str()).filter(|v| !v.trim().is_empty()) { listing.title = title.trim().to_owned(); }
            entries.as_array().ok_or_else(|| "Playlist lookup returned invalid entries.".to_owned())?.clone()
        } else { vec![value] };
        for row in rows {
            if let Some(title) = row.get("playlist_title").and_then(|v| v.as_str()).filter(|v| !v.trim().is_empty()) { listing.title = title.trim().to_owned(); }
            if let Some(id) = row.get("id").and_then(|v| v.as_str()).filter(|v| !v.trim().is_empty()) {
                if !seen.insert(id.trim().to_owned()) { continue; }
            }
            listing.total += 1;
            let unavailable = matches!(row.get("availability").and_then(|v| v.as_str()), Some("private" | "premium_only" | "subscriber_only" | "needs_auth"))
                || matches!(row.get("title").and_then(|v| v.as_str()), Some("[Private video]" | "[Deleted video]"));
            let entry = parse_playlist_entries(&row.to_string()).into_iter().next();
            if unavailable || entry.is_none() { listing.unavailable += 1; } else { listing.entries.push(entry.unwrap()); }
        }
    }
    Ok(listing)
}

fn parse_playlist_entries(output: &str) -> Vec<PlaylistEntry> {
    output.lines().filter_map(|line| {
        let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
        let id = value.get("id")?.as_str()?.trim();
        let title = value.get("title")?.as_str()?.trim();
        if id.is_empty() || title.is_empty() { return None; }
        let url = value.get("webpage_url").and_then(|value| value.as_str())
            .or_else(|| value.get("url").and_then(|value| value.as_str()).filter(|url| url.starts_with("http://") || url.starts_with("https://")))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}"));
        let channel = value.get("channel").and_then(|value| value.as_str())
            .or_else(|| value.get("uploader").and_then(|value| value.as_str()))
            .or_else(|| value.get("playlist_uploader").and_then(|value| value.as_str()))
            .map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned);
        let duration_secs = value.get("duration").and_then(|value| value.as_u64());
        Some(PlaylistEntry { id: Some(id.to_owned()), title: title.to_owned(), channel, duration_secs, url: Some(url) })
    }).collect()
}

fn media_probe_from_json(value: &serde_json::Value) -> MediaProbe {
    let string_field = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    };
    // YouTube music uploads expose `track`/`artist`; fall back to the video
    // title and uploader/channel for everything else.
    let title = string_field("track")
        .or_else(|| string_field("title"))
        .unwrap_or_default();
    let artist = string_field("artist")
        .or_else(|| string_field("creator"))
        .or_else(|| string_field("uploader"))
        .or_else(|| string_field("channel"));
    let duration = value.get("duration").and_then(|v| v.as_u64());
    let thumbnail_url = string_field("thumbnail").or_else(|| {
        value
            .get("thumbnails")
            .and_then(|v| v.as_array())
            .and_then(|list| list.last())
            .and_then(|v| v.get("url"))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
    });
    MediaProbe { title, artist, duration, thumbnail_url }
}

/// Execute the yt-dlp search and parse JSON-per-line output. Each result is a
/// single JSON object on its own line (`--dump-json` with `--flat-playlist`).
fn run_ytdlp(helper: &Path, prefix: &str, query: &str, limit: usize) -> Result<Vec<serde_json::Value>, String> {
    let mut command = Command::new(helper);
    command.args([
        "--ignore-config",
        "--dump-json",
        "--no-warnings",
        "--socket-timeout",
        "15",
        "--flat-playlist",
        "--playlist-items",
        &format!("1:{limit}"),
        &format!("{prefix}{limit}:{query}"),
    ]);
    let output = match run_with_timeout(command, if limit > 25 { Duration::from_secs(60) } else { SEARCH_TIMEOUT }) {
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

    #[cfg(unix)]
    #[test]
    fn url_lookup_enumerates_entire_playlist_without_turning_url_into_search_text() {
        use std::os::unix::fs::PermissionsExt;
        let folder = std::env::temp_dir().join(format!("luno-source-resolution-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let executable = folder.join("yt-dlp");
        std::fs::write(&executable, r#"#!/bin/sh
for arg in "$@"; do
  case "$arg" in ytsearch*|scsearch*) exit 9;; esac
done
printf '%s\n' '{"id":"first","title":"First song","channel":"Artist"}' '{"id":"second","title":"Second song","channel":"Artist"}'
"#).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let supervisor = Supervisor::new(crate::process::supervisor::ProcessRegistry::new());
        let rows = resolve_download_url(&supervisor, &executable, "https://www.youtube.com/playlist?list=PL123").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].url.as_deref(), Some("https://www.youtube.com/watch?v=first"));
        assert_eq!(rows[1].title, "Second song");
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn playlist_access_errors_direct_users_to_in_place_browser_selection() {
        let without_browser = playlist_access_error(None);
        assert!(without_browser.starts_with("YouTube could not access this playlist"));
        assert!(without_browser.contains("in the sync prompt"));
        assert!(!without_browser.contains("Settings"));

        let with_browser = playlist_access_error(Some("firefox"));
        assert!(with_browser.starts_with("YouTube could not access this playlist"));
        assert!(with_browser.contains("cookies from firefox"));
        assert!(with_browser.contains("choose another browser and retry"));
    }

    #[test]
    fn configured_browser_is_used_only_after_anonymous_playlist_access_fails() {
        let mut calls = Vec::new();
        let public = with_playlist_cookie_fallback(Some("firefox"), |browser| {
            calls.push(browser.map(str::to_owned));
            Ok::<_, String>("public playlist")
        });
        assert_eq!(public.unwrap(), "public playlist");
        assert_eq!(calls, vec![None]);

        calls.clear();
        let private = with_playlist_cookie_fallback(Some("firefox"), |browser| {
            calls.push(browser.map(str::to_owned));
            match browser {
                None => Err(playlist_access_error(None)),
                Some("firefox") => Ok("account playlist"),
                Some(_) => unreachable!(),
            }
        });
        assert_eq!(private.unwrap(), "account playlist");
        assert_eq!(calls, vec![None, Some("firefox".into())]);
    }

    #[test]
    fn non_access_playlist_errors_do_not_retry_with_browser_cookies() {
        let mut calls = Vec::new();
        let result = with_playlist_cookie_fallback(Some("firefox"), |browser| {
            calls.push(browser.map(str::to_owned));
            Err::<(), _>("playlist lookup timed out".into())
        });
        assert_eq!(result.unwrap_err(), "playlist lookup timed out");
        assert_eq!(calls, vec![None]);
    }

    #[test]
    fn formats_durations_like_the_legacy_ui() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(59), "0:59");
        assert_eq!(format_duration(268), "4:28");
        assert_eq!(format_duration(3600), "60:00");
    }

    #[test]
    fn media_probe_prefers_music_fields_and_falls_back_to_video_fields() {
        let music: serde_json::Value = serde_json::from_str(
            r#"{"track":"Never Gonna Give You Up","artist":"Rick Astley","title":"Rick Astley - Never Gonna Give You Up (Official Video)","duration":213,"thumbnail":"https://img.test/rick.jpg"}"#,
        )
        .unwrap();
        let probe = media_probe_from_json(&music);
        assert_eq!(probe.title, "Never Gonna Give You Up");
        assert_eq!(probe.artist.as_deref(), Some("Rick Astley"));
        assert_eq!(probe.duration, Some(213));
        assert_eq!(probe.thumbnail_url.as_deref(), Some("https://img.test/rick.jpg"));

        let video: serde_json::Value = serde_json::from_str(
            r#"{"title":"Some Live Set","uploader":"Some Channel","thumbnails":[{"url":"https://img.test/a.jpg"},{"url":"https://img.test/b.jpg"}]}"#,
        )
        .unwrap();
        let probe = media_probe_from_json(&video);
        assert_eq!(probe.title, "Some Live Set");
        assert_eq!(probe.artist.as_deref(), Some("Some Channel"));
        assert_eq!(probe.duration, None);
        assert_eq!(probe.thumbnail_url.as_deref(), Some("https://img.test/b.jpg"));
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
    fn replacement_video_ids_accept_only_allowlisted_youtube_watch_urls() {
        assert_eq!(youtube_video_id("https://www.youtube.com/watch?v=6U7_Qy5c_uc&feature=share"), Some("6U7_Qy5c_uc".into()));
        assert_eq!(youtube_video_id("https://music.youtube.com/watch?v=6U7_Qy5c_uc"), Some("6U7_Qy5c_uc".into()));
        assert_eq!(youtube_video_id("https://youtube.com.evil/watch?v=6U7_Qy5c_uc"), None);
        assert_eq!(youtube_video_id("https://youtu.be/6U7_Qy5c_uc"), None);
        assert_eq!(youtube_video_id("https://www.youtube.com/shorts/6U7_Qy5c_uc"), None);
        assert_eq!(youtube_video_id("https://www.youtube.com/watch?v=short"), None);
    }

    #[cfg(unix)]
    #[test]
    fn playlist_lookup_rejects_output_limit_instead_of_returning_a_partial_list() {
        use std::os::unix::fs::PermissionsExt;
        let folder = std::env::temp_dir().join(format!("luno-playlist-bounded-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let data = folder.join("listing");
        std::fs::write(&data, format!("{{\"title\":\"Full list\",\"entries\":[],\"description\":\"{}\"}}", "x".repeat(8 * 1024 * 1024))).unwrap();
        let helper = folder.join("yt-dlp");
        std::fs::write(&helper, format!("#!/bin/sh\ncat '{}'\n", data.display())).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let supervisor = Supervisor::new(crate::process::supervisor::ProcessRegistry::new());
        let error = playlist_listing(&supervisor, &helper, "https://www.youtube.com/playlist?list=PLfixture", None, &CancellationToken::new()).unwrap_err();
        std::fs::remove_dir_all(folder).unwrap();
        assert!(error.contains("No partial playlist"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn paginated_name_search_requests_more_than_fifty_results() {
        use std::os::unix::fs::PermissionsExt;
        let folder = std::env::temp_dir().join(format!("luno-search75-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let helper = folder.join("yt-dlp");
        std::fs::write(&helper, "#!/bin/sh\nn=0\nfor arg in \"$@\"; do case \"$arg\" in ytsearch*) n=${arg#ytsearch}; n=${n%%:*};; esac; done\ni=0\nwhile [ \"$i\" -lt \"$n\" ]; do printf '{\"id\":\"id-%s\",\"title\":\"Song %s\"}\\n' \"$i\" \"$i\"; i=$((i + 1)); done\n").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let results = search_provider(&helper, "youtube", "Artist", 75).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(results.len(), 75, "cumulative pagination must not silently clamp at fifty");
    }

    #[test]
    fn complete_playlist_listing_keeps_metadata_more_than_25_and_rejects_partial_json() {
        let mut entries: Vec<_> = (0..61).map(|index| serde_json::json!({"id": format!("video-{index}"), "title": format!("Artist - Song {index}"), "playlist_title": "Complete playlist"})).collect();
        entries.push(entries[0].clone());
        entries.push(serde_json::json!({"id":"private-video", "title":"[Private video]", "availability":"private"}));
        let json = serde_json::json!({"title":"Complete playlist", "entries":entries}).to_string();
        let listing = parse_playlist_listing(&json).unwrap();
        assert_eq!(listing.title, "Complete playlist");
        assert_eq!(listing.total, 62);
        assert_eq!(listing.entries.len(), 61);
        assert_eq!(listing.unavailable, 1);
        assert!(parse_playlist_listing(&json[..json.len() - 10]).is_err(), "partial playlist output must not masquerade as complete");
        assert_eq!(parse_playlist_listing("{\"id\":\"v1\",\"title\":\"Song\",\"playlist_title\":\"Line playlist\"}\n").unwrap().title, "Line playlist");
    }

    #[test]
    fn parses_flat_playlist_entries_into_stable_watch_urls() {
        let output = concat!(
            "{\"id\":\"video-1\",\"title\":\"Artist - First\",\"url\":\"video-1\",\"channel\":\"Artist - Topic\",\"duration\":292,\"availability\":\"NA\"}\n",
            "{\"id\":\"video-2\",\"title\":\"Second\",\"webpage_url\":\"https://www.youtube.com/watch?v=video-2\"}\n",
            "{\"id\":\"\",\"title\":\"Unavailable\"}\n",
            "not-json\n",
        );
        let entries = parse_playlist_entries(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id.as_deref(), Some("video-1"));
        assert_eq!(entries[0].url.as_deref(), Some("https://www.youtube.com/watch?v=video-1"));
        assert_eq!(entries[0].channel.as_deref(), Some("Artist - Topic"));
        assert_eq!(entries[0].duration_secs, Some(292));
        assert_eq!(entries[1].url.as_deref(), Some("https://www.youtube.com/watch?v=video-2"));
    }

    #[test]
    #[ignore = "requires live YouTube network access"]
    fn live_fast_search_returns_hello_eminem_results() {
        let results = search_youtube_web("hello eminem", 10).expect("live fast search");
        assert!(!results.is_empty());
        assert!(results.iter().any(|result| result.title.to_lowercase().contains("hello")));
    }
}
