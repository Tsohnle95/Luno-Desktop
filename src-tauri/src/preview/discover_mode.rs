//! Discover-mode queue projection.
//!
//! This state is deliberately independent from the tap-session state machine:
//! Discover captures the physical queue, appends assets which are already
//! ready, and lets the normal player advance into them naturally.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::contracts::TrackId;

pub const ROLLING_WINDOW: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyAsset {
    pub id: TrackId,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoverAction {
    QueueChanged { queue: Vec<TrackId>, index: usize },
    Prepare { id: TrackId },
    Restore { queue: Vec<TrackId>, index: usize },
    Noop,
}

#[derive(Debug, Clone)]
pub struct DiscoverMode {
    generation: u64,
    physical_queue: Vec<TrackId>,
    physical_index: usize,
    recommendations: Vec<TrackId>,
    assets: HashMap<TrackId, PathBuf>,
    queued: Vec<TrackId>,
    cursor: usize,
    discover: bool,
    active: bool,
}

impl DiscoverMode {
    pub fn new(
        generation: u64,
        physical_queue: Vec<TrackId>,
        physical_index: usize,
        recommendations: Vec<TrackId>,
    ) -> Option<Self> {
        if physical_queue.is_empty() || physical_index >= physical_queue.len() {
            return None;
        }
        Some(Self {
            generation,
            physical_queue,
            physical_index,
            recommendations,
            assets: HashMap::new(),
            queued: Vec::new(),
            cursor: 0,
            discover: false,
            active: false,
        })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    pub fn discover_enabled(&self) -> bool {
        self.discover
    }
    pub fn recommendations(&self) -> &[TrackId] {
        &self.recommendations
    }
    pub fn ready_count(&self) -> usize {
        self.assets.len()
    }
    pub fn queued_assets(&self) -> &[TrackId] {
        &self.queued
    }
    pub fn physical_queue(&self) -> &[TrackId] {
        &self.physical_queue
    }

    /// The visible queue projection and its playback position, used by the
    /// wiring layer to apply adoption and restoration to the playback queue.
    pub fn projected(&self) -> (Vec<TrackId>, usize) {
        (self.projected_queue(), self.physical_index)
    }

    /// A ready completion is retained for adoption; stale completions cannot
    /// reintroduce an asset after cancellation or a new seed.
    pub fn ready_asset(&mut self, generation: u64, asset: ReadyAsset) -> DiscoverAction {
        if generation != self.generation || !self.recommendations.contains(&asset.id) {
            return DiscoverAction::Noop;
        }
        self.assets.insert(asset.id.clone(), asset.path);
        let in_window = self
            .recommendations
            .iter()
            .position(|id| id == &asset.id)
            .is_some_and(|position| position < self.cursor.saturating_add(ROLLING_WINDOW));
        if self.active && self.discover && in_window && !self.queued.contains(&asset.id) {
            self.queued.push(asset.id.clone());
            return self.queue_action();
        }
        DiscoverAction::Noop
    }

    /// Enables Discover and projects the rolling window in order. It does not
    /// start playback: the ordinary completion path autoplays it.
    pub fn enable(&mut self) -> DiscoverAction {
        if self.active && self.discover {
            return DiscoverAction::Noop;
        }
        self.active = true;
        self.discover = true;
        self.cursor = 0;
        self.adopt_window();
        self.queue_action()
    }

    pub fn disable(&mut self, generation: u64) -> DiscoverAction {
        self.generation = generation;
        self.active = false;
        self.discover = false;
        self.queued.clear();
        DiscoverAction::Restore {
            queue: self.physical_queue.clone(),
            index: self.physical_index,
        }
    }

    /// Call when the player reaches a recommendation. The next five ready
    /// assets are added; a missing next asset is returned as a preparation job.
    pub fn advance(&mut self, current: &TrackId) -> DiscoverAction {
        if !self.active || !self.discover || !self.queued.contains(current) {
            return DiscoverAction::Noop;
        }
        if let Some(position) = self.recommendations.iter().position(|id| id == current) {
            self.cursor = position.saturating_add(1);
        }
        self.adopt_window();
        // The window is projected up front, so "needs preparing" is defined by
        // the ASSET table, not queue membership. Only look inside the rolling
        // window: past items were already handed off.
        let end = self
            .cursor
            .saturating_add(ROLLING_WINDOW)
            .min(self.recommendations.len());
        self.recommendations[self.cursor..end]
            .iter()
            .find(|id| !self.assets.contains_key(*id))
            .cloned()
            .map_or(DiscoverAction::Noop, |id| DiscoverAction::Prepare { id })
    }

    /// A natural handoff or queue replacement ends Discover without mutating
    /// a queue owned by somebody else.
    pub fn handoff(&mut self, queue: &[TrackId]) -> DiscoverAction {
        if queue != self.projected_queue().as_slice() && queue != self.physical_queue.as_slice() {
            self.active = false;
            self.discover = false;
            self.queued.clear();
            // A caller-owned replacement queue must never be overwritten by restoration.
            return DiscoverAction::Noop;
        }
        DiscoverAction::Restore {
            queue: self.physical_queue.clone(),
            index: self.physical_index,
        }
    }

    pub fn cancel(&mut self, generation: u64) -> DiscoverAction {
        self.disable(generation)
    }

    pub fn path_for(&self, id: &TrackId) -> Option<&PathBuf> {
        self.assets.get(id)
    }

    pub fn projected_queue(&self) -> Vec<TrackId> {
        let mut queue = self.physical_queue[..=self.physical_index].to_vec();
        queue.extend(self.queued.iter().cloned());
        queue.extend(
            self.physical_queue[self.physical_index + 1..]
                .iter()
                .cloned(),
        );
        queue
    }

    /// Adopts the rolling window into the projection. Items enter `queued`
    /// whether or not their asset is staged yet: `projected_queue` is what the
    /// transport walks, so leaving pending items out made Next/Previous fall
    /// through to the physical library until a download happened to finish
    /// (the reported "discover Next plays my library"). A pending id whose path
    /// is unresolved is prepared on selection instead of being skipped.
    fn adopt_window(&mut self) {
        for id in self
            .recommendations
            .iter()
            .skip(self.cursor)
            .take(ROLLING_WINDOW)
        {
            if !self.queued.contains(id) {
                self.queued.push(id.clone());
            }
        }
    }
    fn queue_action(&self) -> DiscoverAction {
        DiscoverAction::QueueChanged {
            queue: self.projected_queue(),
            index: self.physical_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> TrackId {
        TrackId::new(value).unwrap()
    }

    #[test]
    fn enable_projects_pending_recommendations_so_transport_walks_them() {
        // Regression: enable only adopted ALREADY-STAGED assets, so until a
        // download finished the projection equalled the physical queue and
        // Next/Previous walked the library instead of the temporary list.
        let recs = vec![id("r0"), id("r1"), id("r2")];
        let mut mode = DiscoverMode::new(1, vec![id("seed"), id("next")], 0, recs).unwrap();
        assert!(matches!(mode.enable(), DiscoverAction::QueueChanged { .. }));
        assert_eq!(mode.queued_assets().len(), 3, "pending window is projected immediately");
        assert_eq!(
            mode.projected_queue(),
            vec![id("seed"), id("r0"), id("r1"), id("r2"), id("next")]
        );
    }

    #[test]
    fn stale_generation_ready_asset_is_ignored() {
        let mut mode = DiscoverMode::new(1, vec![id("seed")], 0, vec![id("r")]).unwrap();
        assert_eq!(
            mode.ready_asset(2, ReadyAsset { id: id("r"), path: "r.mp3".into() }),
            DiscoverAction::Noop
        );
        assert!(mode.path_for(&id("r")).is_none());
    }
}
