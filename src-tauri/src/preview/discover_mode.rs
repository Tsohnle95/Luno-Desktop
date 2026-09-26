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
    failed: Vec<TrackId>,
    queued: Vec<TrackId>,
    cursor: usize,
    current_recommendation: Option<TrackId>,
    pending_focus: Option<TrackId>,
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
        Self::new_anchored(generation, physical_queue, physical_index, recommendations, None, None)
    }

    /// Creates a projection around the audible recommendation, or carries a
    /// tapped recommendation that is still preparing into Discover ownership.
    pub fn new_anchored(
        generation: u64,
        physical_queue: Vec<TrackId>,
        physical_index: usize,
        recommendations: Vec<TrackId>,
        current_recommendation: Option<&TrackId>,
        pending_focus: Option<&TrackId>,
    ) -> Option<Self> {
        if physical_queue.is_empty() || physical_index >= physical_queue.len() {
            return None;
        }
        let current_recommendation = current_recommendation
            .filter(|id| recommendations.contains(id))
            .cloned();
        let pending_focus = pending_focus
            .filter(|id| recommendations.contains(id) && current_recommendation.is_none())
            .cloned();
        let cursor = current_recommendation.as_ref().or(pending_focus.as_ref())
            .and_then(|id| recommendations.iter().position(|item| item == id))
            .unwrap_or(0);
        Some(Self {
            generation,
            physical_queue,
            physical_index,
            recommendations,
            assets: HashMap::new(),
            failed: Vec::new(),
            queued: Vec::new(),
            cursor,
            current_recommendation,
            pending_focus,
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
    pub fn preparation_window(&self) -> &[TrackId] {
        let end = self.cursor.saturating_add(ROLLING_WINDOW).min(self.recommendations.len());
        &self.recommendations[self.cursor.min(end)..end]
    }
    pub fn ready_count(&self) -> usize {
        self.assets.len()
    }
    pub fn prepared_keys(&self) -> Vec<TrackId> {
        let mut keys: Vec<_> = self.assets.keys().cloned().collect();
        keys.sort_by(|left, right| left.0.cmp(&right.0));
        keys
    }
    pub fn failed_keys(&self) -> Vec<TrackId> { self.failed.clone() }
    pub fn is_failed(&self, id: &TrackId) -> bool { self.failed.contains(id) }
    pub fn queued_assets(&self) -> &[TrackId] {
        &self.queued
    }
    pub fn physical_queue(&self) -> &[TrackId] {
        &self.physical_queue
    }
    pub fn physical_index_for_projected(&self, index: usize) -> Option<usize> {
        let physical = if index <= self.physical_index { index }
            else if index < self.physical_index + 1 + self.queued.len() { return None }
            else { index - self.queued.len() };
        (physical < self.physical_queue.len()).then_some(physical)
    }
    pub fn current_recommendation(&self) -> Option<&TrackId> {
        self.current_recommendation.as_ref()
    }
    pub fn pending_focus(&self) -> Option<&TrackId> {
        self.pending_focus.as_ref()
    }

    /// The visible queue projection and its playback position, used by the
    /// wiring layer to apply adoption and restoration to the playback queue.
    pub fn projected(&self) -> (Vec<TrackId>, usize) {
        let queue = self.projected_queue();
        let index = self.current_recommendation.as_ref()
            .and_then(|id| queue.iter().position(|candidate| candidate == id))
            .unwrap_or(self.physical_index);
        (queue, index)
    }

    /// A ready completion is retained for adoption; stale completions cannot
    /// reintroduce an asset after cancellation or a new seed.
    pub fn ready_asset(&mut self, generation: u64, asset: ReadyAsset) -> DiscoverAction {
        if generation != self.generation || !self.recommendations.contains(&asset.id) {
            return DiscoverAction::Noop;
        }
        self.assets.insert(asset.id.clone(), asset.path);
        self.failed.retain(|failed| failed != &asset.id);
        let in_window = self
            .recommendations
            .iter()
            .position(|id| id == &asset.id)
            .is_some_and(|position| position >= self.cursor && position < self.cursor.saturating_add(ROLLING_WINDOW));
        if self.active && self.discover && in_window && !self.queued.contains(&asset.id) {
            self.queued.push(asset.id.clone());
            self.sort_queued();
            return self.queue_action();
        }
        DiscoverAction::Noop
    }

    /// A terminal preparation failure stays visible to the UI but leaves the
    /// transport projection so Next/Previous cannot park on an unplayable id.
    pub fn failed_asset(&mut self, generation: u64, id: &TrackId) -> DiscoverAction {
        if generation != self.generation || !self.recommendations.contains(id) {
            return DiscoverAction::Noop;
        }
        self.assets.remove(id);
        if !self.failed.contains(id) { self.failed.push(id.clone()); }
        if self.pending_focus.as_ref() == Some(id) { self.pending_focus = None; }
        if self.current_recommendation.as_ref() == Some(id) { self.current_recommendation = None; }
        self.queued.retain(|queued| queued != id);
        self.queue_action()
    }

    pub fn retry_asset(&mut self, id: &TrackId) -> DiscoverAction {
        if !self.active || !self.discover || !self.recommendations.contains(id) {
            return DiscoverAction::Noop;
        }
        self.failed.retain(|failed| failed != id);
        if !self.queued.contains(id) {
            self.queued.push(id.clone());
            self.sort_queued();
        }
        self.queue_action()
    }

    /// Enables Discover and projects the rolling window in order. It does not
    /// start playback: the ordinary completion path autoplays it.
    pub fn enable(&mut self) -> DiscoverAction {
        if self.active && self.discover {
            return DiscoverAction::Noop;
        }
        self.active = true;
        self.discover = true;
        self.cursor = self.current_recommendation.as_ref().or(self.pending_focus.as_ref())
            .and_then(|id| self.recommendations.iter().position(|candidate| candidate == id))
            .unwrap_or(0);
        if self.current_recommendation.is_some() || self.pending_focus.is_some() {
            self.adopt_through(self.cursor);
        }
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
            self.current_recommendation = Some(current.clone());
            self.pending_focus = None;
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
        if !self.owns_projection(queue) {
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

    /// Async preparation can update the projection between a UI selection and
    /// the playback start that follows it. Accept an older snapshot of this
    /// session's projection during that handoff, while still rejecting queues
    /// with reordered recommendations or unrelated tracks.
    fn owns_projection(&self, queue: &[TrackId]) -> bool {
        if queue == self.projected_queue() || queue == self.physical_queue {
            return true;
        }
        let prefix = &self.physical_queue[..=self.physical_index];
        let suffix = &self.physical_queue[self.physical_index + 1..];
        if queue.len() < prefix.len() + suffix.len()
            || !queue.starts_with(prefix)
            || !queue.ends_with(suffix)
        {
            return false;
        }
        let middle_end = queue.len() - suffix.len();
        let recommendations = &queue[prefix.len()..middle_end];
        let mut previous = None;
        for id in recommendations {
            let Some(position) = self.recommendations.iter().position(|item| item == id) else {
                return false;
            };
            if previous.is_some_and(|previous| position <= previous) {
                return false;
            }
            previous = Some(position);
        }
        true
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
            if !self.failed.contains(id) && !self.queued.contains(id) {
                self.queued.push(id.clone());
            }
        }
        self.sort_queued();
    }
    fn sort_queued(&mut self) {
        let positions: HashMap<TrackId, usize> = self
            .recommendations
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, id)| (id, index))
            .collect();
        self.queued
            .sort_by_key(|id| positions.get(id).copied().unwrap_or(usize::MAX));
    }
    fn queue_action(&self) -> DiscoverAction {
        let (queue, index) = self.projected();
        DiscoverAction::QueueChanged {
            queue,
            index,
        }
    }

    fn adopt_through(&mut self, position: usize) {
        for id in self.recommendations.iter().take(position.saturating_add(1)) {
            if !self.failed.contains(id) && !self.queued.contains(id) {
                self.queued.push(id.clone());
            }
        }
        self.sort_queued();
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
    fn queue_edits_translate_only_physical_rows() {
        let recs = vec![id("r0"), id("r1")];
        let mut mode = DiscoverMode::new(1, vec![id("before"), id("seed"), id("after")], 1, recs).unwrap();
        mode.enable();
        assert_eq!(mode.physical_index_for_projected(0), Some(0));
        assert_eq!(mode.physical_index_for_projected(1), Some(1));
        assert_eq!(mode.physical_index_for_projected(2), None);
        assert_eq!(mode.physical_index_for_projected(3), None);
        assert_eq!(mode.physical_index_for_projected(4), Some(2));
    }

    #[test]
    fn enabled_projection_anchors_next_and_previous_at_the_audible_recommendation() {
        let physical = vec![id("physical-before"), id("seed"), id("physical-after")];
        let recs: Vec<_> = (0..7).map(|n| id(&format!("r{n}"))).collect();
        let mut mode = DiscoverMode::new_anchored(
            2,
            physical.clone(),
            1,
            recs.clone(),
            Some(&recs[2]),
            None,
        ).unwrap();
        mode.enable();
        let (queue, index) = mode.projected();
        assert_eq!(queue, vec![
            id("physical-before"), id("seed"), id("r0"), id("r1"), id("r2"),
            id("r3"), id("r4"), id("r5"), id("r6"), id("physical-after"),
        ]);
        assert_eq!(queue[index], id("r2"), "the queue cursor must match the audible recommendation");
        assert_eq!(mode.preparation_window(), &recs[2..7]);
        assert!(matches!(mode.advance(&id("r2")), DiscoverAction::Prepare { id } if id == recs[3]));
        assert!(matches!(mode.advance(&id("r3")), DiscoverAction::Prepare { id } if id == recs[4]));
        let (queue, index) = mode.projected();
        assert_eq!(queue[index], id("r3"), "window growth must keep the audible item selected");
        assert_eq!(mode.disable(3), DiscoverAction::Restore { queue: physical, index: 1 });
    }

    #[test]
    fn pending_tap_keeps_the_seed_current_until_preview_audio_starts() {
        let recs: Vec<_> = (0..7).map(|n| id(&format!("r{n}"))).collect();
        let mut mode = DiscoverMode::new_anchored(
            4,
            vec![id("seed"), id("next")],
            0,
            recs.clone(),
            None,
            Some(&recs[3]),
        ).unwrap();
        mode.enable();
        assert_eq!(mode.pending_focus(), Some(&recs[3]));
        assert_eq!(&mode.preparation_window()[..], &recs[3..]);
        let (queue, index) = mode.projected();
        assert_eq!(queue[index], id("seed"), "a policy toggle must not relabel the song still playing");
        assert_eq!(mode.ready_asset(4, ReadyAsset { id: recs[3].clone(), path: "r3.aac".into() }), DiscoverAction::Noop);
        assert_eq!(mode.current_recommendation(), None, "preparation completion alone does not claim preview audio started");
        assert!(matches!(mode.advance(&recs[3]), DiscoverAction::Prepare { .. }));
        let (queue, index) = mode.projected();
        assert_eq!(queue[index], recs[3], "the started tapped item becomes the discovery cursor");
    }

    #[test]
    fn handoff_accepts_a_stale_snapshot_of_its_own_projection() {
        let recs = vec![id("r0"), id("r1"), id("r2")];
        let physical = vec![id("before"), id("seed"), id("after")];
        let mut mode = DiscoverMode::new(5, physical.clone(), 1, recs.clone()).unwrap();
        mode.enable();
        let selected_queue = mode.projected_queue();

        // A terminal failure can remove an id asynchronously after the UI
        // selected it but before playback reports that its audio started.
        mode.failed_asset(5, &recs[0]);
        assert!(matches!(
            mode.handoff(&selected_queue),
            DiscoverAction::Restore { queue, index } if queue == physical && index == 1
        ));
        assert!(mode.is_active(), "a stale projection is still owned by Discover");
    }

    #[test]
    fn handoff_still_ends_for_an_unrelated_or_reordered_queue() {
        let recs = vec![id("r0"), id("r1")];
        let mut mode = DiscoverMode::new(6, vec![id("seed"), id("next")], 0, recs).unwrap();
        mode.enable();
        assert_eq!(mode.handoff(&[id("seed"), id("r1"), id("r0"), id("next")]), DiscoverAction::Noop);
        assert!(!mode.is_active());
    }

    #[test]
    fn failed_asset_leaves_projection_and_can_be_retried_after_ready() {
        let failed = id("r0");
        let mut mode = DiscoverMode::new(4, vec![id("seed"), id("next")], 0, vec![failed.clone(), id("r1")]).unwrap();
        let _ = mode.enable();
        assert!(mode.projected_queue().contains(&failed));
        let action = mode.failed_asset(4, &failed);
        assert!(matches!(action, DiscoverAction::QueueChanged { .. }));
        assert!(mode.is_failed(&failed));
        assert!(!mode.projected_queue().contains(&failed));
        let _ = mode.retry_asset(&failed);
        assert_eq!(
            mode.projected_queue(),
            vec![id("seed"), failed.clone(), id("r1"), id("next")],
            "retry must restore recommendation order"
        );
        let _ = mode.ready_asset(4, ReadyAsset { id: failed.clone(), path: PathBuf::from("/tmp/r0.aac") });
        assert!(!mode.is_failed(&failed));
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
