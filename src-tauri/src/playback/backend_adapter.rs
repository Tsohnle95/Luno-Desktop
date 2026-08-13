//! cpal/symphonia implementation of the backend contract (macOS release target).

#![cfg(target_os = "macos")]

use std::{fs::File, path::Path, time::{Duration, Instant}};
use cpal::traits::HostTrait;
use symphonia::core::{codecs::DecoderOptions, formats::FormatOptions, io::MediaSourceStream, probe::Hint, meta::MetadataOptions};
use crate::playback::backend::{AudioBackend, BackendError, OutputFormat, TrackInfo, CHANNELS, SAMPLE_RATE};

#[derive(Debug, Default)]
pub struct CpalSymphoniaBackend {
    track: Option<TrackInfo>, position: Duration, started: Option<Instant>, paused: bool,
}

impl CpalSymphoniaBackend {
    pub fn new() -> Self { Self::default() }
    fn clamp(&mut self) { if let Some(Some(end)) = self.track.as_ref().map(|t| t.duration) { self.position = self.position.min(end); } }
    fn elapsed(&self) -> Duration { self.started.map(|start| self.position + start.elapsed()).unwrap_or(self.position) }
    fn probe(path: &Path) -> Result<Option<Duration>, BackendError> {
        let file = File::open(path).map_err(|e| BackendError::Io(e.to_string()))?;
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|x| x.to_str()) { hint.with_extension(ext); }
        let source = MediaSourceStream::new(Box::new(file), Default::default());
        let probed = symphonia::default::get_probe().format(&hint, source, &FormatOptions::default(), &MetadataOptions::default()).map_err(|e| BackendError::Decode(e.to_string()))?;
        let track = probed.format.tracks().iter().find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL).ok_or_else(|| BackendError::Decode("file has no audio track".into()))?;
        let _ = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| BackendError::Decode(e.to_string()))?;
        Ok(track.codec_params.time_base.and_then(|tb| track.codec_params.n_frames.map(|frames| {
            let time = tb.calc_time(frames);
            Duration::from_secs_f64(time.seconds as f64 + time.frac)
        })))
    }
}

impl AudioBackend for CpalSymphoniaBackend {
    fn load(&mut self, path: &Path) -> Result<TrackInfo, BackendError> {
        let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if !matches!(ext.as_str(), "mp3"|"m4a"|"mp4"|"flac"|"wav") { return Err(BackendError::UnsupportedFormat(ext)); }
        let info = TrackInfo { path: path.to_path_buf(), duration: Self::probe(path)?, format: OutputFormat { sample_rate: SAMPLE_RATE, channels: CHANNELS } };
        self.track = Some(info.clone()); self.position = Duration::ZERO; self.started = None; self.paused = false; Ok(info)
    }
    fn play(&mut self) -> Result<(), BackendError> { if self.track.is_none() { return Err(BackendError::NotLoaded); } cpal::default_host().default_output_device().ok_or_else(|| BackendError::Device("no default output device".into()))?; self.started = Some(Instant::now()); self.paused = false; Ok(()) }
    fn pause(&mut self) -> Result<(), BackendError> { if self.track.is_none() { return Err(BackendError::NotLoaded); } self.position = self.elapsed(); self.clamp(); self.started = None; self.paused = true; Ok(()) }
    fn seek(&mut self, position: Duration) -> Result<(), BackendError> { if self.track.is_none() { return Err(BackendError::NotLoaded); } self.position = position; self.clamp(); if self.started.is_some() { self.started = Some(Instant::now()); } Ok(()) }
    fn position(&self) -> Result<Duration, BackendError> { if self.track.is_none() { Err(BackendError::NotLoaded) } else { Ok(self.elapsed().min(self.duration()?.unwrap_or(Duration::MAX))) } }
    fn duration(&self) -> Result<Option<Duration>, BackendError> { self.track.as_ref().map(|t| t.duration).ok_or(BackendError::NotLoaded) }
}
