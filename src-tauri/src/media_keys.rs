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
use objc2_media_player::{MPNowPlayingInfoCenter, MPRemoteCommandCenter, MPRemoteCommandHandlerStatus, MPRemoteCommandEvent};

/// The subset of transport actions hardware keys can request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKey {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
}

/// Remote-command targets kept alive for the app's lifetime.
pub struct MediaKeys {
    _tokens: Vec<Retained<AnyObject>>,
}

impl MediaKeys {
    /// Publishes minimal now-playing info so the system treats THIS process
    /// as an eligible now-playing app and routes media keys to its handlers.
    fn claim_now_playing() {
        unsafe {
            let info_center = MPNowPlayingInfoCenter::defaultCenter();
            let dict: *mut AnyObject = msg_send![class!(NSMutableDictionary), dictionary];

            let title_key = NSString::from_str("title");
            let title_value = NSString::from_str("Luno");
            let _: () = msg_send![dict, setObject: &*title_value, forKey: &*title_key];

            // A non-zero playback rate advertises "this app is actively
            // playing", which is what wins the now-routing contest.
            let rate_key = NSString::from_str("MPNowPlayingInfoPropertyPlaybackRate");
            let rate_value: *mut AnyObject =
                msg_send![class!(NSNumber), numberWithDouble: 1.0f64];
            let _: () = msg_send![dict, setObject: rate_value, forKey: &*rate_key];

            let _: () = msg_send![&info_center, setNowPlayingInfo: dict];
        }
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
