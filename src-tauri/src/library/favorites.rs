//! Path-based favorites. Paths are location data, but remain the desktop
//! compatibility format for this local-only preference.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub fn normalize_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")) .join(path)
    };
    let mut result = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => { result.pop(); }
            other => result.push(other.as_os_str()),
        }
    }
    let value = result.to_string_lossy().to_string();
    #[cfg(target_os = "macos")]
    { value.to_lowercase() }
    #[cfg(not(target_os = "macos"))]
    { value }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Favorites {
    paths: BTreeSet<String>,
}

impl Favorites {
    pub fn from_paths<I, P>(paths: I) -> Self
    where I: IntoIterator<Item = P>, P: AsRef<Path> {
        Self { paths: paths.into_iter().map(normalize_path).collect() }
    }
    pub fn paths(&self) -> impl Iterator<Item = &str> { self.paths.iter().map(String::as_str) }
    pub fn contains(&self, path: impl AsRef<Path>) -> bool { self.paths.contains(&normalize_path(path)) }
    pub fn set(&mut self, path: impl AsRef<Path>, favorite: bool) {
        let path = normalize_path(path);
        if favorite { self.paths.insert(path); } else { self.paths.remove(&path); }
    }
    /// Retain only tracks published by the completed scan and return the
    /// virtual Favorites playlist projection in deterministic order.
    pub fn project<I, P>(&mut self, scanned: I) -> Vec<PathBuf>
    where I: IntoIterator<Item = P>, P: AsRef<Path> {
        let available: BTreeSet<_> = scanned.into_iter().map(normalize_path).collect();
        self.paths.retain(|path| available.contains(path));
        available.into_iter().filter(|path| self.paths.contains(path)).map(PathBuf::from).collect()
    }
    pub fn remove(&mut self, path: impl AsRef<Path>) { self.paths.remove(&normalize_path(path)); }
}
