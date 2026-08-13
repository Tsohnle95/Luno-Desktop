//! Crossfade is intentionally absent from the target desktop application.
//!
//! The migration retires both the legacy preference and the behavior. Playback
//! transitions remain immediate; this module exists to make that decision
//! explicit at the playback boundary without introducing a fade or no-op API.
