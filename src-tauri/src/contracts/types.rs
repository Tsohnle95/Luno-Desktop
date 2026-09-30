use serde::{Deserialize, Deserializer, Serialize};

pub const CONTRACT_VERSION: u16 = 1;
pub const MAX_SAFE_EVENT_ID: u64 = 9_007_199_254_740_991;
pub const MAX_DISCOVER_LIMIT: usize = 20;
pub const MAX_PREVIEW_SEEDS: usize = 50;
/// Downloader search result cap per provider (YouTube). The frontend "Load
/// More" grows the cap; legacy parity capped at 10.
pub const MAX_DOWNLOADER_SEARCH_LIMIT: usize = 50;

pub fn default_discover_limit() -> usize {
    5
}

pub fn default_search_limit() -> usize {
    10
}

pub(crate) fn non_empty_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if value.trim().is_empty() {
        Err(serde::de::Error::custom("identifier must not be empty"))
    } else {
        Ok(value)
    }
}

pub(crate) fn safe_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    if value <= MAX_SAFE_EVENT_ID {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "number exceeds JavaScript safe integer range",
        ))
    }
}

pub(crate) fn safe_usize<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let value = safe_u64(deserializer)?;
    usize::try_from(value).map_err(|_| serde::de::Error::custom("number exceeds platform range"))
}

pub(crate) fn safe_option_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<u64>::deserialize(deserializer)?.map_or(Ok(None), |value| {
        if value <= MAX_SAFE_EVENT_ID {
            Ok(Some(value))
        } else {
            Err(serde::de::Error::custom(
                "number exceeds JavaScript safe integer range",
            ))
        }
    })
}

pub(crate) fn progress<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<f32>::deserialize(deserializer)?;
    if value.map_or(true, |value| {
        value.is_finite() && (0.0..=1.0).contains(&value)
    }) {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "progress must be finite and between zero and one",
        ))
    }
}

pub(crate) fn event_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    let digits = value
        .strip_prefix("event-")
        .or_else(|| value.strip_prefix("delivery-error-"));
    let valid = digits.is_some_and(|digits| {
        !digits.is_empty()
            && !digits.starts_with('0')
            && digits.bytes().all(|byte| byte.is_ascii_digit())
            && digits
                .parse::<u64>()
                .is_ok_and(|id| id <= MAX_SAFE_EVENT_ID)
    });
    if valid {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("invalid event ID"))
    }
}

pub(crate) fn version<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u16::deserialize(deserializer)?;
    if value == CONTRACT_VERSION {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(format!(
            "unsupported contract version {value}"
        )))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractVersion(#[serde(deserialize_with = "version")] pub u16);

impl Default for ContractVersion {
    fn default() -> Self {
        Self(CONTRACT_VERSION)
    }
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(#[serde(deserialize_with = "non_empty_string")] pub String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ContractError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(ContractError::invalid("identifier must not be empty"));
                }
                Ok(Self(value))
            }
        }
    };
}

id_type!(TrackId);
id_type!(PlaylistId);
id_type!(JobId);
id_type!(SessionId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Generation(#[serde(deserialize_with = "safe_u64")] pub u64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderId {
    #[serde(deserialize_with = "non_empty_string")]
    pub provider: String,
    #[serde(deserialize_with = "non_empty_string")]
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub id: TrackId,
    pub path: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    #[serde(deserialize_with = "safe_option_u64")]
    pub duration_ms: Option<u64>,
    pub artwork_url: Option<String>,
    pub favorite: bool,
    pub provider: Option<ProviderId>,
    /// Portable first-added timestamp (ms since the Unix epoch), retained in
    /// the library-root metadata sidecar. Existing files seed it from mtime;
    /// `None` only when both sources are unavailable. Omitted from the wire
    /// when absent to keep the shape backward compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Playlist {
    pub id: PlaylistId,
    pub name: String,
    pub path: Option<String>,
    pub url: Option<String>,
    pub ordered_track_ids: Vec<TrackId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaylistSyncIssue {
    #[serde(deserialize_with = "non_empty_string")]
    pub video_id: String,
    #[serde(deserialize_with = "non_empty_string")]
    pub title: String,
    pub channel: Option<String>,
    #[serde(deserialize_with = "safe_option_u64")]
    pub duration_secs: Option<u64>,
    #[serde(deserialize_with = "non_empty_string")]
    pub reason: String,
    pub attempted_source_id: Option<String>,
    pub selected_source_id: Option<String>,
    pub retry_pending: bool,
    #[serde(deserialize_with = "safe_u64")]
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaylistSyncCandidate {
    #[serde(deserialize_with = "non_empty_string")]
    pub video_id: String,
    #[serde(deserialize_with = "non_empty_string")]
    pub title: String,
    pub channel: Option<String>,
    pub duration: Option<String>,
    pub thumbnail_url: Option<String>,
    #[serde(deserialize_with = "non_empty_string")]
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibrarySnapshot {
    #[serde(deserialize_with = "version")]
    pub version: u16,
    pub generation: Generation,
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaybackSnapshot {
    pub queue: Vec<TrackId>,
    pub current_index: Option<u32>,
    #[serde(default)]
    pub audible_track_id: Option<TrackId>,
    pub shuffle: bool,
    pub repeat: crate::contracts::events::RepeatMode,
    #[serde(deserialize_with = "safe_u64")]
    pub position_ms: u64,
    #[serde(deserialize_with = "safe_option_u64")]
    pub duration_ms: Option<u64>,
    pub status: PlaybackStatus,
    pub generation: Generation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
    Failed,
}

/// One recommendation offered to a preview session.  The session key is the
/// normalized `artist-title` safe key, mirrored by the frontend `previewKey`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewSeed {
    #[serde(deserialize_with = "non_empty_string")]
    pub artist: String,
    #[serde(deserialize_with = "non_empty_string")]
    pub title: String,
}

/// Projection of the active preview session, published on every transition.
/// Artwork and metadata live with the discover results; only keys are signed
/// into the snapshot so the frontend can map rows to states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewSnapshot {
    #[serde(deserialize_with = "safe_u64")]
    pub generation: u64,
    pub active: bool,
    pub current: Option<String>,
    pub window: Vec<String>,
    pub prepared: Vec<String>,
    pub unavailable: Vec<String>,
    pub cancelled: bool,
    #[serde(default)]
    pub progress: Vec<PreviewProgress>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewProgress {
    pub key: String,
    pub percent: u8,
}

/// Discover-mode state: the rolling recommendation window, the number of
/// staged assets, and how many of them are adopted into the playback queue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoverSnapshot {
    #[serde(deserialize_with = "safe_u64")]
    pub generation: u64,
    pub enabled: bool,
    pub window: Vec<String>,
    pub ready: usize,
    pub queued: usize,
    /// Stable keys whose temporary audio is atomically available. The field
    /// is additive so older clients can continue using the aggregate counts.
    #[serde(default)]
    pub prepared: Vec<String>,
    /// Stable keys whose latest preparation attempt reached a terminal error.
    #[serde(default)]
    pub unavailable: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveStateKind {
    Saving,
    Queued,
    Saved,
    Failed,
}

/// Process-local state of an explicit save-to-playlist request. Staged
/// recommendations are promoted from the preview store; unstaged ones fall
/// back to a queued durable download observed through job events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveState {
    pub key: TrackId,
    pub playlist_id: PlaylistId,
    pub state: SaveStateKind,
    pub already_saved: bool,
    pub path: Option<String>,
    pub job_id: Option<JobId>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DownloadJob {
    pub id: JobId,
    pub kind: DownloadKind,
    pub state: JobState,
    pub track_id: Option<TrackId>,
    #[serde(deserialize_with = "progress")]
    pub progress: Option<f32>,
    /// Transfer completion is followed by validation/promotion/rescan work.
    /// Keep that phase explicit so a 95–97% job cannot look stalled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<DownloadPhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_item: Option<String>,
    pub generation: Generation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPhase {
    Finalizing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadKind {
    Single,
    Csv,
    Sync,
    UrlQueue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsInput {
    pub music_root: Option<String>,
    pub auto_discovery: Option<bool>,
    pub volume: Option<f64>,
    pub sleep_timer: Option<serde_json::Value>,
    pub global_keys: Option<bool>,
    pub favorite_songs: Vec<String>,
    /// Song-page video mode: auto-stage the current track's music video and
    /// pre-download the next five queue videos. Absent = off.
    #[serde(default)]
    pub video_mode: Option<bool>,
    /// Browser whose signed-in YouTube cookies playlist lookup may use.
    /// `"none"` explicitly clears the preference; absent/null leaves it alone.
    #[serde(default)]
    pub youtube_cookies_browser: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchResult {
    pub provider: String,
    /// The exact source handed to the download helper: a URL or a provider
    /// search query (`ytsearch1:...`). The frontend passes this verbatim as
    /// `start_download`'s `source`.
    pub query: String,
    pub title: String,
    pub artist: Option<String>,
    pub duration: Option<String>,
    pub thumbnail_url: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    UnsupportedVersion,
    NotFound,
    Busy,
    Cancelled,
    Unavailable,
    Internal,
    ShuttingDown,
}

impl ContractError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidRequest,
            message: message.into(),
            retryable: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unknown_version() {
        let result: Result<LibrarySnapshot, _> =
            serde_json::from_str(r#"{"version":2,"generation":0,"tracks":[],"playlists":[]}"#);
        assert!(result.is_err());
    }
    #[test]
    fn rejects_unknown_fields() {
        let result: Result<ProviderId, _> =
            serde_json::from_str(r#"{"provider":"x","id":"y","extra":true}"#);
        assert!(result.is_err());
    }
    #[test]
    fn rejects_unsafe_numbers_and_empty_identifiers() {
        let snapshot = r#"{"version":1,"generation":9007199254740992,"tracks":[],"playlists":[]}"#;
        assert!(serde_json::from_str::<LibrarySnapshot>(snapshot).is_err());
        let track = r#"{"id":"","path":null,"title":"x","artist":null,"album":null,"duration_ms":null,"artwork_url":null,"favorite":false,"provider":null}"#;
        assert!(serde_json::from_str::<Track>(track).is_err());
        let provider = r#"{"provider":"","id":"x"}"#;
        assert!(serde_json::from_str::<ProviderId>(provider).is_err());
    }
}
