//! Real-helper end-to-end download verification (gate 4 boundary).
//! Uses the actual bundled luno-download-helper + yt-dlp + ffmpeg against a
//! temp library root, exactly like production start_download does.

use crate::contracts::events::{Command, CommandEnvelope, Event};
use crate::contracts::types::JobState;
use crate::state::AppState;
use std::path::PathBuf;
use std::time::Duration;

fn helper_path() -> Option<PathBuf> {
    let dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let p = PathBuf::from(dir).join("resources").join("bin").join("luno-download-helper");
    if p.is_file() { Some(p) } else { None }
}

#[test]
fn real_helper_download_lands_in_unsorted_and_publishes_job_events() {
    let Some(helper) = helper_path() else {
        eprintln!("SKIP: luno-download-helper not found (resources/bin missing)");
        return;
    };
    let root = std::env::temp_dir().join(format!("luno-gate4-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let unsorted = root.join("Unsorted");
    std::fs::create_dir_all(&unsorted).unwrap();
    let state = AppState::new(8);
    state.set_scan_roots(vec![root.clone()]);
    state.set_bundle_helper(Some(helper.clone()));

    // Exact frontend flow: search -> take the provider query -> start download
    let search = state
        .dispatch(CommandEnvelope {
            version: 1,
            request_id: "real-search".into(),
            command: Command::SearchDownloader {
                query: "spaceboy smashing pumpkins".into(),
                try_soundcloud: false,
                limit: 10,
            },
        })
        .expect("search dispatch");
    let sources = search.as_array().expect("search returns array");
    assert!(!sources.is_empty(), "real search returned no results");
    let first = &sources[0];
    // Rich metadata now flows to the UI: real title/artist/url, not a raw query.
    let title = first["title"].as_str().expect("result title").to_owned();
    assert!(!title.is_empty(), "result title must be non-empty");
    assert!(first["artist"].as_str().is_some(), "result artist must be present");
    let url = first["url"].as_str().expect("result url").to_owned();
    assert!(url.starts_with("https://"), "result url must be absolute: {url}");
    let query = first["query"].as_str().unwrap().to_owned();
    assert!(query.starts_with("ytsearch1:"), "query should be a yt search: {query}");
    eprintln!("REAL SEARCH OK: {title} ({url})");

    let job_id = "real-dl-job";
    let destination = unsorted.join(format!("{title} - Test.m4a"));
    state
        .dispatch(CommandEnvelope {
            version: 1,
            request_id: "real-download".into(),
            command: Command::StartDownload {
                job_id: crate::contracts::types::JobId(job_id.to_owned()),
                destination: destination.to_string_lossy().into_owned(),
                helper: helper.to_string_lossy().into_owned(),
                source: query.clone(),
            },
        })
        .expect("start download");

    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    let mut saw_succeeded = false;
    let mut last = String::new();
    while std::time::Instant::now() < deadline {
        let jobs: Vec<_> = state
            .events()
            .iter()
            .filter_map(|e| match &e.event {
                Event::JobUpdated { job } if job.id.0 == job_id => Some(job.clone()),
                _ => None,
            })
            .collect();
        if let Some(job) = jobs.last() { last = format!("{:?}", job.state); }
        if jobs.iter().any(|j| j.state == JobState::Succeeded) { saw_succeeded = true; break; }
        if jobs.iter().any(|j| j.state == JobState::Failed || j.state == JobState::Cancelled) {
            panic!("download job failed: {last:?}");
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    assert!(saw_succeeded, "job never reached succeeded; last={last}");
    assert!(destination.is_file(), "file did not land at {destination:?}");
    let size = std::fs::metadata(&destination).unwrap().len();
    assert!(size > 100_000, "downloaded file suspiciously small: {size}");

    // The download completion must trigger a library rescan so the new file
    // appears in the app immediately (the "nothing appeared in Unsorted" bug).
    let scan_deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut saw_scan = false;
    while std::time::Instant::now() < scan_deadline {
        if state.events().iter().any(|e| matches!(&e.event, Event::LibrarySnapshot { .. })) {
            saw_scan = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    assert!(saw_scan, "download completion must trigger a library rescan");
    eprintln!("REAL DOWNLOAD OK: {size} bytes at {destination:?}; auto-rescan observed");
    let _ = std::fs::remove_dir_all(&root);
}
