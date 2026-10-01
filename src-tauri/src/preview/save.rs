//! Explicit promotion of a staged recommendation into a physical playlist.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::download::promotion::{promote_output, CollisionPolicy, PromotionError, Validation};

pub const REMUX_DEADLINE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistDestination {
    pub playlist_id: String,
    pub directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReceipt {
    pub playlist_id: String,
    pub path: PathBuf,
    pub already_saved: bool,
}

#[derive(Debug)]
pub enum SaveError {
    MissingDestination,
    InvalidDestination,
    Promotion(PromotionError),
    Remux(String),
}
impl From<PromotionError> for SaveError {
    fn from(e: PromotionError) -> Self {
        Self::Promotion(e)
    }
}

/// Resolves the target file for a promotion, validating the destination and
/// the requested filename identically for the copy and remux paths.
fn destination_target(
    destination: Option<&PlaylistDestination>,
    filename: &str,
) -> Result<(PlaylistDestination, PathBuf), SaveError> {
    let destination = destination.ok_or(SaveError::MissingDestination)?;
    if destination.playlist_id.trim().is_empty() || destination.directory.as_os_str().is_empty() {
        return Err(SaveError::InvalidDestination);
    }
    let filename_path = Path::new(filename);
    if filename.is_empty()
        || filename == "."
        || filename == ".."
        || filename_path.file_name().and_then(|name| name.to_str()) != Some(filename)
    {
        return Err(SaveError::InvalidDestination);
    }
    let directory = fs::symlink_metadata(&destination.directory).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            SaveError::InvalidDestination
        } else {
            SaveError::Promotion(PromotionError::Io(error))
        }
    })?;
    if directory.file_type().is_symlink() || !directory.is_dir() {
        return Err(SaveError::InvalidDestination);
    }
    let target = destination.directory.join(filename);
    if target.exists() {
        crate::download::promotion::validate_output(&target, Validation::audio())?;
        return Ok((destination.clone(), target));
    }
    Ok((destination.clone(), target))
}

fn promote_staged(staged: &Path, target: &Path, destination: &PlaylistDestination) -> Result<SaveReceipt, SaveError> {
    // Promotion consumes a temporary output, so copy first and let U30 own
    // validation, fsync, atomic rename, and partial cleanup.
    let partial = crate::download::promotion::stage_output(target).map_err(PromotionError::Io)?;
    if let Err(error) = fs::copy(staged, &partial) {
        let _ = fs::remove_file(&partial);
        return Err(SaveError::Promotion(PromotionError::Io(error)));
    }
    let receipt = promote_output(&partial, target, Validation::audio(), CollisionPolicy::Reject, None)?;
    Ok(SaveReceipt {
        playlist_id: destination.playlist_id.clone(),
        path: receipt.path,
        already_saved: false,
    })
}

/// Remuxes a staged ADTS preview into a scannable `.m4a` with the bundled
/// ffmpeg (`-c:a copy`; no re-encode), then promotes it once per destination.
pub fn remux_and_promote(
    staged: impl AsRef<Path>,
    filename: &str,
    destination: Option<&PlaylistDestination>,
    ffmpeg: Option<&Path>,
) -> Result<SaveReceipt, SaveError> {
    // Keep destination checks and publication serialized with deletion so a
    // playlist cannot disappear mid-save and receive a stale promotion.
    let _mutation_guard = crate::library::mutations::lock_library_file_mutations();
    let (destination, target) = destination_target(destination, filename)?;
    if target.exists() {
        return Ok(SaveReceipt {
            playlist_id: destination.playlist_id.clone(),
            path: target,
            already_saved: true,
        });
    }
    let staged = staged.as_ref();
    let partial = crate::download::promotion::stage_output(&target).map_err(PromotionError::Io)?;
    let result = match ffmpeg {
        Some(ffmpeg) => remux(ffmpeg, staged, &partial),
        None => fs::copy(staged, &partial).map(|_| ()).map_err(|e| SaveError::Promotion(PromotionError::Io(e))),
    };
    if let Err(error) = result {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    let receipt = promote_output(&partial, &target, Validation::audio(), CollisionPolicy::Reject, None)?;
    Ok(SaveReceipt {
        playlist_id: destination.playlist_id.clone(),
        path: receipt.path,
        already_saved: false,
    })
}

/// Promotes a staged file exactly once per destination. The staged file is
/// retained so the temporary queue remains playable after saving.
pub fn promote_to_playlist(
    staged: impl AsRef<Path>,
    filename: &str,
    destination: Option<&PlaylistDestination>,
) -> Result<SaveReceipt, SaveError> {
    let _mutation_guard = crate::library::mutations::lock_library_file_mutations();
    let (destination, target) = destination_target(destination, filename)?;
    if target.exists() {
        return Ok(SaveReceipt {
            playlist_id: destination.playlist_id.clone(),
            path: target,
            already_saved: true,
        });
    }
    promote_staged(staged.as_ref(), &target, &destination)
}

/// Runs the bundled ffmpeg conversion into the staged partial with a hard
/// deadline. The fixture ffmpeg reads its input from positional argument 3,
/// matching the `-y -i INPUT ... OUTPUT` shape used here.
fn remux(ffmpeg: &Path, input: &Path, output: &Path) -> Result<(), SaveError> {
    let mut child = Command::new(ffmpeg)
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-c:a")
        .arg("copy")
        .arg("-f")
        .arg("mp4")
        .arg(output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| SaveError::Remux(format!("ffmpeg could not start: {e}")))?;
    let deadline = Instant::now() + REMUX_DEADLINE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(SaveError::Remux(format!(
                    "ffmpeg remux failed with status {status}"
                )))
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    while child.try_wait().is_ok_and(|s| s.is_none()) {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    let _ = child.try_wait();
                    return Err(SaveError::Remux("ffmpeg remux timed out".into()));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(SaveError::Remux(format!("ffmpeg remux failed: {error}"))),
        }
    }
}
