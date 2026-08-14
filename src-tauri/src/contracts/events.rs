use serde::{Deserialize, Serialize};
use super::types::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandEnvelope {
    #[serde(deserialize_with = "super::types::version")]
    pub version: u16,
    pub request_id: String,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Command {
    GetLibrary,
    ScanLibrary,
    Play { track_id: TrackId },
    Pause,
    Seek { position_ms: u64 },
    CancelJob { job_id: JobId },
    Shutdown { reason: ShutdownReason },
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    #[serde(deserialize_with = "super::types::version")]
    pub version: u16,
    pub event_id: String,
    pub generation: Generation,
    pub job_id: Option<JobId>,
    pub session_id: Option<SessionId>,
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Event {
    LibrarySnapshot { snapshot: LibrarySnapshot },
    LibraryRootStatus { root_index: usize, status: RootStatus },
    PlaybackSnapshot { snapshot: PlaybackSnapshot },
    TrackStarted { track_id: TrackId },
    QueueChanged { queue: Vec<TrackId> },
    JobUpdated { job: DownloadJob },
    Error { error: ContractError },
    Shutdown { envelope: ShutdownEnvelope },
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
pub enum ShutdownState { Requested, Draining, Complete, TimedOut, Failed }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootStatus { Scanned, Missing, Inaccessible }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownReason { UserRequested, System, FatalError }
