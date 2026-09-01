//! Audio backend contract.  The queue and transition policy deliberately live elsewhere.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const SAMPLE_RATE: u32 = 44_100;
pub const CHANNELS: u16 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self {
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackInfo {
    pub path: PathBuf,
    pub duration: Option<Duration>,
    pub format: OutputFormat,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum BackendError {
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),
    #[error("audio file error: {0}")]
    Io(String),
    #[error("audio decode error: {0}")]
    Decode(String),
    #[error("audio output device unavailable: {0}")]
    Device(String),
    #[error("no track is loaded")]
    NotLoaded,
    #[error("seek position is invalid: {0}")]
    InvalidSeek(f64),
    #[error("load cancelled by a newer playback request")]
    Cancelled,
}

pub trait AudioBackend {
    fn load(&mut self, path: &Path) -> Result<TrackInfo, BackendError>;
    /// Load a track for streaming playback: decode happens on a background
    /// thread that stays just ahead of the output, so a click starts audio in
    /// a fraction of a second instead of after a full-file decode. The default
    /// implementation falls back to (dual-purpose) `load`.
    fn load_streaming(&mut self, path: &Path) -> Result<TrackInfo, BackendError> {
        self.load(path)
    }
    /// Like `load_streaming`, but accepts a duration already known from scan
    /// metadata. Streaming backends should let their producer discover an
    /// unknown duration rather than synchronously opening the file twice.
    fn load_streaming_with_duration(&mut self, path: &Path, known_duration: Option<Duration>) -> Result<TrackInfo, BackendError> {
        let _ = known_duration;
        self.load_streaming(path)
    }
    /// Master volume in `0.0..=1.0`, applied to the samples the backend emits.
    fn set_volume(&mut self, _volume: f32) {}
    fn play(&mut self) -> Result<(), BackendError>;
    fn pause(&mut self) -> Result<(), BackendError>;
    fn seek(&mut self, position: Duration) -> Result<(), BackendError>;
    /// Position is the backend's authoritative elapsed audio position.
    fn position(&self) -> Result<Duration, BackendError>;
    fn duration(&self) -> Result<Option<Duration>, BackendError>;
    fn stop(&mut self) {}
    fn shutdown(&mut self) {}
}

#[derive(Debug, Default)]
pub struct FakeBackend {
    track: Option<TrackInfo>,
    configured_duration: Option<Duration>,
    position: Duration,
    started: Option<Instant>,
    paused: bool,
}

impl FakeBackend {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_track(path: impl Into<PathBuf>, duration: Option<Duration>) -> Self {
        let mut backend = Self::new();
        let path = path.into();
        backend.track = Some(TrackInfo {
            path,
            duration,
            format: OutputFormat::default(),
        });
        backend.configured_duration = duration;
        backend
    }
    /// Advances the fake clock without sleeping, for deterministic tests.
    pub fn advance(&mut self, amount: Duration) {
        if self.started.is_some() && !self.paused {
            self.position += amount;
            self.clamp_position();
        }
    }
    pub fn is_playing(&self) -> bool {
        self.started.is_some() && !self.paused
    }
    fn clamp_position(&mut self) {
        if let Some(Some(end)) = self.track.as_ref().map(|t| t.duration) {
            self.position = self.position.min(end);
        }
    }
}

impl AudioBackend for FakeBackend {
    fn load(&mut self, path: &Path) -> Result<TrackInfo, BackendError> {
        if path
            .extension()
            .and_then(|x| x.to_str())
            .map(|x| {
                matches!(
                    x.to_ascii_lowercase().as_str(),
                    "mp3" | "m4a" | "mp4" | "flac" | "wav"
                )
            })
            .unwrap_or(false)
            == false
        {
            return Err(BackendError::UnsupportedFormat(path.display().to_string()));
        }
        if !path.is_file() {
            return Err(BackendError::Io(format!(
                "{}: file not found",
                path.display()
            )));
        }
        let duration = self
            .track
            .as_ref()
            .filter(|track| track.path == path)
            .and_then(|track| track.duration)
            .or(self.configured_duration);
        let info = TrackInfo {
            path: path.to_path_buf(),
            duration,
            format: OutputFormat::default(),
        };
        self.track = Some(info.clone());
        self.position = Duration::ZERO;
        self.started = None;
        self.paused = false;
        Ok(info)
    }
    fn play(&mut self) -> Result<(), BackendError> {
        if self.track.is_none() {
            return Err(BackendError::NotLoaded);
        }
        self.started = Some(Instant::now());
        self.paused = false;
        Ok(())
    }
    fn pause(&mut self) -> Result<(), BackendError> {
        if self.track.is_none() {
            return Err(BackendError::NotLoaded);
        }
        self.paused = true;
        self.started = None;
        Ok(())
    }
    fn seek(&mut self, position: Duration) -> Result<(), BackendError> {
        if self.track.is_none() {
            return Err(BackendError::NotLoaded);
        }
        self.position = position;
        self.clamp_position();
        Ok(())
    }
    fn position(&self) -> Result<Duration, BackendError> {
        if self.track.is_none() {
            Err(BackendError::NotLoaded)
        } else {
            Ok(self.position)
        }
    }
    fn duration(&self) -> Result<Option<Duration>, BackendError> {
        self.track
            .as_ref()
            .map(|t| t.duration)
            .ok_or(BackendError::NotLoaded)
    }
}
