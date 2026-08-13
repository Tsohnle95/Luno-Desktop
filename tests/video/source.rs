use luno_desktop_lib::video::source::*;
use luno_desktop_lib::video::staging::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("luno-video-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    path
}

#[test]
fn lookup_uses_three_forms_and_deduplicates_canonical_ids() {
    let mut queries = Vec::new();
    let results = lookup(Some("The Artist"), "A Song", |query| {
        queries.push(query.to_owned());
        match queries.len() {
            1 => vec![VideoCandidate { id: "same".into(), title: "A Song".into(), artist: Some("The Artist".into()), url: "bad".into() }],
            2 => vec![VideoCandidate { id: "same".into(), title: "A Song".into(), artist: Some("The Artist".into()), url: "bad".into() }, VideoCandidate { id: "new".into(), title: "A Song live".into(), artist: Some("The Artist".into()), url: "bad".into() }],
            _ => vec![VideoCandidate { id: "url-id".into(), title: "A Song".into(), artist: Some("The Artist".into()), url: "https://youtu.be/url-id".into() }],
        }
    });
    assert_eq!(queries, ["The Artist - A Song", "A Song The Artist", "A Song"]);
    assert_eq!(results.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["same", "new", "url-id"]);
    assert_eq!(results[0].url, "https://www.youtube.com/watch?v=same");
}

#[test]
fn lookup_rejects_irrelevant_title_or_artist() {
    let results = lookup(Some("Artist"), "Song", |_| vec![
        VideoCandidate { id: "wrong-title".into(), title: "Other".into(), artist: Some("Artist".into()), url: String::new() },
        VideoCandidate { id: "wrong-artist".into(), title: "Song".into(), artist: Some("Other".into()), url: String::new() },
    ]);
    assert!(results.is_empty());
}

#[test]
fn staging_adopts_existing_and_publishes_without_library_visibility() {
    let root = temp("stage");
    let library = temp("library");
    let mut stage = VideoStage::at(&root, Some(&library)).unwrap();
    let generation = stage.begin_generation();
    let existing = root.join("existing.mp4");
    fs::write(&existing, b"mp4").unwrap();
    assert_eq!(stage.adopt(Slot::Current, generation, &existing).unwrap().state, StageState::Ready);
    let ready = stage.stage(Slot::Next, "song/key", generation, &AtomicBool::new(false), |out| { out.write_all(b"video").map_err(|e| e.to_string()) }).unwrap();
    assert_eq!(ready.state, StageState::Ready);
    let path = ready.path.unwrap();
    assert!(path.starts_with(&root));
    assert!(!path.starts_with(&library));
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(library);
}

#[test]
fn stale_and_cancelled_staging_do_not_leave_partials() {
    let root = temp("cancel");
    let mut stage = VideoStage::at(&root, None).unwrap();
    let generation = stage.begin_generation();
    let cancelled = AtomicBool::new(true);
    assert_eq!(stage.stage(Slot::Current, "x", generation, &cancelled, |_| Ok(())).unwrap_err(), StageError::Cancelled);
    let newer = stage.begin_generation();
    assert_eq!(stage.stage(Slot::Current, "x", generation, &AtomicBool::new(false), |_| Ok(())).unwrap_err(), StageError::StaleGeneration);
    assert_eq!(stage.unavailable(Slot::Next, newer).state, StageState::Unavailable);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stage_root_cannot_be_library_or_child() {
    let library = temp("library-root");
    assert_eq!(VideoStage::at(&library, Some(&library)).unwrap_err(), StageError::InvalidRoot);
    let child = library.join("previews");
    assert_eq!(VideoStage::at(&child, Some(&library)).unwrap_err(), StageError::InvalidRoot);
    let _ = fs::remove_dir_all(library);
}
