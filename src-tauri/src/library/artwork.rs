//! Bounded, lazy embedded-artwork extraction for library tracks.
//!
//! The desktop library scan deliberately does not read audio metadata for
//! every file (that would slow first scan significantly), so library tracks
//! carry no `artwork_url`. Instead, artwork is extracted on demand for the
//! rows the UI actually shows, embedded in the file's tags (ID3 APIC, ISO-BMFF
//! `covr`, FLAC `PICTURE`), and served to the webview as a `data:` URL.

use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::path::Path;
use std::sync::Mutex;

/// Bounded in-memory cache of extracted artwork `data:` URLs, keyed by track
/// id. Entries are evicted oldest-first once the total payload exceeds
/// [`ARTWORK_CACHE_BYTES`], so a large library cannot exhaust memory.
pub const ARTWORK_CACHE_BYTES: usize = 32 * 1024 * 1024;
/// Single artwork larger than this is rejected to avoid a single 10 MB JPEG
/// becoming a 14 MB base64 string and spiking the WebView heap.
pub const MAX_SINGLE_ARTWORK_BYTES: usize = 2 * 1024 * 1024;

#[derive(Default)]
pub struct ArtworkCache {
    entries: HashMap<String, (usize, String)>,
    order: VecDeque<String>,
    total_bytes: usize,
}

impl ArtworkCache {
    pub fn get(&self, track_id: &str) -> Option<String> {
        self.entries.get(track_id).map(|(_, url)| url.clone())
    }

    pub fn insert(&mut self, track_id: String, url: String) {
        let bytes = url.len();
        if let Some((previous, _)) = self.entries.insert(track_id.clone(), (bytes, url)) {
            self.total_bytes = self.total_bytes.saturating_sub(previous);
        }
        self.total_bytes += bytes;
        self.order.push_back(track_id);
        while self.total_bytes > ARTWORK_CACHE_BYTES {
            match self.order.pop_front() {
                Some(evicted) => {
                    if let Some((size, _)) = self.entries.remove(&evicted) {
                        self.total_bytes = self.total_bytes.saturating_sub(size);
                    }
                }
                None => break,
            }
        }
    }
}

/// Cheap MIME sniffing for embedded pictures whose declared media type is
/// empty or implausible (some ID3 tags omit it).
fn guess_mime(data: &[u8]) -> &'static str {
    if data.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        "image/png"
    } else if data.starts_with(b"GIF8") {
        "image/gif"
    } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

/// Read the first embedded picture for an audio file and return it as a
/// `data:` URL. `None` means the file has no usable artwork; extraction
/// failures are deliberately silent so a single odd file cannot break the row.
///
/// MP3 art comes from ID3v2 APIC frames via the `id3` crate because
/// Symphonia's mp3 demuxer intentionally ignores ID3 tags; other containers
/// (ISO-BMFF `covr`, FLAC `PICTURE`) fall back to Symphonia probe metadata.
#[cfg(target_os = "macos")]
pub fn embedded_artwork_data_url(path: &Path) -> Option<String> {
    if let Some(url) = id3_artwork_data_url(path) {
        return Some(url);
    }
    symphonia_artwork_data_url(path)
}

#[cfg(target_os = "macos")]
fn id3_artwork_data_url(path: &Path) -> Option<String> {
    use base64::Engine;
    let tag = id3::Tag::read_from_path(path).ok()?;
    let picture = tag.pictures().next()?;
    if picture.data.is_empty() || picture.data.len() > MAX_SINGLE_ARTWORK_BYTES {
        return None;
    }
    let mime = if picture.mime_type.starts_with("image/") {
        picture.mime_type.clone()
    } else {
        guess_mime(&picture.data).to_string()
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&picture.data);
    // Base64 inflates ~33%; reject if the final data URL would be huge.
    if encoded.len() > MAX_SINGLE_ARTWORK_BYTES * 4 / 3 + 64 {
        return None;
    }
    Some(format!("data:{mime};base64,{encoded}"))
}

#[cfg(target_os = "macos")]
fn symphonia_artwork_data_url(path: &Path) -> Option<String> {
    use base64::Engine;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = File::open(path).ok()?;
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(extension);
    }
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let metadata = probed.format.metadata();
    let visual = metadata.current()?.visuals().first()?;
    let data: &[u8] = visual.data.as_ref();
    if data.is_empty() || data.len() > MAX_SINGLE_ARTWORK_BYTES {
        return None;
    }
    let mime = if visual.media_type.starts_with("image/") {
        visual.media_type.as_str()
    } else {
        guess_mime(data)
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    if encoded.len() > MAX_SINGLE_ARTWORK_BYTES * 4 / 3 + 64 {
        return None;
    }
    Some(format!("data:{mime};base64,{encoded}"))
}

#[cfg(not(target_os = "macos"))]
pub fn embedded_artwork_data_url(_path: &Path) -> Option<String> {
    None
}

/// Resolve artwork for a track id through the process-wide cache, extracting
/// on first request. The track must be part of the current library snapshot.
pub struct TrackArtwork {
    pub cache: Mutex<ArtworkCache>,
    pub resolve: Box<dyn Fn(&str) -> Option<std::path::PathBuf> + Send + Sync>,
}

impl TrackArtwork {
    pub fn new(
        resolve: impl Fn(&str) -> Option<std::path::PathBuf> + Send + Sync + 'static,
    ) -> Self {
        Self {
            cache: Mutex::new(ArtworkCache::default()),
            resolve: Box::new(resolve),
        }
    }

    pub fn artwork(&self, track_id: &str) -> Option<String> {
        let cached = self.cache.lock().ok()?.get(track_id);
        if let Some(url) = cached {
            return Some(url);
        }
        let path = (self.resolve)(track_id)?;
        let url = embedded_artwork_data_url(&path)?;
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(track_id.to_string(), url.clone());
        }
        Some(url)
    }
}