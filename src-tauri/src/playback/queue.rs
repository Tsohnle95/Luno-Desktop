use crate::contracts::TrackId;

pub const HISTORY_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode { Off, One, All }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueState {
    physical: Vec<TrackId>, original: Vec<TrackId>, current_index: Option<usize>,
    user_queue_count: usize, shuffle: bool, repeat: RepeatMode,
    history: Vec<TrackId>, previews: Vec<TrackId>,
}

impl Default for QueueState {
    fn default() -> Self { Self { physical: vec![], original: vec![], current_index: None, user_queue_count: 0, shuffle: false, repeat: RepeatMode::Off, history: vec![], previews: vec![] } }
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

    pub fn play_with_context(&mut self, track: TrackId, context: Vec<TrackId>) {
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
        if let Some(current) = self.current().cloned() {
            let mut rest: Vec<_> = self.physical.iter().filter(|x| **x != current).cloned().collect();
            let rotation = rest.len().saturating_sub(1);
            rest.rotate_left(rotation);
            self.physical = std::iter::once(current).chain(rest).collect(); self.current_index = Some(0);
        }
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
    pub fn clear(&mut self) { self.physical.clear(); self.original.clear(); self.current_index = None; self.user_queue_count = 0; self.previews.clear(); }
    pub fn start_preview<I: IntoIterator<Item = TrackId>>(&mut self, tracks: I) { self.previews = tracks.into_iter().collect(); }
    pub fn end_preview(&mut self) { self.previews.clear(); }
}
