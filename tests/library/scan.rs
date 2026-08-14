use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use luno_desktop_lib::library::index::{LibraryIndex, RefreshRequest};
use luno_desktop_lib::library::scanner::{scan_library, Entry, FileSystem, RootStatus};

#[derive(Default)]
struct FakeFs { dirs: BTreeMap<PathBuf, Result<Vec<Entry>, String>>, mtimes: BTreeMap<PathBuf, SystemTime> }

impl FileSystem for FakeFs {
    fn read_dir(&self, path: &Path) -> Result<Vec<Entry>, String> { self.dirs.get(path).cloned().unwrap_or_else(|| Err("not found".into())) }
    fn modified(&self, path: &Path) -> Result<SystemTime, String> { self.mtimes.get(path).copied().ok_or_else(|| "missing mtime".into()) }
}

fn entry(root: &Path, name: &str, is_dir: bool) -> Entry { Entry { path: root.join(name), name: name.into(), is_dir } }

#[test]
fn scans_immediate_playlists_empty_folders_and_unsorted() {
    let root = PathBuf::from("/music");
    let playlist = root.join("Zed");
    let mut fs = FakeFs::default();
    fs.dirs.insert(root.clone(), Ok(vec![entry(&root, "Zed", true), entry(&root, ".luno_previews", true), entry(&root, "root.MP3", false), entry(&root, "ignored.ogg", false), entry(&root, "nested", true)]));
    fs.dirs.insert(playlist.clone(), Ok(vec![entry(&playlist, "b.flac", false), entry(&playlist, "a.mp3", false), entry(&playlist, "nested", true), entry(&playlist, "no.ogg", false)]));
    let result = scan_library(&[root.clone()], &fs);
    assert_eq!(result.playlists["Zed"], vec![playlist.join("a.mp3"), playlist.join("b.flac")]);
    assert_eq!(result.playlists["Unsorted"], vec![root.join("root.MP3")]);
    assert!(!result.playlists.contains_key(".luno_previews"));
}

#[test]
fn unsorted_is_newest_first_with_casefolded_name_ties() {
    let root = PathBuf::from("/music");
    let mut fs = FakeFs::default();
    let entries = ["z.mp3", "A.mp3", "b.mp3"].map(|name| entry(&root, name, false));
    fs.dirs.insert(root.clone(), Ok(entries.into()));
    let tie = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
    fs.mtimes.insert(root.join("z.mp3"), tie);
    fs.mtimes.insert(root.join("A.mp3"), tie);
    fs.mtimes.insert(root.join("b.mp3"), tie + Duration::from_secs(1));
    assert_eq!(scan_library(&[root.clone()], &fs).playlists["Unsorted"], vec![root.join("b.mp3"), root.join("A.mp3"), root.join("z.mp3")]);
}

#[test]
fn reports_missing_and_inaccessible_roots_without_content() {
    let missing = PathBuf::from("/missing");
    let denied = PathBuf::from("/denied");
    let mut fs = FakeFs::default();
    fs.dirs.insert(denied.clone(), Err("permission denied".into()));
    let result = scan_library(&[missing.clone(), denied.clone()], &fs);
    assert_eq!(result.roots, vec![(missing, RootStatus::Missing), (denied, RootStatus::Inaccessible)]);
    assert!(result.playlists.is_empty());
    assert_eq!(result.errors.len(), 2);
}

#[test]
fn preserves_child_scan_errors_instead_of_emptying_playlist_silently() {
    let root = PathBuf::from("/music");
    let playlist = root.join("Broken");
    let mut fs = FakeFs::default();
    fs.dirs.insert(root.clone(), Ok(vec![entry(&root, "Broken", true)]));
    fs.dirs.insert(playlist, Err("permission denied".into()));
    let result = scan_library(&[root], &fs);
    assert_eq!(result.playlists["Broken"], Vec::<PathBuf>::new());
    assert_eq!(result.errors[0].playlist.as_deref(), Some("Broken"));
    assert!(result.errors[0].message.contains("permission"));
}

#[test]
fn index_coalesces_refreshes_and_rejects_stale_publication() {
    let mut index = LibraryIndex::default();
    let first = match index.request_refresh() { RefreshRequest::Started { generation } => generation, _ => panic!() };
    assert_eq!(index.request_refresh(), RefreshRequest::Coalesced { generation: first });
    let root = PathBuf::from("/music");
    let fs = FakeFs::default();
    assert!(!index.scan(first - 1, &[root.clone()], &fs));
    assert!(index.scan(first, &[root], &fs));
    assert_eq!(index.snapshot().generation, first);
    assert!(index.take_pending());
}
