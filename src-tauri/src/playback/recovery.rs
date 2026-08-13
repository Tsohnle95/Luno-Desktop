//! Failure recovery policy for automatic playback transitions.

pub const MAX_CONSECUTIVE_FAILURES: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction { Skip, Stop }

#[derive(Debug, Default, Clone, Copy)]
pub struct FailureRecovery { consecutive: u32, stopped: bool }

impl FailureRecovery {
    pub fn new() -> Self { Self::default() }
    pub fn consecutive_failures(&self) -> u32 { self.consecutive }
    pub fn stopped(&self) -> bool { self.stopped }
    pub fn record_failure(&mut self) -> RecoveryAction {
        if self.stopped { return RecoveryAction::Stop; }
        self.consecutive = self.consecutive.saturating_add(1);
        if self.consecutive > MAX_CONSECUTIVE_FAILURES { self.stopped = true; RecoveryAction::Stop } else { RecoveryAction::Skip }
    }
    pub fn record_success(&mut self) { self.consecutive = 0; }
    pub fn shutdown(&mut self) { self.stopped = true; }
}
