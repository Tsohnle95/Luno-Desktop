//! Playback sleep timer.  The timer is deliberately polled by the playback
//! owner, rather than spawning a thread, so shutdown and cancellation are
//! deterministic and UI/event-loop ownership stays with the caller.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SleepTimer {
    Off,
    Minutes(u32),
}

impl SleepTimer {
    pub const OPTIONS: [Self; 5] = [Self::Off, Self::Minutes(5), Self::Minutes(15), Self::Minutes(30), Self::Minutes(60)];

    pub fn duration(self) -> Option<Duration> {
        match self { Self::Off => None, Self::Minutes(minutes) => Some(Duration::from_secs(minutes as u64 * 60)) }
    }
}

#[derive(Debug, Default)]
pub struct SleepTimerState {
    deadline: Option<Instant>,
    generation: u64,
}

impl SleepTimerState {
    pub fn new() -> Self { Self::default() }

    /// Set or replace the timer.  Replacing a timer invalidates any pending
    /// firing from the previous generation.
    pub fn set(&mut self, timer: SleepTimer, now: Instant) {
        self.generation = self.generation.wrapping_add(1);
        self.deadline = timer.duration().map(|duration| now + duration);
    }

    pub fn cancel(&mut self) { self.set(SleepTimer::Off, Instant::now()); }

    pub fn reset(&mut self, timer: SleepTimer, now: Instant) { self.set(timer, now); }

    pub fn active(&self) -> bool { self.deadline.is_some() }

    pub fn remaining(&self, now: Instant) -> Option<Duration> { self.deadline.map(|deadline| deadline.saturating_duration_since(now)) }

    /// Returns true once when the deadline has passed, then clears the timer.
    pub fn poll(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.deadline = None;
            self.generation = self.generation.wrapping_add(1);
            true
        } else { false }
    }

    pub fn generation(&self) -> u64 { self.generation }
}
