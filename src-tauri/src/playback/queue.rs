use crate::contracts::{PlaylistId, TrackId};

pub const HISTORY_LIMIT: usize = 100;

/// Seed each shuffle from the operating system's CSPRNG. The fallback only
/// exists for an OS entropy failure; its counter separates consecutive calls.
static SHUFFLE_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn seed_from_entropy() -> u64 {
    let mut bytes = [0_u8; 8];
    if getrandom::fill(&mut bytes).is_ok() { return u64::from_ne_bytes(bytes); }
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let calls = SHUFFLE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id() as u64;
    nanos ^ (calls.wrapping_mul(0x9E3779B97F4A7C15)) ^ (pid << 32)
}

/// One xorshift64* step (nonzero state required). Pure so shuffle
/// permutations are deterministically unit-testable via injected seeds.
fn next_random(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    x.wrapping_mul(0x2545F4914F6CDD1D)
}

/// Fisher-Yates over `len` positions, deterministic for a given seed. Returns
/// a permutation of `0..len`. Pure; unit-tested for validity, reproducibility,
/// and seed sensitivity.
pub fn shuffled_order(len: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    let mut rng = seed | 1;
    for i in (1..order.len()).rev() {
        let j = (next_random(&mut rng) % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode { Off, One, All }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueState {
    physical: Vec<TrackId>, original: Vec<TrackId>, current_index: Option<usize>,
    user_queue_count: usize, shuffle: bool, repeat: RepeatMode, source_playlist_id: Option<PlaylistId>,
    history: Vec<TrackId>, previews: Vec<TrackId>,
}

impl Default for QueueState {
    fn default() -> Self { Self { physical: vec![], original: vec![], current_index: None, user_queue_count: 0, shuffle: false, repeat: RepeatMode::Off, source_playlist_id: None, history: vec![], previews: vec![] } }
}

impl QueueState {
    pub fn new() -> Self { Self::default() }
    pub fn queue(&self) -> &[TrackId] { &self.physical }
    pub fn current_index(&self) -> Option<usize> { self.current_index }
    pub fn current(&self) -> Option<&TrackId> { self.current_index.and_then(|i| self.physical.get(i)) }
    pub fn user_queue_count(&self) -> usize { self.user_queue_count }
    pub fn history(&self) -> &[TrackId] { &self.history }
    pub fn previews(&self) -> &[TrackId] { &self.previews }
    pub fn shuffle(&self) -> bool { self.shuffle }
    pub fn repeat(&self) -> RepeatMode { self.repeat }
    pub fn source_playlist_id(&self) -> Option<&PlaylistId> { self.source_playlist_id.as_ref() }
    pub fn set_source_playlist_id(&mut self, playlist_id: Option<PlaylistId>) { self.source_playlist_id = playlist_id; }

    pub fn play_with_context(&mut self, track: TrackId, context: Vec<TrackId>) {
        self.source_playlist_id = None;
        self.physical = context; self.original = self.physical.clone(); self.user_queue_count = 0;
        self.current_index = self.physical.iter().position(|id| id == &track).or_else(|| (!self.physical.is_empty()).then_some(0));
        if self.shuffle { self.shuffle_current_first(); }
    }

    pub fn play_next<I: IntoIterator<Item = TrackId>>(&mut self, tracks: I) {
        let items: Vec<_> = tracks.into_iter().collect(); if items.is_empty() { return; }
        if self.physical.is_empty() { self.play_with_context(items[0].clone(), items); self.user_queue_count = self.physical.len().saturating_sub(1); return; }
        let at = self.current_index.map_or(0, |i| i + 1).min(self.physical.len());
        for item in items.iter().rev() { self.physical.insert(at, item.clone()); }
        self.user_queue_count += items.len(); self.sync_original(&items, true);
    }

    pub fn add_to_queue<I: IntoIterator<Item = TrackId>>(&mut self, tracks: I) {
        let items: Vec<_> = tracks.into_iter().collect(); if items.is_empty() { return; }
        if self.physical.is_empty() { return self.play_next(items); }
        let at = self.current_index.map_or(0, |i| i + 1 + self.user_queue_count).min(self.physical.len());
        for (offset, item) in items.iter().enumerate() { self.physical.insert(at + offset, item.clone()); }
        self.user_queue_count += items.len(); self.sync_original(&items, false);
    }

    fn sync_original(&mut self, items: &[TrackId], next: bool) {
        if self.original.is_empty() { self.original = self.physical.clone(); return; }
        let current = self.current().cloned();
        let at = current.and_then(|c| self.original.iter().position(|x| x == &c)).map(|i| if next { i + 1 } else { i + 1 + self.user_queue_count - items.len() }).unwrap_or(self.original.len());
        for (offset, item) in items.iter().enumerate() { self.original.insert((at + offset).min(self.original.len()), item.clone()); }
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if self.shuffle { self.original = self.physical.clone(); self.shuffle_current_first(); }
        else if let Some(current) = self.current().cloned() { self.physical = self.original.clone(); self.current_index = self.physical.iter().position(|x| x == &current); }
    }

    fn shuffle_current_first(&mut self) {
        let Some(current) = self.current().cloned() else { return; };
        let rest: Vec<_> = self.physical.iter().filter(|x| **x != current).cloned().collect();
        // Fisher-Yates from an OS-random seed; current stays first when
        // shuffle is toggled during playback. Shuffled Play chooses a random
        // current song before entering this path.
        let order = shuffled_order(rest.len(), seed_from_entropy());
        let shuffled: Vec<TrackId> = order.into_iter().map(|i| rest[i].clone()).collect();
        self.physical = std::iter::once(current).chain(shuffled).collect();
        self.current_index = Some(0);
    }

    pub fn set_repeat(&mut self, mode: RepeatMode) { self.repeat = mode; }
    pub fn next(&mut self) -> Option<&TrackId> {
        let index = self.current_index?; if self.repeat == RepeatMode::One { return self.current(); }
        if index + 1 < self.physical.len() { self.current_index = Some(index + 1); self.user_queue_count = self.user_queue_count.saturating_sub(1); }
        else if self.repeat == RepeatMode::All && !self.physical.is_empty() { self.current_index = Some(0); self.user_queue_count = 0; } else { return None; }
        self.current()
    }
    pub fn previous(&mut self) -> Option<&TrackId> { let index = self.current_index?; self.user_queue_count = 0; self.current_index = Some(index.saturating_sub(1)); self.current() }
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.physical.len() { return false; }
        self.physical.remove(index);
        if let Some(current) = self.current_index {
            self.current_index = if self.physical.is_empty() { None } else if index < current { Some(current - 1) } else if index == current { Some(current.min(self.physical.len() - 1)) } else { Some(current) };
        }
        self.user_queue_count = self.user_queue_count.min(self.physical.len());
        true
    }
    pub fn reorder(&mut self, from: usize, to: usize) -> bool {
        if from >= self.physical.len() || to >= self.physical.len() { return false; }
        let item = self.physical.remove(from);
        self.physical.insert(to, item);
        if let Some(current) = self.current_index {
            self.current_index = if current == from { Some(to) } else if from < current && current <= to { Some(current - 1) } else if to <= current && current < from { Some(current + 1) } else { Some(current) };
        }
        true
    }
    /// Replaces the visible queue wholesale while keeping the clock position.
    /// Discover-mode adoption, restoration and handoff rebuild the queue
    /// through this single entry so the captured physical queue always stays
    /// restorable.
    pub fn replace(&mut self, queue: Vec<TrackId>, index: usize) {
        if index >= queue.len() {
            self.clear();
            return;
        }
        self.physical = queue;
        self.original = self.physical.clone();
        self.current_index = Some(index);
        self.user_queue_count = 0;
    }

    pub fn record_started(&mut self, track: TrackId, transient: bool) { if transient { return; } self.history.retain(|id| id != &track); self.history.insert(0, track); self.history.truncate(HISTORY_LIMIT); }
    pub fn clear_history(&mut self) { self.history.clear(); }
    pub fn clear(&mut self) { self.physical.clear(); self.original.clear(); self.current_index = None; self.user_queue_count = 0; self.source_playlist_id = None; self.previews.clear(); }
    pub fn start_preview<I: IntoIterator<Item = TrackId>>(&mut self, tracks: I) { self.previews = tracks.into_iter().collect(); }
    pub fn end_preview(&mut self) { self.previews.clear(); }
}

#[cfg(test)]
mod shuffle_tests {
    use super::*;

    fn id(name: &str) -> TrackId { TrackId::new(format!("metadata:{name}:0")).unwrap() }

    #[test]
    fn shuffled_order_is_a_permutation_and_seed_stable() {
        let order = shuffled_order(64, 42);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, Vec::<usize>::from_iter(0..64), "must be a permutation");
        assert_eq!(shuffled_order(64, 42), order, "same seed must reproduce");
        assert_eq!(shuffled_order(0, 7), Vec::<usize>::new());
        assert_eq!(shuffled_order(1, 9), vec![0]);
    }

    #[test]
    fn shuffle_produces_different_orders_across_instances() {
        // The 2026-08-25 complaint: every app launch shuffled identically.
        // Distinct instances (distinct entropy) must diverge; current stays
        // first in every one.
        let queue_input: Vec<TrackId> = (0..24).map(|n| id(&format!("t{n}"))).collect();
        let mut seen_rest_orders = std::collections::HashSet::new();
        for _ in 0..8 {
            let mut q = QueueState::new();
            q.play_with_context(id("t0"), queue_input.clone());
            q.toggle_shuffle();
            assert_eq!(q.current(), Some(&id("t0")), "current must stay first");
            let rest: Vec<_> = q.queue()[1..].to_vec();
            seen_rest_orders.insert(rest);
        }
        assert!(seen_rest_orders.len() >= 2, "shuffle must not be deterministic across instances (got {seen_rest_orders:?})");
    }

    #[test]
    fn toggle_off_restores_true_original_even_after_many_shuffles() {
        let original: Vec<TrackId> = ["a", "b", "c", "d", "e"].iter().map(|n| id(n)).collect();
        let mut q = QueueState::new();
        q.play_with_context(id("c"), original.clone());
        for _ in 0..5 {
            q.toggle_shuffle();
            q.toggle_shuffle();
        }
        q.toggle_shuffle(); // ON
        assert_ne!(q.queue(), &original);
        q.toggle_shuffle(); // OFF
        assert_eq!(q.queue(), &original, "original order must survive repeated shuffles");
    }
}
