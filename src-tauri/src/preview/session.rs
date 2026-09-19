//! State machine for a tapped recommendation preview.
//!
//! The session owns no library state.  It captures the physical playback queue
//! on entry and produces a temporary queue projection until the user hands
//! playback back to the physical queue (or the session is invalidated).

use std::collections::HashMap;
use std::path::PathBuf;

use crate::contracts::TrackId;

pub const AHEAD_WINDOW: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMode { Tap }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionAction {
    Play(TrackId),
    Wait(TrackId),
    LeaveToNext { queue: Vec<TrackId>, index: Option<usize> },
    ReturnToSeed { queue: Vec<TrackId>, index: usize },
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff { pub queue: Vec<TrackId>, pub index: Option<usize> }

#[derive(Debug, Clone)]
pub struct PreviewSession {
    generation: u64,
    physical_queue: Vec<TrackId>,
    physical_index: usize,
    recommendations: Vec<TrackId>,
    paths: HashMap<TrackId, PathBuf>,
    failed: Vec<TrackId>,
    progress: std::collections::HashMap<TrackId, u8>,
    window_start: usize,
    window_end: usize,
    current: Option<usize>,
    active: bool,
    auto_play: bool,
}

impl PreviewSession {
    /// Captures the queue before any preview item is inserted.
    pub fn start(
        generation: u64,
        physical_queue: Vec<TrackId>,
        physical_index: usize,
        recommendations: Vec<TrackId>,
        tapped: &TrackId,
    ) -> Option<Self> {
        let tapped_index = recommendations.iter().position(|id| id == tapped)?;
        Self::create(generation, physical_queue, physical_index, recommendations, tapped_index, true)
    }

    pub fn start_prefetch(
        generation: u64,
        physical_queue: Vec<TrackId>,
        physical_index: usize,
        recommendations: Vec<TrackId>,
    ) -> Option<Self> {
        if recommendations.is_empty() { return None; }
        let mut session = Self::create(generation, physical_queue, physical_index, recommendations, 0, false)?;
        session.window_end = AHEAD_WINDOW.min(session.recommendations.len());
        Some(session)
    }

    fn create(
        generation: u64,
        physical_queue: Vec<TrackId>,
        physical_index: usize,
        recommendations: Vec<TrackId>,
        start: usize,
        auto_play: bool,
    ) -> Option<Self> {
        if physical_queue.is_empty() || physical_index >= physical_queue.len() { return None; }
        Some(Self {
            generation,
            physical_queue,
            physical_index,
            recommendations,
            paths: HashMap::new(),
            failed: Vec::new(),
            progress: std::collections::HashMap::new(),
            window_start: start,
            window_end: start.saturating_add(1 + AHEAD_WINDOW),
            current: None,
            active: true,
            auto_play,
        })
    }

    pub fn generation(&self) -> u64 { self.generation }
    pub fn is_active(&self) -> bool { self.active }
    pub fn is_prefetch(&self) -> bool { !self.auto_play }
    pub fn current(&self) -> Option<&TrackId> { self.current.and_then(|i| self.recommendations.get(i)) }
    pub fn window(&self) -> &[TrackId] { &self.recommendations[self.window_start..self.window_end.min(self.recommendations.len())] }
    pub fn prepared(&self) -> usize { self.paths.len() }
    pub fn prepared_keys(&self) -> Vec<TrackId> { self.paths.keys().cloned().collect() }
    pub fn is_prepared(&self, id: &TrackId) -> bool { self.paths.contains_key(id) }
    pub fn unavailable(&self) -> &[TrackId] { &self.failed }
    pub fn progress(&self) -> Vec<(TrackId, u8)> { self.progress.iter().map(|(key, value)| (key.clone(), *value)).collect() }
    pub fn set_progress(&mut self, generation: u64, id: TrackId, percent: u8) {
        if self.active && generation == self.generation && self.in_window(&id) { self.progress.insert(id, percent.min(100)); }
    }
    pub fn physical_queue(&self) -> &[TrackId] { &self.physical_queue }
    pub fn physical_index(&self) -> usize { self.physical_index }
    pub fn recommendations(&self) -> &[TrackId] { &self.recommendations }

    /// Accepts a completed asset.  A stale completion is ignored and cannot
    /// resurrect a session after handoff or shutdown.
    pub fn prepared_asset(&mut self, generation: u64, id: TrackId, path: PathBuf) -> SessionAction {
        if !self.active || generation != self.generation || !self.in_window(&id) { return SessionAction::Ended; }
        self.failed.retain(|failed| failed != &id);
        self.paths.insert(id.clone(), path);
        self.progress.insert(id.clone(), 100);
        if self.current.is_none() && self.auto_play {
            self.current = Some(self.window_start);
            return SessionAction::Play(id);
        }
        SessionAction::Wait(id)
    }

    /// A failed tapped item ends the session without changing physical
    /// playback.  Later-item failures simply leave a preparation gap.
    pub fn failed_asset(&mut self, generation: u64, id: &TrackId) -> SessionAction {
        if !self.active || generation != self.generation { return SessionAction::Ended; }
        if !self.failed.contains(id) { self.failed.push(id.clone()); }
        if self.current.is_none() && self.recommendations.get(self.window_start) == Some(id) {
            self.active = false;
            return SessionAction::Ended;
        }
        self.paths.remove(id);
        SessionAction::Wait(id.clone())
    }

    /// Advances to a prepared recommendation and extends the desired window.
    pub fn next(&mut self) -> SessionAction {
        let next = self.current.map_or(self.window_start, |i| i + 1);
        if next >= self.window_end.min(self.recommendations.len()) { return self.leave_next(); }
        self.current = Some(next);
        let id = self.recommendations[next].clone();
        if self.paths.contains_key(&id) { SessionAction::Play(id) } else { SessionAction::Wait(id) }
    }

    pub fn previous(&mut self) -> SessionAction {
        match self.current {
            Some(i) if i > self.window_start => {
                let id = self.recommendations[i - 1].clone();
                self.current = Some(i - 1);
                if self.paths.contains_key(&id) { SessionAction::Play(id) } else { SessionAction::Wait(id) }
            }
            _ => self.return_to_seed(),
        }
    }

    pub fn refill_target(&self) -> usize {
        self.current.map_or(0, |i| (i + 1 + AHEAD_WINDOW).min(self.recommendations.len()))
    }

    pub fn invalidate(&mut self, new_generation: u64) -> Handoff {
        self.generation = new_generation;
        self.active = false;
        self.handoff()
    }

    pub fn shutdown(&mut self) -> Handoff { self.invalidate(self.generation.saturating_add(1)) }

    pub fn handoff(&self) -> Handoff {
        Handoff { queue: self.physical_queue.clone(), index: Some(self.physical_index) }
    }

    fn leave_next(&mut self) -> SessionAction {
        self.active = false;
        let index = (self.physical_index + 1 < self.physical_queue.len()).then_some(self.physical_index + 1);
        SessionAction::LeaveToNext { queue: self.physical_queue.clone(), index }
    }

    fn return_to_seed(&mut self) -> SessionAction {
        self.active = false;
        SessionAction::ReturnToSeed { queue: self.physical_queue.clone(), index: self.physical_index }
    }

    fn in_window(&self, id: &TrackId) -> bool { self.window().iter().any(|candidate| candidate == id) }
}
