//! macOS media-session adapter boundary.
//!
//! Apple's MPRemoteCommandCenter and Now Playing APIs are intentionally kept
//! behind `MediaSessionRegistrar`.  This keeps the playback contract testable
//! without an Apple framework and gives the application one teardown point.

use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaCommand { Play, Pause, Toggle, Next, Previous, Stop }

pub fn xf86_command(key: &str) -> Option<MediaCommand> {
    match key { "XF86AudioPlay" => Some(MediaCommand::Toggle), "XF86AudioNext" => Some(MediaCommand::Next), "XF86AudioPrev" => Some(MediaCommand::Previous), _ => None }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: f64,
    pub elapsed_secs: f64,
    pub playing: bool,
}

pub type Handler = Arc<dyn Fn(MediaCommand) + Send + Sync + 'static>;

pub trait MediaSessionRegistrar: Send {
    fn register(&mut self, handler: Handler) -> Result<(), String>;
    fn unregister(&mut self);
    fn publish(&mut self, info: &NowPlaying) -> Result<(), String>;
    fn clear(&mut self);
}

#[derive(Default)]
struct SessionState { ready: bool, closing: bool, registered: bool, handler: Option<Handler>, queued: Vec<MediaCommand>, now_playing: Option<NowPlaying> }

pub struct MediaSession<R: MediaSessionRegistrar> { registrar: R, state: Arc<Mutex<SessionState>> }

impl<R: MediaSessionRegistrar> MediaSession<R> {
    pub fn new(registrar: R) -> Self { Self { registrar, state: Arc::new(Mutex::new(SessionState::default())) } }

    pub fn set_ready(&self, ready: bool) { if let Ok(mut state) = self.state.lock() { state.ready = ready; } }
    pub fn is_registered(&self) -> bool { self.state.lock().map(|state| state.registered).unwrap_or(false) }

    /// Register exactly once.  The closure is retained by this object for as
    /// long as the native command center may call it.
    pub fn register(&mut self) -> Result<bool, String> {
        let mut state = self.state.lock().map_err(|_| "media session state poisoned".to_string())?;
        if !state.ready || state.closing { return Ok(false); }
        if state.registered { return Ok(false); }
        let weak = Arc::downgrade(&self.state);
        let handler: Handler = Arc::new(move |command| { if let Some(state) = weak.upgrade() { if let Ok(mut state) = state.lock() { if state.ready && !state.closing { state.queued.push(command); } } } });
        self.registrar.register(handler.clone())?;
        state.handler = Some(handler);
        state.registered = true;
        Ok(true)
    }

    pub fn drain_commands(&self) -> Vec<MediaCommand> { self.state.lock().map(|mut state| std::mem::take(&mut state.queued)).unwrap_or_default() }

    pub fn publish(&mut self, info: NowPlaying) -> Result<bool, String> {
        let mut state = self.state.lock().map_err(|_| "media session state poisoned".to_string())?;
        if !state.ready || state.closing || !state.registered { return Ok(false); }
        self.registrar.publish(&info)?;
        state.now_playing = Some(info);
        Ok(true)
    }

    pub fn now_playing(&self) -> Option<NowPlaying> { self.state.lock().ok().and_then(|state| state.now_playing.clone()) }

    pub fn shutdown(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            if state.closing { return; }
            state.closing = true;
            state.ready = false;
            state.queued.clear();
            if state.registered { self.registrar.unregister(); self.registrar.clear(); }
            state.registered = false;
            state.handler = None;
            state.now_playing = None;
        }
    }
}

impl<R: MediaSessionRegistrar> Drop for MediaSession<R> { fn drop(&mut self) { self.shutdown(); } }
