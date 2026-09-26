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
    pub fn update_physical_queue(&mut self, queue: Vec<TrackId>, index: usize) {
        if self.active && index < queue.len() {
            self.physical_queue = queue;
            self.physical_index = index;
        }
    }
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
        if self.auto_play && self.current().is_some_and(|current| current == &id) {
            return SessionAction::Play(id);
        }
        SessionAction::Wait(id)
    }

    /// A failed focused tap ends playback without changing the physical queue.
    /// Preparation-only sessions keep moving through their window after any
    /// item fails, including the first one.
    pub fn failed_asset(&mut self, generation: u64, id: &TrackId) -> SessionAction {
        if !self.active || generation != self.generation { return SessionAction::Ended; }
        if !self.failed.contains(id) { self.failed.push(id.clone()); }
        // A retry starts in helper/source resolution, which is indeterminate
        // until a fresh byte report arrives. Do not reuse a failed attempt's
        // last numeric value for that new preparation.
        self.progress.remove(id);
        if self.auto_play && self.current.is_none() && self.recommendations.get(self.window_start) == Some(id) {
            self.active = false;
            return SessionAction::Ended;
        }
        self.paths.remove(id);
        SessionAction::Wait(id.clone())
    }

    /// Closes a session whose completion driver stopped unexpectedly. Assets
    /// already published stay usable; every remaining item in this window is
    /// made explicitly retryable instead of being left in a loading state.
    pub fn fail_pending(&mut self, generation: u64) -> bool {
        if !self.active || generation != self.generation { return false; }
        for id in self.window().to_vec() {
            if self.paths.contains_key(&id) { continue; }
            if !self.failed.contains(&id) { self.failed.push(id.clone()); }
            self.progress.remove(&id);
        }
        self.current = None;
        self.active = false;
        true
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selected_cached_successor_starts_playback() {
        let seed = TrackId::new("seed").unwrap();
        let first = TrackId::new("first").unwrap();
        let second = TrackId::new("second").unwrap();
        let mut session = PreviewSession::start(1, vec![seed], 0, vec![first.clone(), second.clone()], &first).unwrap();
        assert_eq!(session.prepared_asset(1, first.clone(), "first.m4a".into()), SessionAction::Play(first));
        assert_eq!(session.prepared_asset(1, second.clone(), "second.m4a".into()), SessionAction::Wait(second.clone()));
        assert_eq!(session.next(), SessionAction::Play(second.clone()));
        assert_eq!(session.prepared_asset(1, second.clone(), "second.m4a".into()), SessionAction::Play(second));
    }

    #[test]
    fn failed_retry_clears_stale_numeric_progress() {
        let physical = vec![TrackId::new("library-track").unwrap()];
        let recommendation = TrackId::new("recommendation").unwrap();
        let mut session = PreviewSession::start_prefetch(7, physical, 0, vec![recommendation.clone()]).unwrap();

        session.set_progress(7, recommendation.clone(), 42);
        assert_eq!(session.progress(), vec![(recommendation.clone(), 42)]);

        session.failed_asset(7, &recommendation);
        assert!(session.progress().is_empty(), "a retry must return to indeterminate preparation");
    }

    #[test]
    fn lost_driver_marks_unprepared_window_items_retryable_and_stops_session() {
        let physical = vec![TrackId::new("library-track").unwrap()];
        let recommendations = (0..6).map(|n| TrackId::new(format!("rec-{n}")).unwrap()).collect::<Vec<_>>();
        let mut session = PreviewSession::start_prefetch(7, physical, 0, recommendations.clone()).unwrap();

        assert!(session.fail_pending(7));
        assert!(!session.is_active());
        assert_eq!(session.unavailable(), &recommendations[..AHEAD_WINDOW]);
        assert!(!session.fail_pending(7), "a stale/repeated driver failure must be ignored");
    }
}
