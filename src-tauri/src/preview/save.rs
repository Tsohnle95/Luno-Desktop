//! Explicit promotion of a staged recommendation into a physical playlist.

use std::fs;
use std::path::{Path, PathBuf};

use crate::download::promotion::{promote_output, CollisionPolicy, PromotionError, Validation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistDestination { pub playlist_id: String, pub directory: PathBuf }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReceipt { pub playlist_id: String, pub path: PathBuf, pub already_saved: bool }

#[derive(Debug)]
pub enum SaveError { MissingDestination, InvalidDestination, Promotion(PromotionError) }
impl From<PromotionError> for SaveError { fn from(e: PromotionError) -> Self { Self::Promotion(e) } }

/// Promotes a staged file exactly once per destination. The staged file is
/// retained so the temporary queue remains playable after saving.
pub fn promote_to_playlist(staged: impl AsRef<Path>, filename: &str,
                           destination: Option<&PlaylistDestination>) -> Result<SaveReceipt, SaveError> {
    let destination = destination.ok_or(SaveError::MissingDestination)?;
    if destination.playlist_id.trim().is_empty() || destination.directory.as_os_str().is_empty() {
        return Err(SaveError::InvalidDestination);
    }
    fs::create_dir_all(&destination.directory).map_err(|e| SaveError::Promotion(PromotionError::Io(e)))?;
    let target = destination.directory.join(filename);
    if target.exists() {
        return Ok(SaveReceipt { playlist_id: destination.playlist_id.clone(), path: target, already_saved: true });
    }
    // Promotion consumes a temporary output, so copy first and let U30 own
    // validation, fsync, atomic rename, and partial cleanup.
    let partial = crate::download::promotion::stage_output(&target)
        .map_err(PromotionError::Io)?;
    if let Err(error) = fs::copy(staged.as_ref(), &partial) {
        let _ = fs::remove_file(&partial);
        return Err(SaveError::Promotion(PromotionError::Io(error)));
    }
    let receipt = promote_output(&partial, &target, Validation::audio(), CollisionPolicy::Reject)?;
    Ok(SaveReceipt { playlist_id: destination.playlist_id.clone(), path: receipt.path, already_saved: false })
}
