//! Range-aware chunked streaming for staged videos (`luno-video://localhost/<abs path>`).
//!
//! WHY THIS EXISTS: the stock Tauri asset protocol buffers a response body
//! WHOLE whenever the webview's first media request carries no `Range`
//! header (`file.read_to_end` in `tauri::protocol::asset`), and wry then
//! hands WebKit the entire file as ONE giant `NSData` blob. Staged MP4s grew
//! past 100 MB, so every `<video>` load / quality-upgrade hot-swap /
//! seek-rebuffer allocated and copied hundreds of megabytes inside the UI
//! process while yt-dlp + ffmpeg saturated the disk — together reading as
//! "switch song, window unclickable for ~20 s" (2026-08-25).
//!
//! This handler answers EVERY media GET with a small (≤ [`CHUNK_LEN`]) 206
//! window and `Accept-Ranges: bytes`, so WebKit stays in range mode: seeks
//! become cheap ranged hops and no single request ever moves more than a few
//! megabytes.

use std::path::{Component, Path, PathBuf};

/// Scheme registered in `main.rs`. The Shell builds `<video>` srcs through
/// `convertFileSrc(staged_path, "luno-video")`, which produces
/// `luno-video://localhost/<percent-encoded absolute path>` on macOS.
pub const STREAM_SCHEME: &str = "luno-video";

/// Bytes served per ranged response. Large enough that normal playback does
/// not issue back-to-back scheme round-trips; small enough that each one is
/// trivial for the UI process to allocate and copy.
pub const CHUNK_LEN: u64 = 2_000_000;

/// Decode `%XX` escapes in a URI path. Returns `None` on malformed escapes,
/// invalid UTF-8, or embedded NULs. `+` is intentionally NOT decoded — this
/// is a path, not a query.
pub fn percent_decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = bytes.get(i + 1..i + 3)?;
                let hi = (*hex.first()? as char).to_digit(16)?;
                let lo = (*hex.get(1)? as char).to_digit(16)?;
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    let decoded = String::from_utf8(out).ok()?;
    if decoded.contains('\0') {
        return None;
    }
    Some(decoded)
}

/// Map the request path of a stream URL to a file INSIDE the video store.
/// The staged path is absolute, so containment (not relativization) is the
/// check: percent-decode, refuse NULs, canonicalize (which also requires the
/// file to exist), then require the result to stay under `video_root` and be
/// an `.mp4`.
pub fn resolve_stored_video(video_root: &Path, raw_request_path: &str) -> Option<PathBuf> {
    if raw_request_path.is_empty() {
        return None;
    }
    let decoded = percent_decode(raw_request_path)?;
    let candidate = PathBuf::from(decoded);
    if candidate.components().any(|component| matches!(component, Component::ParentDir)) {
        return None;
    }
    let canonical = candidate.canonicalize().ok()?;
    let canonical_root = video_root.canonicalize().ok().unwrap_or_else(|| video_root.to_path_buf());
    if !canonical.starts_with(&canonical_root) {
        return None;
    }
    let is_mp4 = canonical
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("mp4"))
        .unwrap_or(false);
    is_mp4.then_some(canonical)
}

/// How the handler should answer one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyPlan {
    /// HEAD: advertise the size, send no body.
    Head { total_len: u64 },
    /// Serve bytes `start..=end` (inclusive) as `status` (206 or 200).
    Window { status: u16, start: u64, end: u64, total_len: u64 },
    /// Range start past EOF (or an empty file): 416 with `bytes */<len>`.
    NotSatisfiable { total_len: u64 },
}

fn clamped_window(start: u64, requested_end: Option<u64>, total_len: u64) -> BodyPlan {
    if total_len == 0 {
        return BodyPlan::NotSatisfiable { total_len };
    }
    let last_byte = total_len - 1;
    let end = requested_end.unwrap_or(last_byte).min(last_byte).min(start.saturating_add(CHUNK_LEN - 1));
    if end < start {
        return BodyPlan::NotSatisfiable { total_len };
    }
    // 206 even without a Range header: media loaders accept a prefix window
    // with Content-Range as "bytes=0-", which is what pins WebKit into cheap
    // ranged follow-ups instead of expecting the whole resource at once.
    BodyPlan::Window { status: 206, start, end, total_len }
}

/// Pure decision logic for one request. `range_header` is the raw `Range`
/// header value (`None` when absent); only the FIRST range of a multi-range
/// request is honored — media loaders never rely on multipart bodies.
pub fn plan_response(method: &str, range_header: Option<&str>, total_len: u64) -> BodyPlan {    if method.eq_ignore_ascii_case("HEAD") {
        return BodyPlan::Head { total_len };
    }
    let Some(spec) = range_header.map(str::trim).filter(|value| !value.is_empty()) else {
        return clamped_window(0, None, total_len);
    };
    let Some(spec) = spec.strip_prefix("bytes=") else {
        // Unknown range unit: serve the prefix window rather than failing.
        return clamped_window(0, None, total_len);
    };
    let first = spec.split(',').next().unwrap_or("").trim();
    // Suffix form: "bytes=-N" → last N bytes (capped to CHUNK_LEN).
    if let Some(suffix) = first.strip_prefix('-') {
        let n: u64 = suffix.trim().parse().unwrap_or(0);
        if n == 0 {
            return BodyPlan::NotSatisfiable { total_len };
        }
        let start = total_len.saturating_sub(n.min(CHUNK_LEN));
        return clamped_window(start, None, total_len);
    }
    let mut parts = first.splitn(2, '-');
    let Ok(start) = parts.next().unwrap_or("").trim().parse::<u64>() else {
        return clamped_window(0, None, total_len);
    };
    if start >= total_len {
        return BodyPlan::NotSatisfiable { total_len };
    }
    let requested_end = parts.next().and_then(|value| value.trim().parse::<u64>().ok());
    clamped_window(start, requested_end, total_len)
}

/// Full handler for one scheme request: containment → metadata → plan →
/// headers + bounded body read. Lives in the lib (not `main.rs`) so the
/// whole path is covered by deterministic file-backed tests; main.rs only
/// spawns this onto the async runtime and hands the response to wry.
pub fn response_for(
    video_root: &Path,
    method: &str,
    request_path: &str,
    range_header: Option<&str>,
) -> tauri::http::Response<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    let empty = |status: u16| {
        tauri::http::Response::builder().status(status).body(Vec::new()).unwrap()
    };

    // Containment FIRST: canonicalize + starts_with(video store), mp4 only.
    let Some(path) = resolve_stored_video(video_root, request_path) else { return empty(404) };
    let Ok(metadata) = std::fs::metadata(&path) else { return empty(404) };
    let total_len = metadata.len();

    match plan_response(method, range_header, total_len) {
        BodyPlan::Head { total_len } => tauri::http::Response::builder()
            .status(200)
            .header("Content-Type", "video/mp4")
            .header("Accept-Ranges", "bytes")
            .header("Content-Length", total_len.to_string())
            .body(Vec::new())
            .unwrap(),
        BodyPlan::NotSatisfiable { total_len } => tauri::http::Response::builder()
            .status(416)
            .header("Content-Range", format!("bytes */{total_len}"))
            .body(Vec::new())
            .unwrap(),
        BodyPlan::Window { status, start, end, .. } => {
            let Ok(mut file) = std::fs::File::open(&path) else { return empty(404) };
            if file.seek(SeekFrom::Start(start)).is_err() { return empty(404) }
            let mut buffer = vec![0u8; (end - start + 1) as usize];
            if file.read_exact(&mut buffer).is_err() { return empty(404) }
            match tauri::http::Response::builder()
                .status(status)
                .header("Content-Type", "video/mp4")
                .header("Accept-Ranges", "bytes")
                .header("Content-Range", format!("bytes {start}-{end}/{total_len}"))
                .header("Content-Length", buffer.len().to_string())
                .body(buffer)
            {
                Ok(response) => response,
                Err(_) => empty(500),
            }
        }
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;

    #[test]
    fn percent_decode_handles_escapes_and_rejects_garbage() {
        assert_eq!(percent_decode("/Users/ty/Application%20Support/videos/current-x.mp4").as_deref(), Some("/Users/ty/Application Support/videos/current-x.mp4"));
        assert_eq!(percent_decode("%2Ftmp%2Fa.mp4").as_deref(), Some("/tmp/a.mp4"));
        assert_eq!(percent_decode("plain.mp4").as_deref(), Some("plain.mp4"));
        assert_eq!(percent_decode("%zz"), None);
        assert_eq!(percent_decode("%2"), None);
        assert_eq!(percent_decode("%00"), None);
    }

    #[test]
    fn resolve_stored_video_containment_and_extension() {
        let root = std::env::temp_dir().join(format!("luno-stream-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let video = root.join("current-abc.mp4");
        std::fs::write(&video, b"x").unwrap();
        let outside = std::env::temp_dir().join(format!("luno-stream-outside-{}.mp4", std::process::id()));
        std::fs::write(&outside, b"x").unwrap();

        let encoded = format!("/{}", urlencoding_lite(&video.to_string_lossy()));
        assert_eq!(resolve_stored_video(&root, &encoded), Some(video.canonicalize().unwrap()));
        // Outside the store → refused even though it exists.
        assert_eq!(resolve_stored_video(&root, &format!("/{}", urlencoding_lite(&outside.to_string_lossy()))), None);
        // Encoded traversal that canonicalizes outside → refused.
        assert_eq!(resolve_stored_video(&root, &format!("/{}/../escape.mp4", urlencoding_lite(&root.to_string_lossy()))), None);
        // Wrong extension inside the store → refused.
        let txt = root.join("notes.txt");
        std::fs::write(&txt, b"x").unwrap();
        assert_eq!(resolve_stored_video(&root, &format!("/{}", urlencoding_lite(&txt.to_string_lossy()))), None);

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
    }

    /// Minimal encodeURIComponent stand-in for tests (the app uses the JS API).
    fn urlencoding_lite(value: &str) -> String {
        value.bytes().map(|b| {
            if b.is_ascii_alphanumeric() || b"._~-/@".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }
        }).collect()
    }

    #[test]
    fn head_advertises_length_without_body() {
        assert_eq!(plan_response("HEAD", None, 123), BodyPlan::Head { total_len: 123 });
        assert_eq!(plan_response("head", Some("bytes=0-"), 5), BodyPlan::Head { total_len: 5 });
    }

    #[test]
    fn missing_range_serves_first_window_as_206() {
        assert_eq!(plan_response("GET", None, 10_000_000), BodyPlan::Window { status: 206, start: 0, end: CHUNK_LEN - 1, total_len: 10_000_000 });
        // Files smaller than one chunk are served whole.
        assert_eq!(plan_response("GET", None, 500), BodyPlan::Window { status: 206, start: 0, end: 499, total_len: 500 });
        assert_eq!(plan_response("GET", None, 0), BodyPlan::NotSatisfiable { total_len: 0 });
    }

    #[test]
    fn open_ended_and_bounded_ranges_are_chunk_capped() {
        // bytes=100- → 100..min(CHUNK)-capped
        assert_eq!(plan_response("GET", Some("bytes=100-"), 50_000_000), BodyPlan::Window { status: 206, start: 100, end: CHUNK_LEN + 99, total_len: 50_000_000 });
        // Explicit end beyond EOF clamps to the last byte.
        assert_eq!(plan_response("GET", Some("bytes=9999999-"), 10_000_000), BodyPlan::Window { status: 206, start: 9_999_999, end: 9_999_999, total_len: 10_000_000 });
        // Bounded range larger than a chunk shrinks to the chunk window.
        let planned = plan_response("GET", Some("bytes=0-9999999"), 20_000_000);
        assert_eq!(planned, BodyPlan::Window { status: 206, start: 0, end: CHUNK_LEN - 1, total_len: 20_000_000 });
        // Small bounded range served exactly.
        assert_eq!(plan_response("GET", Some("bytes=10-19"), 100), BodyPlan::Window { status: 206, start: 10, end: 19, total_len: 100 });
    }

    #[test]
    fn suffix_ranges_tail_the_file_within_the_cap() {
        assert_eq!(plan_response("GET", Some("bytes=-500"), 10_000), BodyPlan::Window { status: 206, start: 9_500, end: 9_999, total_len: 10_000 });
        // Suffix bigger than a chunk caps to the final CHUNK_LEN bytes.
        assert_eq!(plan_response("GET", Some("bytes=-9999999"), 50_000_000), BodyPlan::Window { status: 206, start: 50_000_000 - CHUNK_LEN, end: 49_999_999, total_len: 50_000_000 });
        assert_eq!(plan_response("GET", Some("bytes=-0"), 100), BodyPlan::NotSatisfiable { total_len: 100 });
    }

    #[test]
    fn unsatisfiable_and_malformed_ranges_fall_back_safely() {
        assert_eq!(plan_response("GET", Some("bytes=999-"), 100), BodyPlan::NotSatisfiable { total_len: 100 });
        // Unknown unit → prefix window instead of a hard failure.
        assert_eq!(plan_response("GET", Some("chunks=0-1"), 1_000), BodyPlan::Window { status: 206, start: 0, end: 999, total_len: 1_000 });
        // Garbage numbers → prefix window.
        assert_eq!(plan_response("GET", Some("bytes=abc-def"), 1_000), BodyPlan::Window { status: 206, start: 0, end: 999, total_len: 1_000 });
        // Only the first range of a multi-range request is honored.
        assert_eq!(plan_response("GET", Some("bytes=5-9,50-60"), 100), BodyPlan::Window { status: 206, start: 5, end: 9, total_len: 100 });
    }

    /// END-TO-END handler coverage against a real staged-file layout: the
    /// exact code path main.rs hands each `luno-video://` request to.
    #[test]
    fn response_for_serves_real_files_in_windows() {
        let root = std::env::temp_dir().join(format!("luno-stream-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let body: Vec<u8> = (0u8..=99).collect();
        std::fs::write(root.join("current-abc.mp4"), &body).unwrap();

        let url = format!("/{}", encode_uri_path(&root.join("current-abc.mp4").to_string_lossy()));
        let get = |range: Option<&'static str>| response_for(&root, "GET", &url, range);

        // No Range header → small 206 prefix window with Content-Range…
        let response = get(None);
        assert_eq!(response.status(), 206);
        assert_eq!(response.headers()["Content-Range"].to_str().unwrap(), format!("bytes 0-{}/100", CHUNK_LEN.min(100) - 1));
        assert_eq!(response.headers()["Accept-Ranges"], "bytes");
        // …and the body bytes are the real file prefix.
        assert_eq!(*response.body(), body[..(CHUNK_LEN.min(100) as usize)]);

        // Bounded middle window returns exactly those file bytes.
        let response = get(Some("bytes=10-19"));
        assert_eq!(response.status(), 206);
        assert_eq!(response.headers()["Content-Range"].to_str().unwrap(), "bytes 10-19/100");
        assert_eq!(*response.body(), body[10..20]);

        // Suffix window tails the file.
        let response = get(Some("bytes=-7"));
        assert_eq!(*response.body(), body[93..]);

        // HEAD advertises the size with no body.
        let response = response_for(&root, "HEAD", &url, None);
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["Content-Length"].to_str().unwrap(), "100");
        assert!(response.body().is_empty());

        // Start past EOF → 416 advertising the length.
        let response = get(Some("bytes=500-"));
        assert_eq!(response.status(), 416);
        assert_eq!(response.headers()["Content-Range"].to_str().unwrap(), "bytes */100");

        // Anything outside the store is a plain 404 — never served.
        let outside = std::env::temp_dir().join(format!("luno-stream-e2e-out-{}.mp4", std::process::id()));
        std::fs::write(&outside, b"secret").unwrap();
        let escaped_url = format!("/{}", encode_uri_path(&outside.to_string_lossy()));
        assert_eq!(response_for(&root, "GET", &escaped_url, None).status(), 404);

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
    }

    /// Test-only URI-path encoder (inverse of `percent_decode`) mirroring
    /// `encodeURIComponent`'s output shape for absolute paths.
    fn encode_uri_path(value: &str) -> String {
        value.bytes().map(|b| {
            if b.is_ascii_alphanumeric() || b"._~-/@".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }
        }).collect()
    }
}
