use std::time::Duration;
use luno_desktop_lib::playback::backend::{AudioBackend, BackendError, FakeBackend, OutputFormat, CHANNELS, SAMPLE_RATE};

#[test]
fn fake_clock_pause_seek_and_output_contract_are_deterministic() {
    let mut b = FakeBackend::with_track("song.mp3", Some(Duration::from_secs(10)));
    assert_eq!(OutputFormat::default(), OutputFormat { sample_rate: SAMPLE_RATE, channels: CHANNELS });
    b.play().unwrap(); b.advance(Duration::from_secs(3)); assert_eq!(b.position().unwrap(), Duration::from_secs(3));
    b.pause().unwrap(); b.advance(Duration::from_secs(4)); assert_eq!(b.position().unwrap(), Duration::from_secs(3));
    b.seek(Duration::from_secs(8)).unwrap(); b.play().unwrap(); b.advance(Duration::from_secs(5)); assert_eq!(b.position().unwrap(), Duration::from_secs(10));
}

#[test]
fn fake_rejects_unsupported_format_and_missing_track() {
    let mut b = FakeBackend::new();
    assert_eq!(b.play(), Err(BackendError::NotLoaded));
    assert!(matches!(b.load(std::path::Path::new("song.ogg")), Err(BackendError::UnsupportedFormat(_))));
}
