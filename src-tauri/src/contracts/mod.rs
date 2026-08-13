//! Versioned contracts shared by the Rust domain and the TypeScript client.
//!
//! These types are intentionally independent of Tauri command handlers.  The
//! handler layer can therefore evolve without changing the wire format.

pub mod events;
pub mod types;

pub use events::{CommandEnvelope, EventEnvelope, ShutdownEnvelope};
pub use types::{ContractError, ContractVersion, Generation, JobId, SessionId, TrackId, PlaylistId};
