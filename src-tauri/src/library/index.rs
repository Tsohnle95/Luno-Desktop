//! The published, path-based desktop library index.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::scanner::{scan_library, FileSystem, ScanOutcome};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySnapshot {
    pub generation: u64,
    pub playlists: BTreeMap<String, Vec<PathBuf>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshRequest {
    Started { generation: u64 },
    Coalesced { generation: u64 },
}

/// Owns publication boundaries. A worker may scan freely, but only the newest
/// accepted generation can replace the visible snapshot.
#[derive(Debug, Clone)]
pub struct LibraryIndex {
    snapshot: LibrarySnapshot,
    next_generation: u64,
    active: bool,
    pending: bool,
}

impl Default for LibraryIndex {
    fn default() -> Self {
        Self { snapshot: LibrarySnapshot { generation: 0, playlists: BTreeMap::new() }, next_generation: 0, active: false, pending: false }
    }
}

impl LibraryIndex {
    pub fn snapshot(&self) -> &LibrarySnapshot { &self.snapshot }

    pub fn request_refresh(&mut self) -> RefreshRequest {
        if self.active {
            self.pending = true;
            RefreshRequest::Coalesced { generation: self.next_generation }
        } else {
            self.next_generation += 1;
            self.active = true;
            RefreshRequest::Started { generation: self.next_generation }
        }
    }

    pub fn finish_scan(&mut self, generation: u64, outcome: ScanOutcome) -> bool {
        if !self.active || generation != self.next_generation { return false; }
        self.active = false;
        if generation == self.snapshot.generation { return false; }
        self.snapshot = LibrarySnapshot { generation, playlists: outcome.playlists };
        true
    }

    pub fn take_pending(&mut self) -> bool {
        let pending = self.pending;
        self.pending = false;
        pending
    }

    pub fn scan<F: FileSystem>(&mut self, generation: u64, roots: &[PathBuf], fs: &F) -> bool {
        self.finish_scan(generation, scan_library(roots, fs))
    }
}
