use serde::{Deserialize, Serialize};
use super::types::*;

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
    Play { track_id: TrackId },
    Pause,
    Seek { #[serde(deserialize_with = "super::types::safe_u64")] position_ms: u64 },
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

    #[test]
    fn event_ids_accept_only_safe_domain_or_delivery_sequences() {
        let envelope = |id: &str| format!(r#"{{"version":1,"event_id":"{id}","generation":0,"job_id":null,"session_id":null,"event":{{"type":"error","payload":{{"error":{{"code":"internal","message":"x","retryable":true}}}}}}}}"#);
        assert!(serde_json::from_str::<EventEnvelope>(&envelope("event-1")).is_ok());
        assert!(serde_json::from_str::<EventEnvelope>(&envelope("delivery-error-1")).is_ok());
        for id in ["event-0", "event-01", "event-9007199254740992", "delivery-error-9007199254740992", "event-1.5", "error-1"] {
            assert!(serde_json::from_str::<EventEnvelope>(&envelope(id)).is_err(), "accepted malformed ID {id}");
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
    LibrarySnapshot { snapshot: LibrarySnapshot },
    LibraryRootStatus { #[serde(deserialize_with = "super::types::safe_usize")] root_index: usize, status: RootStatus },
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
