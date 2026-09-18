//! macOS media-key integration (MPRemoteCommandCenter).
//!
//! Registering targets on the SHARED remote-command center — plus publishing
//! a now-playing info dictionary — makes macOS route hardware play/pause/
//! next/previous keys to Luno instead of whatever app last claimed them
//! (Apple Music). The old Tk desktop app did this through PyObjC; the Tauri
//! port never reimplemented it, which is why the keys "went back" to Music.
//!
//! The layer is deliberately thin: it translates remote events into
//! [`MediaKey`] values and hands them to a caller-supplied sink. Status-aware
//! transport mapping (toggle semantics, disabled setting) lives in
//! `AppState::handle_media_key`.

use std::sync::Arc;

use objc2::class;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::NSString;
use objc2_media_player::{MPNowPlayingInfoCenter, MPNowPlayingPlaybackState, MPRemoteCommandCenter, MPRemoteCommandHandlerStatus, MPRemoteCommandEvent};

/// The subset of transport actions hardware keys can request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKey {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
}

/// Truthful Now Playing metadata published to `MPNowPlayingInfoCenter`.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: f64,
    pub elapsed_secs: f64,
    pub playing: bool,
}

/// Remote-command targets kept alive for the app's lifetime.
pub struct MediaKeys {
    _tokens: Vec<Retained<AnyObject>>,
}

/// Playing advertises an active now-playing session; every other state stays
/// `Paused` (not `Stopped`) so Luno keeps owning the hardware keys while idle
/// instead of handing them back to Apple Music.
pub fn playback_state_for(playing: bool) -> MPNowPlayingPlaybackState {
    if playing { MPNowPlayingPlaybackState::Playing } else { MPNowPlayingPlaybackState::Paused }
}

unsafe fn apply_now_playing(info: &NowPlaying) {
    let center = MPNowPlayingInfoCenter::defaultCenter();
    let dict: *mut AnyObject = msg_send![class!(NSMutableDictionary), dictionary];
    let set_string = |key: &str, value: &str| {
        if value.is_empty() { return; }
        let key = NSString::from_str(key);
        let value = NSString::from_str(value);
        let _: () = msg_send![dict, setObject: &*value, forKey: &*key];
    };
    let set_number = |key: &str, value: f64| {
        let key = NSString::from_str(key);
        let number: *mut AnyObject = msg_send![class!(NSNumber), numberWithDouble: value];
        let _: () = msg_send![dict, setObject: number, forKey: &*key];
    };
    set_string("title", &info.title);
    set_string("artist", &info.artist);
    set_string("album", &info.album);
    set_number("MPNowPlayingInfoPropertyPlaybackRate", if info.playing { 1.0 } else { 0.0 });
    set_number("MPNowPlayingInfoPropertyElapsedPlaybackTime", info.elapsed_secs.max(0.0));
    if info.duration_secs.is_finite() && info.duration_secs > 0.0 {
        set_number("MPMediaItemPropertyPlaybackDuration", info.duration_secs);
    }
    let _: () = msg_send![&center, setNowPlayingInfo: dict];
    center.setPlaybackState(playback_state_for(info.playing));
}

/// Publish truthful now-playing metadata + playback state. `playbackState` is
/// what modern macOS consults when deciding which process owns the hardware
/// media keys; a one-time launch claim without it silently loses routing.
pub fn publish_now_playing(info: &NowPlaying) {
    unsafe { apply_now_playing(info) };
}

/// Release the now-playing session so the system can route keys to another app
/// (used when Settings ▸ Playback disables global media keys).
pub fn clear_now_playing() {
    unsafe {
        let center = MPNowPlayingInfoCenter::defaultCenter();
        center.setNowPlayingInfo(None);
        center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
    }
}

impl MediaKeys {
    /// Publishes minimal now-playing info so the system treats THIS process
    /// as an eligible now-playing app and routes media keys to its handlers.
    pub fn claim_now_playing() {
        publish_now_playing(&NowPlaying {
            title: "Luno".into(),
            artist: String::new(),
            album: String::new(),
            duration_secs: 0.0,
            elapsed_secs: 0.0,
            playing: true,
        });
    }

    /// Registers play/pause/toggle/next/previous handlers on the shared
    /// center. `sink` receives every key press; the caller owns status-aware
    /// dispatch (and must drain queued backend events to the webview).
    pub fn install(sink: Arc<dyn Fn(MediaKey) + Send + Sync>) -> Self {
        unsafe {
            Self::claim_now_playing();
            let center = MPRemoteCommandCenter::sharedCommandCenter();
            let mut tokens = Vec::new();

            macro_rules! bind {
                ($getter:ident, $key:expr) => {{
                    let sink = Arc::clone(&sink);
                    let block = block2::RcBlock::new(
                        move |_event: std::ptr::NonNull<MPRemoteCommandEvent>| {
                            sink($key);
                            MPRemoteCommandHandlerStatus::Success
                        },
                    );
                    let token: Retained<AnyObject> =
                        center.$getter().addTargetWithHandler(&block);
                    tokens.push(token);
                }};
            }

            bind!(togglePlayPauseCommand, MediaKey::Toggle);
            bind!(playCommand, MediaKey::Play);
            bind!(pauseCommand, MediaKey::Pause);
            bind!(nextTrackCommand, MediaKey::Next);
            bind!(previousTrackCommand, MediaKey::Previous);

            Self { _tokens: tokens }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_state_tracks_the_playing_flag() {
        assert_eq!(playback_state_for(true), MPNowPlayingPlaybackState::Playing);
        assert_eq!(playback_state_for(false), MPNowPlayingPlaybackState::Paused);
    }
}
