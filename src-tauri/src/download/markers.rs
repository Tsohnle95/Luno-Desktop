//! Compatibility marker files used by playlist synchronization.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub const CURRENT_SUCCESS: &str = ".downloaded_vids";
pub const LEGACY_SUCCESS: &str = ".downloaded.txt";
pub const PERMANENT_FAILURES: &str = ".sync_failed_vids";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Markers {
    pub successful: HashSet<String>,
    pub failed: HashSet<String>,
}

impl Markers {
    pub fn load(folder: impl AsRef<Path>) -> Self {
        let folder = folder.as_ref();
        let mut result = Self::default();
        read_lines(&folder.join(CURRENT_SUCCESS), |line| {
            if valid_id(line) {
                result.successful.insert(line.to_owned());
            }
        });
        read_lines(&folder.join(LEGACY_SUCCESS), |line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() >= 2 && valid_id(fields[fields.len() - 1]) {
                result
                    .successful
                    .insert(fields[fields.len() - 1].to_owned());
            }
        });
        read_lines(&folder.join(PERMANENT_FAILURES), |line| {
            if let Some(id) = line.split('\t').next().filter(|id| valid_id(id)) {
                result.failed.insert(id.to_owned());
            }
        });
        result
    }

    pub fn load_checked(folder: impl AsRef<Path>) -> Result<Self, String> {
        let folder = folder.as_ref();
        if !folder.is_dir() {
            return Err(if folder.exists() {
                "marker folder is not a directory".into()
            } else {
                "marker folder is missing".into()
            });
        }
        let paths = [
            folder.join(CURRENT_SUCCESS),
            folder.join(LEGACY_SUCCESS),
            folder.join(PERMANENT_FAILURES),
        ];
        let mut found = false;
        let mut result = Self::default();
        for path in &paths {
            if !path.exists() {
                continue;
            }
            found = true;
            let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
            for (line_number, raw) in contents.lines().enumerate() {
                let line = raw.trim();
                if line.is_empty() {
                    continue;
                }
                let valid = if path.ends_with(CURRENT_SUCCESS) {
                    valid_id(line).then(|| result.successful.insert(line.to_owned()))
                } else if path.ends_with(LEGACY_SUCCESS) {
                    let fields: Vec<_> = line.split_whitespace().collect();
                    fields
                        .last()
                        .filter(|id| valid_id(id))
                        .map(|id| result.successful.insert((*id).to_owned()))
                } else {
                    line.split('\t')
                        .next()
                        .filter(|id| valid_id(id))
                        .map(|id| result.failed.insert((*id).to_owned()))
                };
                if valid.is_none() {
                    return Err(format!("invalid marker data in {} at line {}", path.display(), line_number + 1));
                }
            }
        }
        if !found {
            return Err("marker data is missing".into());
        }
        Ok(result)
    }
    pub fn record_success(&mut self, folder: impl AsRef<Path>, id: &str) -> io::Result<()> {
        if !valid_id(id) || self.successful.contains(id) {
            return Ok(());
        }
        append_line(folder.as_ref().join(CURRENT_SUCCESS), id)?;
        self.successful.insert(id.to_owned());
        Ok(())
    }
    pub fn record_failure(
        &mut self,
        folder: impl AsRef<Path>,
        id: &str,
        reason: &str,
    ) -> io::Result<()> {
        if !valid_id(id) || self.failed.contains(id) {
            return Ok(());
        }
        append_line(
            folder.as_ref().join(PERMANENT_FAILURES),
            &format!(
                "{id}\t{}",
                reason
                    .replace('\r', " ")
                    .replace('\n', " ")
                    .replace('\t', " ")
            ),
        )?;
        self.failed.insert(id.to_owned());
        Ok(())
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.chars().any(char::is_whitespace)
}
fn read_lines(path: &Path, mut visit: impl FnMut(&str)) {
    let Ok(file) = fs::File::open(path) else {
        return;
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let line = line.trim();
        if !line.is_empty() {
            visit(line);
        }
    }
}
fn append_line(path: PathBuf, line: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{line}")?;
    file.sync_data()
}
