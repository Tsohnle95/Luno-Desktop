use serde::{Deserialize, Deserializer, Serialize};

pub const CONTRACT_VERSION: u16 = 1;
pub const MAX_SAFE_EVENT_ID: u64 = 9_007_199_254_740_991;
pub const MAX_DISCOVER_LIMIT: usize = 20;

pub fn default_discover_limit() -> usize {
    5
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DownloadJob {
    pub id: JobId,
    pub kind: DownloadKind,
    pub state: JobState,
    pub track_id: Option<TrackId>,
    #[serde(deserialize_with = "progress")]
    pub progress: Option<f32>,
    pub generation: Generation,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchResult {
    pub provider: String,
    pub query: String,
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
