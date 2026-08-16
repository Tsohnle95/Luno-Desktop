use crate::discovery::artwork::{design_index, render_design, ArtworkService};
use crate::discovery::service::{DiscoveryResult, DiscoveryService};
use crate::library::scanner::{scan_library, OsFileSystem};
use crate::preview::assets::PreviewAssetStore;
use crate::preview::save::{promote_to_playlist, PlaylistDestination};
use crate::video::source::{lookup, VideoCandidate, VideoSource};
use crate::video::staging::{Slot, VideoStage};
use serde::Serialize;
use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

#[derive(Debug, Serialize)]
pub struct Gate5SmokeResult {
    pub discovery_items: usize,
    pub discovery_artwork: bool,
    pub artwork_design: u32,
    pub preview_ready: bool,
    pub preview_hidden_before_save: bool,
    pub saved_visible_after_scan: bool,
    pub video_ready: bool,
    pub unobserved: Vec<&'static str>,
}

fn fixture_server(body: serde_json::Value) -> Result<String, String> {
    let listener =
        TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("loopback fixture bind: {e}"))?;
    let address = listener
        .local_addr()
        .map_err(|e| format!("loopback fixture address: {e}"))?;
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            if let Ok(body) = serde_json::to_string(&body) {
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        }
    });
    Ok(format!("http://{address}/fixture"))
}

fn within(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

fn first_video_source(sources: &[VideoSource]) -> Result<&VideoSource, String> {
    sources
        .first()
        .ok_or_else(|| "video fixture returned no candidates".to_string())
}

fn validate_root(path: &Path, temp_root: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute()
        || path.as_os_str().is_empty()
        || path.to_string_lossy().chars().any(char::is_control)
    {
        return Err("fixture roots must be absolute and free of control characters".into());
    }
    if path.exists() {
        return Err("fixture roots must not already exist".into());
    }
    let existing_parent = path
        .ancestors()
        .find(|candidate| candidate.exists())
        .ok_or_else(|| "fixture roots must have an existing parent".to_string())?;
    let canonical_parent =
        fs::canonicalize(existing_parent).map_err(|e| format!("fixture root parent: {e}"))?;
    if !canonical_parent.starts_with(temp_root) {
        return Err("fixture roots must be under the system temporary directory".into());
    }
    Ok(canonical_parent.join(
        path.strip_prefix(existing_parent)
            .map_err(|_| "fixture root path normalization failed".to_string())?,
    ))
}

fn validate_roots(
    root: &Path,
    preview_root: &Path,
    video_root: &Path,
) -> Result<[PathBuf; 3], String> {
    let temp_root = fs::canonicalize(std::env::temp_dir())
        .map_err(|e| format!("fixture temp directory: {e}"))?;
    let roots = [
        validate_root(root, &temp_root)?,
        validate_root(preview_root, &temp_root)?,
        validate_root(video_root, &temp_root)?,
    ];
    if roots.iter().enumerate().any(|(index, path)| {
        roots.iter().enumerate().any(|(other_index, other)| {
            index != other_index && (within(path, other) || within(other, path))
        })
    }) {
        return Err("fixture roots must be separate and non-overlapping".into());
    }
    Ok(roots)
}

pub fn run(
    root: PathBuf,
    preview_root: PathBuf,
    video_root: PathBuf,
) -> Result<Gate5SmokeResult, String> {
    let [root, preview_root, video_root] = validate_roots(&root, &preview_root, &video_root)?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    fs::create_dir_all(&preview_root).map_err(|e| e.to_string())?;
    fs::create_dir_all(&video_root).map_err(|e| e.to_string())?;

    let discovery_url = fixture_server(
        json!({"similartracks":{"track":[{"name":"fixture-track","artist":{"name":"fixture-artist"},"match":0.9,"image":[{"size":"mega","#text":"http://img.test/fixture.jpg"}]}]}}),
    )?;
    let mut discovery =
        DiscoveryService::with_base_url(Some("synthetic-key".into()), discovery_url);
    let items = match discovery.get_similar("fixture-artist", "seed", 5, &[]) {
        DiscoveryResult::Success(items) => items,
        other => return Err(format!("discovery fixture failed: {other:?}")),
    };
    let artwork = items
        .first()
        .and_then(|item| item.artwork_url.as_deref())
        .is_some();

    let artwork_url =
        fixture_server(json!({"data":[{"album":{"cover_xl":"http://img.test/cover.jpg"}}]}))?;
    let mut artwork_service = ArtworkService::with_base_url(artwork_url);
    let _ = artwork_service.find_artwork("fixture-artist", "fixture-track");
    let design = render_design(design_index("fixture-artist", "fixture-track"), 96).index;

    let library_before = scan_library(std::slice::from_ref(&root), &OsFileSystem).playlists;
    let mut assets =
        PreviewAssetStore::at(&preview_root).map_err(|e| format!("preview store: {e:?}"))?;
    let generation = assets.begin_generation();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let preview = assets
        .download(
            "fixture-artist-fixture-track",
            generation,
            &cancel,
            |writer| {
                writer
                    .write_all(b"ID3 synthetic preview")
                    .map_err(|e| e.to_string())
            },
        )
        .map_err(|e| format!("preview stage: {e:?}"))?;
    let hidden_before_save = !library_before
        .values()
        .flatten()
        .any(|path| path == &preview);

    let playlist = root.join("Fixture Playlist");
    let saved = promote_to_playlist(
        &preview,
        "fixture-artist - fixture-track.mp3",
        Some(&PlaylistDestination {
            playlist_id: "fixture".into(),
            directory: playlist.clone(),
        }),
    )
    .map_err(|_| "preview promotion failed".to_string())?;
    let visible_after_save = scan_library(std::slice::from_ref(&root), &OsFileSystem)
        .playlists
        .values()
        .flatten()
        .any(|path| path == &saved.path);

    let candidates = |query: &str| {
        vec![VideoCandidate {
            id: "fixture-video".into(),
            title: format!("{query} fixture-track"),
            artist: Some("fixture-artist".into()),
            url: "https://youtu.be/fixture-video".into(),
        }]
    };
    let sources = lookup(Some("fixture-artist"), "fixture-track", candidates);
    let source = first_video_source(&sources)?;
    let mut video =
        VideoStage::at(&video_root, Some(&root)).map_err(|e| format!("video store: {e:?}"))?;
    let video_generation = video.begin_generation();
    let staged = video
        .stage(
            Slot::Current,
            &source.id,
            video_generation,
            &cancel,
            |writer| {
                writer
                    .write_all(b"ftyp synthetic mp4")
                    .map_err(|e| e.to_string())
            },
        )
        .map_err(|e| format!("video stage: {e:?}"))?;

    Ok(Gate5SmokeResult {
        discovery_items: items.len(),
        discovery_artwork: artwork,
        artwork_design: design,
        preview_ready: preview.is_file(),
        preview_hidden_before_save: hidden_before_save,
        saved_visible_after_scan: visible_after_save,
        video_ready: staged.path.is_some(),
        unobserved: vec![
            "manual Discover/Song Page rendering",
            "APIC/Tk artwork decoding",
            "FFmpeg/process/native audio clock",
            "external provider/video and wall-clock deadlines",
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::{design_index, first_video_source, run, validate_roots};
    use std::path::{Path, PathBuf};

    struct FixtureRoots([PathBuf; 3]);

    impl Drop for FixtureRoots {
        fn drop(&mut self) {
            for root in &self.0 {
                let _ = std::fs::remove_dir_all(root);
            }
        }
    }

    #[test]
    fn fixture_roots_must_be_separate_temp_children() {
        let temp = std::env::temp_dir();
        let root = temp.join("luno-gate5-validation-library");
        let preview = temp.join("luno-gate5-validation-preview");
        let video = temp.join("luno-gate5-validation-video");
        for path in [&root, &preview, &video] {
            let _ = std::fs::remove_dir_all(path);
        }
        assert!(validate_roots(&root, &preview, &video).is_ok());
        assert!(validate_roots(&root, &root.join("preview"), &video).is_err());
        assert!(validate_roots(&preview.join("library"), &preview, &video).is_err());
        assert!(validate_roots(
            &temp.join("luno-gate5-validation-preview/../luno-gate5-validation-preview/library"),
            &preview,
            &video
        )
        .is_err());
        assert!(validate_roots(Path::new("relative"), &preview, &video).is_err());
    }

    #[test]
    fn empty_video_sources_are_reported_without_indexing() {
        assert!(first_video_source(&[]).is_err());
    }

    #[test]
    fn run_executes_isolated_synthetic_gate5_pipeline() {
        let temp = std::env::temp_dir();
        let process_scope = std::process::id();
        let roots = FixtureRoots([
            temp.join(format!("luno-gate5-run-{process_scope}-library")),
            temp.join(format!("luno-gate5-run-{process_scope}-preview")),
            temp.join(format!("luno-gate5-run-{process_scope}-video")),
        ]);
        for root in &roots.0 {
            let _ = std::fs::remove_dir_all(root);
        }

        let result = run(roots.0[0].clone(), roots.0[1].clone(), roots.0[2].clone())
            .expect("synthetic Gate 5 seam should complete");

        assert_eq!(result.discovery_items, 1);
        assert!(result.discovery_artwork);
        assert_eq!(
            result.artwork_design,
            design_index("fixture-artist", "fixture-track")
        );
        assert!(result.preview_ready);
        assert!(result.preview_hidden_before_save);
        assert!(result.saved_visible_after_scan);
        assert!(result.video_ready);
        assert!(result
            .unobserved
            .contains(&"manual Discover/Song Page rendering"));
        assert!(result
            .unobserved
            .contains(&"FFmpeg/process/native audio clock"));
    }
}
