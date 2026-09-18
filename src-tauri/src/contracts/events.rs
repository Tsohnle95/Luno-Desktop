use super::types::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandEnvelope {
    #[serde(deserialize_with = "super::types::version")]
    pub version: u16,
    #[serde(deserialize_with = "super::types::non_empty_string")]
    pub request_id: String,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Command {
    GetLibrary,
    ScanLibrary,
    LoadConfig {
        #[serde(deserialize_with = "super::types::non_empty_string")]
        home: String,
    },
    RegisterRoot {
        #[serde(deserialize_with = "super::types::non_empty_string")]
        root: String,
    },
    PromoteAudioOutput {
        #[serde(deserialize_with = "super::types::non_empty_string")]
        temporary: String,
        #[serde(deserialize_with = "super::types::non_empty_string")]
        destination: String,
    },
    SyncMarkers {
        #[serde(deserialize_with = "super::types::non_empty_string")]
        folder: String,
    },
    SearchDownloader {
        #[serde(deserialize_with = "super::types::non_empty_string")]
        query: String,
        try_soundcloud: bool,
        #[serde(default = "super::types::default_search_limit")]
        limit: usize,
    },
    Discover {
        #[serde(deserialize_with = "super::types::non_empty_string")]
        artist: String,
        #[serde(deserialize_with = "super::types::non_empty_string")]
        title: String,
        #[serde(default = "super::types::default_discover_limit")]
        limit: usize,
    },
    PreviewTap {
        #[serde(deserialize_with = "super::types::non_empty_string")]
        key: String,
        #[serde(default)]
        seeds: Vec<PreviewSeed>,
    },
    PreviewPrefetch {
        #[serde(default)]
        seeds: Vec<PreviewSeed>,
    },
    PreviewNext,
    PreviewPrevious,
    PreviewCancel,
    DiscoverEnable {
        #[serde(default)]
        seeds: Vec<PreviewSeed>,
    },
    DiscoverDisable,
    DiscoverAdvance,
    Save {
        key: TrackId,
        playlist_id: PlaylistId,
    },
    ValidateDirectUrl {
        url: String,
        destination: String,
    },
    PreviewCsv {
        csv: String,
    },
    QueueUrls {
        urls: Vec<String>,
    },
    SelectLibraryRoot {
        root: String,
    },
    CreatePlaylist {
        #[serde(deserialize_with = "super::types::non_empty_string")]
        name: String,
    },
    DeletePlaylist { playlist_id: PlaylistId },
    AddTracksToPlaylist { track_ids: Vec<TrackId>, playlist_id: PlaylistId },
    DeleteTracks { track_ids: Vec<TrackId> },
    EditTrackMetadata {
        track_id: TrackId,
        #[serde(default)]
        artist: String,
        #[serde(deserialize_with = "super::types::non_empty_string")]
        title: String,
    },
    StartDownload {
        job_id: JobId,
        #[serde(deserialize_with = "super::types::non_empty_string")]
        destination: String,
        #[serde(deserialize_with = "super::types::non_empty_string")]
        helper: String,
        #[serde(default)]
        source: String,
    },
    RetryDownload { job_id: JobId },
    BundleHelperPath,
    LoadSettings {
        home: String,
    },
    ResolveRuntimeHome,
    SaveSettings {
        home: String,
        settings: super::types::SettingsInput,
    },
    TransferPreview {
        manifest: String,
        destination: String,
    },
    TransferExport,
    TransferApply {
        manifest: String,
        destination: String,
        approvals: std::collections::BTreeMap<String, String>,
    },
    ToggleFavorite {
        track_id: TrackId,
    },
    RecordRecent {
        track_id: TrackId,
    },
    GetRecent,
    ClearRecent,
    Play {
        track_id: TrackId,
    },
    PlayWithContext {
        track_id: TrackId,
        context: Vec<TrackId>,
    },
    Next,
    Previous,
    PlayNext {
        track_ids: Vec<TrackId>,
    },
    AddToQueue {
        track_ids: Vec<TrackId>,
    },
    RemoveFromQueue {
        #[serde(deserialize_with = "super::types::safe_usize")]
        index: usize,
    },
    ReorderQueue {
        #[serde(deserialize_with = "super::types::safe_usize")]
        from: usize,
        #[serde(deserialize_with = "super::types::safe_usize")]
        to: usize,
    },
    ClearQueue,
    ToggleShuffle,
    SetRepeat {
        repeat: RepeatMode,
    },
    Pause,
    Resume,
    Seek {
        #[serde(deserialize_with = "super::types::safe_u64")]
        position_ms: u64,
    },
    SetVolume {
        volume: f32,
    },
    VideoSource {
        track_id: TrackId,
    },
    VideoStage {
        track_id: TrackId,
    },
    /// Toggle song-page video mode (auto-stage current + prefetch next 5).
    SetVideoMode {
        enabled: bool,
    },
    VideoPlay,
    VideoPause,
    VideoSeek {
        #[serde(deserialize_with = "super::types::safe_u64")]
        position_ms: u64,
    },
    VideoResize {
        #[serde(deserialize_with = "super::types::safe_u64")]
        width: u64,
        #[serde(deserialize_with = "super::types::safe_u64")]
        height: u64,
    },
    VideoFullscreen {
        fullscreen: bool,
    },
    CancelJob {
        job_id: JobId,
    },
    Shutdown {
        reason: ShutdownReason,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_boundary_rejects_negative_and_fractional_positions() {
        assert!(serde_json::from_str::<CommandEnvelope>(r#"{"version":1,"request_id":"x","command":{"type":"seek","payload":{"position_ms":0}}}"#).is_ok());
        assert!(serde_json::from_str::<CommandEnvelope>(r#"{"version":1,"request_id":"x","command":{"type":"seek","payload":{"position_ms":-1}}}"#).is_err());
        assert!(serde_json::from_str::<CommandEnvelope>(r#"{"version":1,"request_id":"x","command":{"type":"seek","payload":{"position_ms":1.5}}}"#).is_err());
    }

    #[test]
    fn playback_commands_carry_context_and_queue_operations() {
        for command in [
            r#"{"type":"play_with_context","payload":{"track_id":"track-b","context":["track-a","track-b"]}}"#,
            r#"{"type":"next"}"#,
            r#"{"type":"previous"}"#,
            r#"{"type":"add_to_queue","payload":{"track_ids":["track-x"]}}"#,
            r#"{"type":"play_next","payload":{"track_ids":["track-y"]}}"#,
            r#"{"type":"remove_from_queue","payload":{"index":2}}"#,
            r#"{"type":"reorder_queue","payload":{"from":1,"to":2}}"#,
            r#"{"type":"toggle_shuffle"}"#,
            r#"{"type":"set_repeat","payload":{"repeat":"all"}}"#,
        ] {
            let json = format!(r#"{{"version":1,"request_id":"test-1","command":{command}}}"#);
            assert!(
                serde_json::from_str::<CommandEnvelope>(&json).is_ok(),
                "{command}"
            );
        }
    }

    #[test]
    fn discover_and_save_commands_carry_explicit_destinations() {
        for command in [
            r#"{"type":"discover_enable","payload":{"seeds":[{"artist":"Artist","title":"Track"}]}}"#,
            r#"{"type":"discover_disable"}"#,
            r#"{"type":"discover_advance"}"#,
            r#"{"type":"save","payload":{"key":"Artist-Track","playlist_id":"playlist-a"}}"#,
        ] {
            let json = format!(r#"{{"version":1,"request_id":"test-1","command":{command}}}"#);
            assert!(
                serde_json::from_str::<CommandEnvelope>(&json).is_ok(),
                "{command}"
            );
        }
        assert!(serde_json::from_str::<CommandEnvelope>(
            r#"{"version":1,"request_id":"test-1","command":{"type":"save","payload":{"key":"Artist-Track","playlist_id":""}}}"#
        )
        .is_err());
        assert!(serde_json::from_str::<CommandEnvelope>(
            r#"{"version":1,"request_id":"test-1","command":{"type":"discover_enable","payload":{"seeds":[]}}}"#
        )
        .is_ok());
    }

    #[test]
    fn event_ids_accept_only_safe_domain_or_delivery_sequences() {
        let envelope = |id: &str| {
            format!(
                r#"{{"version":1,"event_id":"{id}","generation":0,"job_id":null,"session_id":null,"event":{{"type":"error","payload":{{"error":{{"code":"internal","message":"x","retryable":true}}}}}}}}"#
            )
        };
        assert!(serde_json::from_str::<EventEnvelope>(&envelope("event-1")).is_ok());
        assert!(serde_json::from_str::<EventEnvelope>(&envelope("delivery-error-1")).is_ok());
        for id in [
            "event-0",
            "event-01",
            "event-9007199254740992",
            "delivery-error-9007199254740992",
            "event-1.5",
            "error-1",
        ] {
            assert!(
                serde_json::from_str::<EventEnvelope>(&envelope(id)).is_err(),
                "accepted malformed ID {id}"
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    #[serde(deserialize_with = "super::types::version")]
    pub version: u16,
    #[serde(deserialize_with = "super::types::event_id")]
    pub event_id: String,
    pub generation: Generation,
    pub job_id: Option<JobId>,
    pub session_id: Option<SessionId>,
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Event {
    LibrarySnapshot {
        snapshot: LibrarySnapshot,
    },
    LibraryRootStatus {
        #[serde(deserialize_with = "super::types::safe_usize")]
        root_index: usize,
        status: RootStatus,
    },
    PlaybackSnapshot {
        snapshot: PlaybackSnapshot,
    },
    PreviewSnapshot {
        snapshot: PreviewSnapshot,
    },
    DiscoverSnapshot {
        snapshot: DiscoverSnapshot,
    },
    SaveState {
        state: SaveState,
    },
    VideoState {
        state: VideoState,
    },
    TrackStarted {
        track_id: TrackId,
    },
    QueueChanged {
        queue: Vec<TrackId>,
    },
    JobUpdated {
        job: DownloadJob,
    },
    Error {
        error: ContractError,
    },
    Shutdown {
        envelope: ShutdownEnvelope,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    Off,
    One,
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoState {
    pub track_id: Option<TrackId>,
    pub source_url: Option<String>,
    pub staged_path: Option<String>,
    pub status: VideoStatus,
    #[serde(deserialize_with = "super::types::safe_u64")]
    pub position_ms: u64,
    /// Download progress 0–100 while status is Loading; cleared afterwards.
    #[serde(default)]
    pub download_progress: Option<u8>,
    /// Content-revision counter for the CURRENT track's staged file.
    /// `Some(1)` = quick-tier file; `Some(2)` = replaced by the background
    /// full-quality upgrade. The frontend uses it to hot-swap the <video>
    /// element onto the upgraded bytes without a track change. Absent in
    /// legacy payloads; omitted when None so old fixtures stay byte-stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,
    pub fullscreen: bool,
    pub generation: Generation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoStatus {
    NotChecked,
    Loading,
    Ready,
    Playing,
    Paused,
    Ended,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShutdownEnvelope {
    #[serde(deserialize_with = "super::types::version")]
    pub version: u16,
    pub state: ShutdownState,
    pub reason: ShutdownReason,
}

/// Shutdown events use generation 0 as a control-event generation. Consumers
/// must not discard them as stale work results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownState {
    Requested,
    Draining,
    Complete,
    TimedOut,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootStatus {
    Scanned,
    Missing,
    Inaccessible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownReason {
    UserRequested,
    System,
    FatalError,
}
