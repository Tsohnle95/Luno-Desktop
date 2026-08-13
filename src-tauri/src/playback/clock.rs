//! Playback timing policy independent of the audio backend or UI.
use std::time::Duration;

pub const HEARTBEAT: Duration = Duration::from_millis(400);
pub const STARTUP_GRACE: Duration = Duration::from_millis(500);

/// The UI heartbeat must not create late callbacks during shutdown.
pub fn heartbeat_should_continue(shutting_down: bool) -> bool { !shutting_down }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClockSource { Backend, Monotonic }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockReading { pub position: Duration, pub source: ClockSource }

#[derive(Debug, Clone, Copy)]
pub struct PlaybackClock {
    duration: Option<Duration>,
    base_position: Duration,
    base_monotonic: f64,
    paused: bool,
    playing: bool,
    completion_dispatched: bool,
}

impl PlaybackClock {
    pub fn new(duration: Option<Duration>) -> Self {
        Self { duration, base_position: Duration::ZERO, base_monotonic: 0.0,
            paused: false, playing: false, completion_dispatched: false }
    }

    pub fn duration(&self) -> Option<Duration> { self.duration }
    pub fn start(&mut self, monotonic: f64) {
        self.base_position = Duration::ZERO; self.base_monotonic = monotonic;
        self.paused = false; self.playing = true; self.completion_dispatched = false;
    }
    pub fn stop(&mut self) { self.playing = false; self.paused = false; self.base_position = Duration::ZERO; }
    pub fn pause(&mut self, monotonic: f64, backend_position: Option<Duration>) {
        if self.playing && !self.paused { self.base_position = self.position(monotonic, true, backend_position).position; }
        self.paused = true;
    }
    pub fn resume(&mut self, monotonic: f64) { if self.playing { self.base_monotonic = monotonic; self.paused = false; } }
    pub fn seek(&mut self, position: Duration, monotonic: f64) {
        self.base_position = self.clamp(position); self.base_monotonic = monotonic;
    }
    pub fn position(&self, monotonic: f64, backend_busy: bool, backend_position: Option<Duration>) -> ClockReading {
        let (value, source) = if self.paused { (self.base_position, ClockSource::Monotonic) }
            else if backend_busy { if let Some(pos) = backend_position { (pos, ClockSource::Backend) } else { (self.fallback(monotonic), ClockSource::Monotonic) } }
            else { (self.fallback(monotonic), ClockSource::Monotonic) };
        ClockReading { position: self.clamp(value), source }
    }
    fn fallback(&self, monotonic: f64) -> Duration { if !self.playing { self.base_position } else { self.base_position + Duration::from_secs_f64((monotonic - self.base_monotonic).max(0.0)) } }
    fn clamp(&self, position: Duration) -> Duration { self.duration.map_or(position, |d| position.min(d)) }
    pub fn completion_due(&self, elapsed: Duration, backend_busy: bool, monotonic_since_start: Duration) -> bool {
        !self.completion_dispatched && self.playing && !self.paused &&
            (self.duration.is_some_and(|d| d > Duration::ZERO && elapsed >= d) ||
             (!backend_busy && monotonic_since_start > STARTUP_GRACE))
    }
    pub fn dispatch_completion(&mut self) -> bool { if self.completion_dispatched { false } else { self.completion_dispatched = true; true } }
    pub fn completion_dispatched(&self) -> bool { self.completion_dispatched }
}
