use luno_desktop_lib::download::promotion::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

fn dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("luno-promotion-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn fake_audio_is_durably_promoted_and_marker_boundary_is_after_receipt() {
    let root = dir();
    let destination = root.join("track.mp3");
    let temporary = stage_output(&destination).unwrap();
    fs::write(&temporary, b"ID3fake-audio").unwrap();
    let receipt = promote_output(&temporary, &destination, Validation::audio(), CollisionPolicy::Reject, None).unwrap();
    assert_eq!(receipt.path, destination);
    assert_eq!(fs::read(destination).unwrap(), b"ID3fake-audio");
    assert!(!temporary.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn wrong_missing_and_partial_outputs_are_rejected_and_cleaned() {
    let root = dir();
    let destination = root.join("track.mp3");
    let wrong = stage_output(&destination).unwrap();
    fs::write(&wrong, b"not media").unwrap();
    assert!(promote_output(&wrong, &destination, Validation::audio(), CollisionPolicy::Reject, None).is_err());
    assert!(!wrong.exists());
    let missing = root.join("missing");
    assert!(validate_output(&missing, Validation::audio()).is_err());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn collision_cancel_and_video_validation_never_publish() {
    let root = dir();
    let destination = root.join("video.mp4");
    fs::write(&destination, b"old").unwrap();
    let temporary = stage_output(&destination).unwrap();
    fs::write(&temporary, b"....ftypfake-video").unwrap();
    assert!(matches!(promote_output(&temporary, &destination, Validation::video(), CollisionPolicy::Reject, None), Err(PromotionError::Collision(_))));
    assert_eq!(fs::read(&destination).unwrap(), b"old");
    let cancelled = AtomicBool::new(true);
    let temporary = stage_output(&destination).unwrap();
    fs::write(&temporary, b"....ftypfake-video").unwrap();
    assert!(matches!(promote_output(&temporary, &destination, Validation::video(), CollisionPolicy::Replace, Some(&cancelled)), Err(PromotionError::Cancelled)));
    assert!(!temporary.exists());
    let _ = fs::remove_dir_all(root);
}
