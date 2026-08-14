use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutableKind { YtDlp, Ffmpeg, Ffprobe }

impl ExecutableKind {
    pub fn filename(self) -> &'static str {
        match self { Self::YtDlp => "yt-dlp", Self::Ffmpeg => "ffmpeg", Self::Ffprobe => "ffprobe" }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutableError { Missing(ExecutableKind), NotExecutable(PathBuf), NotFile(PathBuf) }

impl fmt::Display for ExecutableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(kind) => write!(f, "{} was not found", kind.filename()),
            Self::NotExecutable(path) => write!(f, "{} is not executable", path.display()),
            Self::NotFile(path) => write!(f, "{} is not a regular file", path.display()),
        }
    }
}
impl std::error::Error for ExecutableError {}

/// Resolves release-managed binaries before consulting PATH. Relative PATH entries
/// are deliberately ignored: accepting them makes the working directory executable.
pub fn discover(kind: ExecutableKind, managed_dir: Option<&Path>) -> Result<PathBuf, ExecutableError> {
    if let Some(dir) = managed_dir {
        let candidate = dir.join(kind.filename());
        if candidate.exists() { return verify(candidate); }
    }
    let path_var = env::var_os("PATH").unwrap_or_default();
    for dir in env::split_paths(&path_var) {
        if !dir.is_absolute() { continue; }
        let candidate = dir.join(kind.filename());
        if candidate.is_file() && is_executable(&candidate) { return Ok(candidate); }
    }
    Err(ExecutableError::Missing(kind))
}

fn verify(path: PathBuf) -> Result<PathBuf, ExecutableError> {
    let metadata = fs::metadata(&path).map_err(|_| ExecutableError::NotFile(path.clone()))?;
    if !metadata.is_file() { return Err(ExecutableError::NotFile(path)); }
    if !is_executable(&path) { return Err(ExecutableError::NotExecutable(path)); }
    Ok(path)
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; fs::metadata(path).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false) }
    #[cfg(not(unix))] { path.is_file() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_managed_binary_falls_back_without_accepting_relative_path() {
        let dir = std::env::temp_dir().join(format!("luno-exec-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let managed = dir.join(ExecutableKind::Ffmpeg.filename());
        let _ = fs::create_dir(&managed);
        assert!(matches!(discover(ExecutableKind::Ffmpeg, Some(&dir)), Err(ExecutableError::NotFile(_))));
        let _ = fs::remove_dir_all(dir);
    }
}
