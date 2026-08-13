//! Validated media output promotion.
//!
//! Downloaders write to a sibling temporary file.  A file is visible at its
//! library path only after validation and a durable atomic rename.  Marker
//! files are deliberately outside this module: a successful `PromotionReceipt`
//! is the publication boundary at which a caller may record a marker.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind { Audio, Video }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionPolicy { Reject, Replace }

#[derive(Debug, Clone, Copy)]
pub struct Validation { pub kind: MediaKind, pub min_bytes: u64 }

impl Validation {
    pub fn audio() -> Self { Self { kind: MediaKind::Audio, min_bytes: 1 } }
    pub fn video() -> Self { Self { kind: MediaKind::Video, min_bytes: 1 } }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionReceipt { pub path: PathBuf, pub bytes: u64 }

#[derive(Debug)]
pub enum PromotionError {
    Io(io::Error),
    Collision(PathBuf),
    Cancelled,
    InvalidOutput(&'static str),
}

impl From<io::Error> for PromotionError { fn from(error: io::Error) -> Self { Self::Io(error) } }

/// Create a same-directory output name for a downloader or FFmpeg process.
/// Keeping it beside the destination makes rename atomic on every supported
/// filesystem and prevents partial output from entering a library scan.
pub fn stage_output(destination: impl AsRef<Path>) -> io::Result<PathBuf> {
    let destination = destination.as_ref();
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    for attempt in 0..100 {
        let path = PathBuf::from(format!("{}.luno-partial-{}-{}", destination.display(), std::process::id(), stamp + attempt));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "could not allocate temporary media output"))
}

pub fn validate_output(path: impl AsRef<Path>, validation: Validation) -> Result<u64, PromotionError> {
    let path = path.as_ref();
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() { return Err(PromotionError::InvalidOutput("output is not a regular file")); }
    if metadata.len() < validation.min_bytes { return Err(PromotionError::InvalidOutput("output is empty or partial")); }
    let mut file = File::open(path)?;
    let mut head = [0u8; 12];
    let count = file.read(&mut head)?;
    if !valid_signature(validation.kind, &head[..count]) {
        return Err(PromotionError::InvalidOutput("output is not a recognized media file"));
    }
    Ok(metadata.len())
}

/// Validate and atomically publish a completed output. The receipt is the only
/// success signal; callers must not publish download markers before receiving it.
pub fn promote_output(
    temporary: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    validation: Validation,
    collision: CollisionPolicy,
    cancelled: Option<&AtomicBool>,
) -> Result<PromotionReceipt, PromotionError> {
    let temporary = temporary.as_ref();
    let destination = destination.as_ref();
    let result = (|| {
        check_cancel(cancelled)?;
        let bytes = validate_output(temporary, validation)?;
        check_cancel(cancelled)?;
        if destination.exists() && collision == CollisionPolicy::Reject {
            return Err(PromotionError::Collision(destination.to_path_buf()));
        }
        let file = OpenOptions::new().read(true).write(true).open(temporary)?;
        file.sync_all()?;
        fs::rename(temporary, destination)?;
        sync_directory(destination.parent().unwrap_or_else(|| Path::new(".")))?;
        Ok(PromotionReceipt { path: destination.to_path_buf(), bytes })
    })();
    if result.is_err() { let _ = fs::remove_file(temporary); }
    result
}

pub fn cancel_and_cleanup(path: impl AsRef<Path>) -> io::Result<()> {
    match fs::remove_file(path) { Ok(()) => Ok(()), Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()), Err(error) => Err(error) }
}

fn check_cancel(cancelled: Option<&AtomicBool>) -> Result<(), PromotionError> {
    if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) { Err(PromotionError::Cancelled) } else { Ok(()) }
}

fn valid_signature(kind: MediaKind, bytes: &[u8]) -> bool {
    match kind {
        MediaKind::Audio => bytes.starts_with(b"ID3") || bytes.starts_with(b"fLaC") || bytes.starts_with(b"OggS") || bytes.starts_with(b"RIFF") || looks_like_mp3(bytes),
        MediaKind::Video => bytes.starts_with(b"\x1a\x45\xdf\xa3") || bytes.starts_with(b"OggS") || (bytes.len() >= 8 && &bytes[4..8] == b"ftyp"),
    }
}

fn looks_like_mp3(bytes: &[u8]) -> bool { bytes.len() >= 2 && bytes[0] == 0xff && (bytes[1] & 0xe0) == 0xe0 }

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)] { File::open(path)?.sync_all()?; }
    Ok(())
}
