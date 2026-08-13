//! Audio-clock authoritative lifecycle state for the HTML video renderer.
use std::time::Duration;

pub const SYNC_TOLERANCE: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoGeneration(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoSize { pub width: u32, pub height: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoStatus { Idle, Loading, Playing, Paused, Ended, Fallback }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoEvent {
    StateChanged(VideoStatus),
    Restarted { generation: VideoGeneration, position: Duration },
    Resize(VideoSize),
    Ended,
    Fallback,
    FullscreenChanged(bool),
}

pub trait Decoder {
    fn start(&mut self, position: Duration, size: VideoSize) -> Result<(), String>;
    fn stop(&mut self);
    fn clear_buffer(&mut self);
}

pub trait DecoderFactory {
    type Decoder: Decoder;
    fn create(&mut self, generation: VideoGeneration) -> Result<Self::Decoder, String>;
}

pub struct VideoPlayer<F: DecoderFactory> {
    factory: F,
    decoder: Option<F::Decoder>,
    generation: VideoGeneration,
    status: VideoStatus,
    size: VideoSize,
    fullscreen: bool,
    last_audio_position: Duration,
    events: Vec<VideoEvent>,
}

impl<F: DecoderFactory> VideoPlayer<F> {
    pub fn new(factory: F, size: VideoSize) -> Self {
        Self { factory, decoder: None, generation: VideoGeneration(0), status: VideoStatus::Idle,
            size, fullscreen: false, last_audio_position: Duration::ZERO, events: Vec::new() }
    }

    pub fn status(&self) -> VideoStatus { self.status }
    pub fn generation(&self) -> VideoGeneration { self.generation }
    pub fn size(&self) -> VideoSize { self.size }
    pub fn is_fullscreen(&self) -> bool { self.fullscreen }
    pub fn take_events(&mut self) -> Vec<VideoEvent> { std::mem::take(&mut self.events) }

    pub fn load(&mut self, position: Duration) -> VideoGeneration {
        self.restart(position);
        self.status = VideoStatus::Loading;
        self.events.push(VideoEvent::StateChanged(self.status));
        self.generation
    }

    pub fn start(&mut self, generation: VideoGeneration, position: Duration) -> bool {
        if generation != self.generation { return false; }
        let Some(decoder) = self.decoder.as_mut() else { self.fallback(); return false; };
        match decoder.start(position, self.size) {
            Ok(()) => { self.last_audio_position = position; self.status = VideoStatus::Playing; self.events.push(VideoEvent::StateChanged(self.status)); true }
            Err(_) => { self.fallback(); false }
        }
    }

    pub fn pause(&mut self, audio_position: Duration) {
        self.last_audio_position = audio_position;
        if self.status == VideoStatus::Playing { self.stop_decoder(); self.status = VideoStatus::Paused; self.events.push(VideoEvent::StateChanged(self.status)); }
    }

    pub fn resume(&mut self, audio_position: Duration) {
        if self.status == VideoStatus::Paused { self.restart(audio_position); let generation = self.generation; let _ = self.start(generation, audio_position); }
    }

    pub fn sync(&mut self, audio_position: Duration) {
        if self.status != VideoStatus::Playing { self.last_audio_position = audio_position; return; }
        if audio_position.abs_diff(self.last_audio_position) > SYNC_TOLERANCE { self.restart(audio_position); let generation = self.generation; let _ = self.start(generation, audio_position); }
        self.last_audio_position = audio_position;
    }

    pub fn seek(&mut self, audio_position: Duration) {
        self.restart(audio_position);
        if self.status != VideoStatus::Fallback { let generation = self.generation; let _ = self.start(generation, audio_position); }
    }

    pub fn resize(&mut self, size: VideoSize, audio_position: Duration) {
        if size == self.size { return; }
        self.size = size;
        self.events.push(VideoEvent::Resize(size));
        if matches!(self.status, VideoStatus::Playing | VideoStatus::Paused | VideoStatus::Loading) {
            let was_playing = self.status == VideoStatus::Playing;
            self.restart(audio_position);
            if was_playing { let generation = self.generation; let _ = self.start(generation, audio_position); }
        }
    }

    pub fn decoder_eof(&mut self) {
        if self.status == VideoStatus::Fallback { return; }
        self.stop_decoder(); self.status = VideoStatus::Ended;
        self.events.push(VideoEvent::Ended); self.events.push(VideoEvent::StateChanged(self.status));
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        if self.fullscreen != fullscreen { self.fullscreen = fullscreen; self.events.push(VideoEvent::FullscreenChanged(fullscreen)); }
    }

    pub fn shutdown(&mut self) { self.stop_decoder(); self.status = VideoStatus::Idle; self.fullscreen = false; }

    fn restart(&mut self, position: Duration) {
        self.stop_decoder();
        self.generation.0 = self.generation.0.saturating_add(1);
        self.last_audio_position = position;
        if self.decoder.is_none() { match self.factory.create(self.generation) { Ok(decoder) => self.decoder = Some(decoder), Err(_) => { self.fallback(); return; } } }
        if let Some(decoder) = self.decoder.as_mut() { decoder.clear_buffer(); }
        self.events.push(VideoEvent::Restarted { generation: self.generation, position });
    }

    fn stop_decoder(&mut self) { if let Some(decoder) = self.decoder.as_mut() { decoder.stop(); } }
    fn fallback(&mut self) { self.stop_decoder(); self.decoder = None; self.status = VideoStatus::Fallback; self.events.push(VideoEvent::Fallback); self.events.push(VideoEvent::StateChanged(self.status)); }
}

impl<F: DecoderFactory> Drop for VideoPlayer<F> { fn drop(&mut self) { self.stop_decoder(); } }
