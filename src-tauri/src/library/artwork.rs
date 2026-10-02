//! Bounded embedded-artwork extraction and thumbnail caching for library tracks.
//!
//! The desktop library scan deliberately does not read audio metadata for
//! every file (that would slow first scan significantly), so library tracks
//! carry no `artwork_url`. The frontend warms artwork in bounded background
//! batches; visible rows raise their requests above that work. Extracted,
//! downscaled covers are kept in a bounded process cache and a versioned
//! persistent cache outside the music library, then served as `data:` URLs.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Bounded in-memory cache of extracted artwork `data:` URLs, keyed by track
/// id. Entries are evicted oldest-first once the total payload exceeds
/// [`ARTWORK_CACHE_BYTES`], so a large library cannot exhaust memory.
pub const ARTWORK_CACHE_BYTES: usize = 32 * 1024 * 1024;
/// Single artwork larger than this is rejected to avoid a single 10 MB JPEG
/// becoming a 14 MB base64 string and spiking the WebView heap.
pub const MAX_SINGLE_ARTWORK_BYTES: usize = 2 * 1024 * 1024;
const DISK_ARTWORK_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const DISK_ARTWORK_CACHE_TARGET_BYTES: u64 = DISK_ARTWORK_CACHE_BYTES * 3 / 4;
const NO_ARTWORK_MARKER: &str = "luno:no-embedded-artwork";
static NEXT_ARTWORK_STAGE: AtomicU64 = AtomicU64::new(1);
/// Display size served to the webview. Embedded covers are routinely
/// 1400–3000 px, and every rendered row decodes its own bitmap (a 3000 px
/// cover is ~36 MB decoded); a 4k-song Home view therefore put gigabytes
/// into the renderer in seconds. The UI never shows art above ~200 px, so
/// large JPEG/PNG art is downscaled here — source files are untouched.
/// Small art is served byte-identical (preserves PNG transparency and
/// avoids a lossy round-trip that buys nothing).
#[cfg(target_os = "macos")]
pub const MAX_ARTWORK_DIM: u32 = 512;
/// JPEG quality for downscaled art: visually transparent at card sizes,
/// roughly a quarter the bytes of the encoder default.
#[cfg(target_os = "macos")]
const DOWNSCALE_JPEG_QUALITY: u8 = 82;

#[derive(Default)]
pub struct ArtworkCache {
    entries: HashMap<String, (usize, String)>,
    order: VecDeque<String>,
    total_bytes: usize,
}

impl ArtworkCache {
    pub fn get(&mut self, track_id: &str) -> Option<String> {
        let url = self.entries.get(track_id).map(|(_, url)| url.clone())?;
        self.order.retain(|id| id != track_id);
        self.order.push_back(track_id.to_owned());
        Some(url)
    }

    pub fn insert(&mut self, track_id: String, url: String) {
        let bytes = url.len();
        if let Some((previous, _)) = self.entries.insert(track_id.clone(), (bytes, url)) {
            self.total_bytes = self.total_bytes.saturating_sub(previous);
        }
        self.total_bytes += bytes;
        self.order.retain(|id| id != &track_id);
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

    pub fn invalidate(&mut self, track_id: &str) {
        if let Some((bytes, _)) = self.entries.remove(track_id) {
            self.total_bytes = self.total_bytes.saturating_sub(bytes);
        }
        self.order.retain(|id| id != track_id);
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
    picture_data_url(&mime, &picture.data)
}

/// Shared tail for every container: downscale oversized JPEG/PNG to display
/// size, then base64 the bytes that remain. Any failure anywhere in the
/// imaging path falls back to the original bytes (or `None` when even those
/// are unusable) — a single odd file must never break its row.
#[cfg(target_os = "macos")]
fn picture_data_url(declared_mime: &str, data: &[u8]) -> Option<String> {
    use base64::Engine;
    let mut mime = declared_mime.to_string();
    let mut bytes: &[u8] = data;
    let owned: Vec<u8>;
    if mime == "image/jpeg" || mime == "image/png" {
        if let Some(small) = downscaled_jpeg(data) {
            owned = small;
            bytes = &owned;
            mime = "image/jpeg".to_string();
        }
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    // Base64 inflates ~33%; reject if the final data URL would be huge.
    if encoded.len() > MAX_SINGLE_ARTWORK_BYTES * 4 / 3 + 64 {
        return None;
    }
    Some(format!("data:{mime};base64,{encoded}"))
}

/// Validates a provider image with bounded decoding, then stores a small JPEG
/// suitable for both the embedded tag and the WebView. The source is already
/// byte-capped by the provider client; dimension and decoder limits protect
/// against compressed image bombs.
#[cfg(target_os = "macos")]
pub fn normalize_provider_image(data: &[u8]) -> Result<(Vec<u8>, &'static str, String), String> {
    use base64::Engine;
    use std::io::Cursor;

    if data.is_empty() || data.len() > MAX_SINGLE_ARTWORK_BYTES {
        return Err("The selected image is empty or too large.".into());
    }
    let mut reader = image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|error| format!("Could not inspect the artwork image: {error}"))?;
    if !matches!(reader.format(), Some(image::ImageFormat::Jpeg | image::ImageFormat::Png)) {
        return Err("Only JPEG and PNG artwork can be embedded.".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(5_000);
    limits.max_image_height = Some(5_000);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| format!("The artwork image could not be decoded: {error}"))?;
    let small = image.thumbnail(MAX_ARTWORK_DIM, MAX_ARTWORK_DIM).to_rgb8();
    let mut normalized = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(
        &mut normalized,
        DOWNSCALE_JPEG_QUALITY,
    )
    .encode_image(&small)
    .map_err(|error| format!("Could not prepare the artwork image: {error}"))?;
    if normalized.len() > MAX_SINGLE_ARTWORK_BYTES {
        return Err("The prepared artwork image is too large.".into());
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(&normalized);
    let url = format!("data:image/jpeg;base64,{encoded}");
    Ok((normalized, "image/jpeg", url))
}

#[cfg(not(target_os = "macos"))]
pub fn normalize_provider_image(_data: &[u8]) -> Result<(Vec<u8>, &'static str, String), String> {
    Err("Artwork tagging is only available in the macOS desktop app.".into())
}

/// Checks for a usable embedded picture without creating a base64 data URL.
/// Used by the Settings artwork scanner, so scanning thousands of tracks does
/// not allocate every cover image in the webview representation.
pub fn has_embedded_artwork(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        use lofty::file::TaggedFileExt;

        match lofty::read_from_path(path) {
            Ok(file) => file
                .tags()
                .iter()
                .flat_map(|tag| tag.pictures())
                .any(|picture| !picture.data().is_empty() && picture.data().len() <= MAX_SINGLE_ARTWORK_BYTES),
            // Keep the scanner aligned with formats served by Symphonia even
            // when Lofty cannot parse that container (for example WebM).
            Err(_) => embedded_artwork_data_url(path).is_some(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        false
    }
}

/// Embeds the selected image in a same-directory staged copy. The original
/// file is left intact unless tag writing succeeds and its identity still
/// matches the source observed before staging.
pub fn apply_embedded_artwork(path: &Path, image: &[u8], mime: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        apply_embedded_artwork_macos(path, image, mime)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, image, mime);
        Err("Artwork tagging is only available in the macOS desktop app.".into())
    }
}

#[cfg(target_os = "macos")]
fn apply_embedded_artwork_macos(path: &Path, image: &[u8], mime: &str) -> Result<(), String> {
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::tag::Tag;

    if !matches!(mime, "image/jpeg" | "image/png") || image.is_empty() || image.len() > MAX_SINGLE_ARTWORK_BYTES {
        return Err("The selected image is not a supported, verified artwork file.".into());
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("webm"))
    {
        return Err("Luno cannot safely embed artwork in WebM files. The music file was left unchanged.".into());
    }
    let _mutation = crate::library::mutations::lock_library_file_mutations();
    let source_type = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect the music file: {error}"))?
        .file_type();
    if !source_type.is_file() {
        return Err("The selected music path is not a regular file.".into());
    }
    let original_permissions = fs::metadata(path)
        .map_err(|error| format!("Could not inspect the music file: {error}"))?
        .permissions();
    let original_stamp = stamp(path)?;
    let staged = stage_path(path)?;

    let mut created_stage = false;
    let result = (|| {
        let mut source = File::open(path)
            .map_err(|error| format!("Could not read the music file: {error}"))?;
        let mut destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
            .map_err(|error| format!("Could not create a staged music file: {error}"))?;
        created_stage = true;
        io::copy(&mut source, &mut destination)
            .map_err(|error| format!("Could not stage the music file: {error}"))?;
        destination
            .sync_all()
            .map_err(|error| format!("Could not flush the staged music file: {error}"))?;
        drop(destination);

        let mut tagged = lofty::read_from_path(&staged).map_err(|error| {
            format!("This audio format cannot safely store embedded artwork: {error}")
        })?;
        let tag_type = tagged.primary_tag_type();
        if !tagged.tag_support(tag_type).is_writable() {
            return Err("This audio format does not support writing embedded artwork.".into());
        }
        if tagged.primary_tag().is_none() {
            tagged.insert_tag(Tag::new(tag_type));
        }
        let tag = tagged
            .primary_tag_mut()
            .ok_or_else(|| "This audio format has no writable artwork tag.".to_string())?;
        let mime_type = if mime == "image/png" { MimeType::Png } else { MimeType::Jpeg };
        let picture = Picture::unchecked(image.to_vec())
            .pic_type(PictureType::CoverFront)
            .mime_type(mime_type)
            .build();
        if let Some(index) = tag
            .pictures()
            .iter()
            .position(|picture| picture.pic_type() == PictureType::CoverFront)
        {
            tag.set_picture(index, picture);
        } else if !tag.pictures().is_empty() {
            tag.set_picture(0, picture);
        } else {
            tag.push_picture(picture);
        }
        tagged
            .save_to_path(&staged, WriteOptions::default())
            .map_err(|error| format!("Could not write artwork into this audio file: {error}"))?;
        let verified = lofty::read_from_path(&staged)
            .map_err(|error| format!("Could not verify the staged artwork: {error}"))?;
        if !verified
            .tags()
            .iter()
            .flat_map(|tag| tag.pictures())
            .any(|picture| picture.data() == image)
        {
            return Err("This audio format could not retain the selected artwork. The original file is unchanged.".into());
        }
        fs::set_permissions(&staged, original_permissions)
            .map_err(|error| format!("Could not preserve music file permissions: {error}"))?;
        File::open(&staged)
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("Could not flush the artwork update: {error}"))?;

        if stamp(path)? != original_stamp {
            return Err("The music file changed while artwork was being prepared. Nothing was replaced.".into());
        }
        fs::rename(&staged, path)
            .map_err(|error| format!("Could not atomically replace the music file: {error}"))?;
        if let Some(parent) = path.parent() {
            if let Ok(directory) = File::open(parent) {
                let _ = directory.sync_all();
            }
        }
        Ok(())
    })();

    if result.is_err() && created_stage {
        let _ = fs::remove_file(&staged);
    }
    result
}

#[cfg(target_os = "macos")]
#[derive(PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
    digest: [u8; 32],
}

#[cfg(target_os = "macos")]
fn stamp(path: &Path) -> Result<FileStamp, String> {
    use sha2::{Digest, Sha256};

    let mut file = File::open(path).map_err(|error| format!("Could not read the music file: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Could not inspect the music file: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not verify the music file: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(FileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        digest: hasher.finalize().into(),
    })
}

#[cfg(target_os = "macos")]
fn stage_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "The music file has no parent folder.".to_string())?;
    let extension = path
        .extension()
        .ok_or_else(|| "The music file has no supported extension.".to_string())?;
    let sequence = NEXT_ARTWORK_STAGE.fetch_add(1, Ordering::Relaxed);
    let mut staged_name = std::ffi::OsString::from(".");
    staged_name.push(format!("luno-artwork-{}-{sequence}.", std::process::id()));
    staged_name.push(extension);
    Ok(parent.join(staged_name))
}

/// Downscales an oversized JPEG/PNG to [`MAX_ARTWORK_DIM`] and re-encodes
/// as JPEG. Returns `None` when the image is already display-sized (serve
/// the original) or when decoding fails (serve the original) — the caller
/// only swaps bytes on `Some`.
#[cfg(target_os = "macos")]
fn downscaled_jpeg(data: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(data).ok()?;
    let (width, height) = (image.width(), image.height());
    if width == 0 || height == 0 || (width <= MAX_ARTWORK_DIM && height <= MAX_ARTWORK_DIM) {
        return None;
    }
    let small = image.thumbnail(MAX_ARTWORK_DIM, MAX_ARTWORK_DIM);
    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, DOWNSCALE_JPEG_QUALITY);
    encoder.encode_image(&small).ok()?;
    Some(out)
}

#[cfg(target_os = "macos")]
fn symphonia_artwork_data_url(path: &Path) -> Option<String> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let file = File::open(path).ok()?;
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(extension);
    }
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .ok()?;
    let metadata = format.metadata();
    let visual = metadata.current()?.media.visuals.first()?;
    let data: &[u8] = visual.data.as_ref();
    if data.is_empty() || data.len() > MAX_SINGLE_ARTWORK_BYTES {
        return None;
    }
    let mime = visual
        .media_type
        .as_deref()
        .filter(|mime| mime.starts_with("image/"))
        .unwrap_or_else(|| guess_mime(data));
    picture_data_url(mime, data)
}

#[cfg(not(target_os = "macos"))]
pub fn embedded_artwork_data_url(_path: &Path) -> Option<String> {
    None
}

/// Resolve artwork for a track through the process and persistent caches,
/// extracting on first cache miss. The track must be in the current snapshot.
pub struct TrackArtwork {
    pub cache: Mutex<ArtworkCache>,
    pub resolve: Box<dyn Fn(&str) -> Option<std::path::PathBuf> + Send + Sync>,
    revision: AtomicU64,
    disk_cache: Mutex<ArtworkDiskCache>,
}

#[derive(Default)]
struct ArtworkDiskCache {
    root: Option<PathBuf>,
    total_bytes: u64,
}

impl TrackArtwork {
    pub fn new(
        resolve: impl Fn(&str) -> Option<std::path::PathBuf> + Send + Sync + 'static,
    ) -> Self {
        Self {
            cache: Mutex::new(ArtworkCache::default()),
            resolve: Box::new(resolve),
            revision: AtomicU64::new(0),
            disk_cache: Mutex::new(ArtworkDiskCache::default()),
        }
    }

    /// Configure a bounded, versioned cache outside the user's music folder.
    pub fn set_cache_directory(&self, root: PathBuf) -> Result<(), String> {
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let total_bytes = fs::read_dir(&root)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                (entry.path().extension().and_then(|extension| extension.to_str()) == Some("cache"))
                    .then(|| entry.metadata().ok().map(|metadata| metadata.len()))
                    .flatten()
            })
            .sum();
        let mut cache = self.disk_cache.lock().unwrap_or_else(|error| error.into_inner());
        cache.root = Some(root);
        cache.total_bytes = total_bytes;
        if cache.total_bytes > DISK_ARTWORK_CACHE_BYTES {
            Self::prune_disk_cache(&mut cache);
        }
        Ok(())
    }

    fn cache_file(&self, source: &Path) -> Option<PathBuf> {
        let root = self.disk_cache.lock().ok()?.root.clone()?;
        let metadata = fs::metadata(source).ok()?;
        let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
        let mut hasher = Sha256::new();
        hasher.update(source.as_os_str().as_encoded_bytes());
        hasher.update(metadata.len().to_le_bytes());
        hasher.update(modified.to_le_bytes());
        Some(root.join(format!("{:x}.cache", hasher.finalize())))
    }

    fn read_cache_file(path: &Path) -> Option<Option<String>> {
        let value = fs::read_to_string(path).ok()?;
        if value == NO_ARTWORK_MARKER {
            return Some(None);
        }
        if value.starts_with("data:image/") && value.len() <= MAX_SINGLE_ARTWORK_BYTES * 2 {
            return Some(Some(value));
        }
        let _ = fs::remove_file(path);
        None
    }

    fn write_cache_file(&self, path: &Path, value: Option<&str>) {
        let Ok(mut cache) = self.disk_cache.lock() else { return };
        let Some(root) = cache.root.as_ref() else { return };
        if !path.starts_with(root) || path.exists() {
            return;
        }
        let bytes = value.unwrap_or(NO_ARTWORK_MARKER).as_bytes();
        let sequence = NEXT_ARTWORK_STAGE.fetch_add(1, Ordering::Relaxed);
        let temporary = root.join(format!(".luno-artwork-cache-{}-{sequence}.tmp", std::process::id()));
        if fs::write(&temporary, bytes).is_err() {
            let _ = fs::remove_file(&temporary);
            return;
        }
        if fs::rename(&temporary, path).is_err() {
            let _ = fs::remove_file(&temporary);
            return;
        }
        cache.total_bytes = cache.total_bytes.saturating_add(bytes.len() as u64);
        if cache.total_bytes > DISK_ARTWORK_CACHE_BYTES {
            Self::prune_disk_cache(&mut cache);
        }
    }

    fn prune_disk_cache(cache: &mut ArtworkDiskCache) {
        let Some(root) = cache.root.as_ref() else { return };
        let Ok(entries) = fs::read_dir(root) else { return };
        let mut files = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("cache") {
                    return None;
                }
                let metadata = entry.metadata().ok()?;
                Some((metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH), path, metadata.len()))
            })
            .collect::<Vec<_>>();
        files.sort_by_key(|(modified, _, _)| *modified);
        for (_, path, size) in files {
            if cache.total_bytes <= DISK_ARTWORK_CACHE_TARGET_BYTES {
                break;
            }
            if fs::remove_file(path).is_ok() {
                cache.total_bytes = cache.total_bytes.saturating_sub(size);
            }
        }
    }

    pub fn artwork(&self, track_id: &str) -> Option<String> {
        let revision = self.revision.load(Ordering::Acquire);
        let cached = self.cache.lock().ok()?.get(track_id);
        if let Some(url) = cached {
            return Some(url);
        }
        let path = (self.resolve)(track_id)?;
        if let Some(cache_file) = self.cache_file(&path) {
            if let Some(url) = Self::read_cache_file(&cache_file) {
                if self.revision.load(Ordering::Acquire) == revision {
                    if let Some(url) = &url {
                        if let Ok(mut cache) = self.cache.lock() {
                            cache.insert(track_id.to_string(), url.clone());
                        }
                    }
                }
                return url;
            }
            let url = embedded_artwork_data_url(&path);
            if self.revision.load(Ordering::Acquire) == revision {
                self.write_cache_file(&cache_file, url.as_deref());
            }
            if let Some(url) = &url {
                if self.revision.load(Ordering::Acquire) == revision {
                    if let Ok(mut cache) = self.cache.lock() {
                        cache.insert(track_id.to_string(), url.clone());
                    }
                }
            }
            return url;
        }
        let url = embedded_artwork_data_url(&path)?;
        if self.revision.load(Ordering::Acquire) == revision {
            if let Ok(mut cache) = self.cache.lock() {
                if self.revision.load(Ordering::Acquire) == revision {
                    cache.insert(track_id.to_string(), url.clone());
                }
            }
        }
        Some(url)
    }

    pub fn invalidate(&self, track_id: &str) {
        self.revision.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut cache) = self.cache.lock() {
            cache.invalidate(track_id);
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod artwork_tests {
    use super::*;
    use base64::Engine;
    use image::{ImageBuffer, Rgb};

    fn jpeg_bytes(width: u32, height: u32) -> Vec<u8> {
        let frame: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
            .encode_image(&frame)
            .expect("fixture encodes");
        out
    }

    fn served_dims(url: &str) -> (u32, u32) {
        let payload = url.split_once(",").expect("data URL").1;
        let raw = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .expect("valid base64");
        let image = image::load_from_memory(&raw).expect("served bytes decode");
        (image.width(), image.height())
    }

    #[test]
    fn oversized_covers_serve_at_display_size_as_jpeg() {
        let big = jpeg_bytes(1600, 1200);
        let url = picture_data_url("image/jpeg", &big).expect("large art serves");
        assert!(url.starts_with("data:image/jpeg;base64,"));
        let (width, height) = served_dims(&url);
        assert!(width <= MAX_ARTWORK_DIM && height <= MAX_ARTWORK_DIM, "{width}x{height}");
        assert_eq!((width, height), (512, 384));
        assert!(url.len() < base64::engine::general_purpose::STANDARD.encode(&big).len());
    }

    #[test]
    fn small_art_serves_byte_identical_without_reencode() {
        let small = jpeg_bytes(200, 100);
        let url = picture_data_url("image/jpeg", &small).expect("small art serves");
        let payload = url.split_once(",").expect("data URL").1;
        assert_eq!(
            payload,
            base64::engine::general_purpose::STANDARD.encode(&small),
            "display-sized art must not take a lossy round-trip"
        );
    }

    #[test]
    fn undecodable_bytes_pass_through_instead_of_breaking_the_row() {
        // Not decodable as JPEG/PNG: served as-is (existing behavior), since
        // the browser may still render it. Emptiness is guarded by the
        // container callers, not here.
        let junk = vec![0xFF, 0xD8, 0x00, 0x01, 0x02, 0x03];
        let url = picture_data_url("image/jpeg", &junk).expect("fallback serves");
        assert!(url.starts_with("data:image/jpeg;base64,"));
    }
}

#[cfg(test)]
mod artwork_disk_cache_tests {
    use super::*;

    fn test_root() -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("luno-artwork-cache-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn disk_cache_round_trips_art_and_definitive_no_art() {
        let root = test_root();
        let source = root.join("song.mp3");
        let cache_dir = root.join("cache");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, b"audio source").unwrap();
        let artwork = TrackArtwork::new({
            let source = source.clone();
            move |_| Some(source.clone())
        });
        artwork.set_cache_directory(cache_dir.clone()).unwrap();

        let art_file = artwork.cache_file(&source).unwrap();
        artwork.write_cache_file(&art_file, Some("data:image/jpeg;base64,Y292ZXI="));
        assert_eq!(TrackArtwork::read_cache_file(&art_file), Some(Some("data:image/jpeg;base64,Y292ZXI=".into())));

        let no_art_file = cache_dir.join("no-art.cache");
        artwork.write_cache_file(&no_art_file, None);
        assert_eq!(TrackArtwork::read_cache_file(&no_art_file), Some(None));

        fs::write(&source, b"audio source replaced").unwrap();
        assert_ne!(art_file, artwork.cache_file(&source).unwrap());
        let _ = fs::remove_dir_all(root);
    }
}
