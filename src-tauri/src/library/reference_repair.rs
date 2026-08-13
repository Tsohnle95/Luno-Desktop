//! Explicit path reference repair for filesystem rename/delete operations.

use std::collections::BTreeMap;
use std::path::Path;
use super::favorites::{normalize_path, Favorites};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairKind { Rename, Delete }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairReport { pub changed: usize, pub removed: usize, pub stale_favorites: usize }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathReferences {
    pub queue_playlist: Vec<String>,
    pub queue: Vec<String>,
    pub original_queue: Vec<String>,
    pub recent: Vec<String>,
    pub favorites: Favorites,
    pub artwork: BTreeMap<String, String>,
}

impl PathReferences {
    /// Run a multi-reference repair as one unit. A later persistence or
    /// filesystem step can return an error and the in-memory projection is
    /// restored, preventing a partially repaired session.
    pub fn transaction<E, F>(&mut self, operation: F) -> Result<RepairReport, E>
    where F: FnOnce(&mut Self) -> Result<RepairReport, E> {
        let before = self.clone();
        match operation(self) { Ok(report) => Ok(report), Err(error) => { *self = before; Err(error) } }
    }

    /// Apply one approved mapping atomically to all in-memory references.
    /// History is included only as session state and is never persisted here.
    pub fn repair(&mut self, old: impl AsRef<Path>, new: Option<impl AsRef<Path>>) -> RepairReport {
        let old = normalize_path(old);
        let replacement = new.map(normalize_path);
        let mut changed = 0;
        let mut removed = 0;
        for list in [&mut self.queue_playlist, &mut self.queue, &mut self.original_queue, &mut self.recent] {
            for path in list.iter_mut() {
                if *path == old {
                    match &replacement { Some(value) => *path = value.clone(), None => { *path = String::new(); removed += 1; } }
                    changed += 1;
                }
            }
            list.retain(|path| !path.is_empty());
        }
        if self.favorites.contains(&old) {
            self.favorites.remove(&old);
            if let Some(value) = &replacement { self.favorites.set(value, true); }
            changed += 1;
        }
        if let Some(value) = replacement {
            if let Some(art) = self.artwork.remove(&old) { self.artwork.insert(value, art); changed += 1; }
        } else if self.artwork.remove(&old).is_some() { removed += 1; }
        RepairReport { changed, removed, stale_favorites: 0 }
    }

    /// Remove references to files that disappeared in a completed scan.
    pub fn prune_stale<I, P>(&mut self, scanned: I) -> RepairReport
    where I: IntoIterator<Item = P>, P: AsRef<Path> {
        let known: std::collections::BTreeSet<_> = scanned.into_iter().map(normalize_path).collect();
        let before = self.favorites.paths().count();
        let mut removed = 0;
        for list in [&mut self.queue_playlist, &mut self.queue, &mut self.original_queue, &mut self.recent] { let old = list.len(); list.retain(|p| known.contains(p)); removed += old - list.len(); }
        self.artwork.retain(|p, _| known.contains(p));
        let mut favorites = Favorites::default();
        for path in self.favorites.paths().filter(|p| known.contains(*p)) { favorites.set(path, true); }
        self.favorites = favorites;
        RepairReport { changed: 0, removed, stale_favorites: before - self.favorites.paths().count() }
    }
}
