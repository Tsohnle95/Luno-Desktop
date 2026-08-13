//! Compatibility marker files used by playlist synchronization.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub const CURRENT_SUCCESS: &str = ".downloaded_vids";
pub const LEGACY_SUCCESS: &str = ".downloaded.txt";
pub const PERMANENT_FAILURES: &str = ".sync_failed_vids";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Markers { pub successful: HashSet<String>, pub failed: HashSet<String> }

impl Markers {
    pub fn load(folder: impl AsRef<Path>) -> Self {
        let folder = folder.as_ref(); let mut result = Self::default();
        read_lines(&folder.join(CURRENT_SUCCESS), |line| { result.successful.insert(line.to_owned()); });
        read_lines(&folder.join(LEGACY_SUCCESS), |line| { let fields: Vec<_> = line.split_whitespace().collect(); if fields.len() >= 2 { result.successful.insert(fields[fields.len() - 1].to_owned()); } });
        read_lines(&folder.join(PERMANENT_FAILURES), |line| { if let Some(id) = line.split('\t').next().filter(|id| !id.is_empty()) { result.failed.insert(id.to_owned()); } }); result
    }
    pub fn record_success(&mut self, folder: impl AsRef<Path>, id: &str) -> io::Result<()> { if id.is_empty() || !self.successful.insert(id.to_owned()) { return Ok(()); } append_line(folder.as_ref().join(CURRENT_SUCCESS), id) }
    pub fn record_failure(&mut self, folder: impl AsRef<Path>, id: &str, reason: &str) -> io::Result<()> { if id.is_empty() || !self.failed.insert(id.to_owned()) { return Ok(()); } append_line(folder.as_ref().join(PERMANENT_FAILURES), &format!("{id}\t{}", reason.replace('\r', " ").replace('\n', " ").replace('\t', " "))) }
}
fn read_lines(path: &Path, mut visit: impl FnMut(&str)) { let Ok(file) = fs::File::open(path) else { return }; for line in BufReader::new(file).lines().map_while(Result::ok) { let line = line.trim(); if !line.is_empty() { visit(line); } } }
fn append_line(path: PathBuf, line: &str) -> io::Result<()> { let mut file = OpenOptions::new().create(true).append(true).open(path)?; writeln!(file, "{line}")?; file.sync_data() }
