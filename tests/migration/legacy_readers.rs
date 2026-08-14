use luno_desktop_lib::migration::legacy_readers::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn tempdir() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("luno-legacy-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn partial_corrupt_inputs_are_warning_only_and_read_only() {
    let dir = tempdir();
    let config = dir.join(".luno_config.json");
    fs::write(&config, br#"{"volume":0.5,"favorite_songs":["/music/a",7]}"#).unwrap();
    fs::write(dir.join(".luno_allowed_dups.json"), br#"["abc",3,"def"]"#).unwrap();
    fs::write(dir.join(".downloaded.txt"), "old title VID1\nmalformed\n").unwrap();
    fs::write(dir.join(".luno_playlist_urls.json"), b"not json").unwrap();
    let state = read_legacy(&dir, None);
    assert_eq!(state.config.unwrap().volume, Some(0.5));
    assert_eq!(state.allowed_duplicate_keys.len(), 2);
    assert!(state.downloaded_ids.contains("VID1"));
    assert!(!state.warnings.is_empty());
    assert_eq!(fs::read(&config).unwrap(), br#"{"volume":0.5,"favorite_songs":["/music/a",7]}"#);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn preview_inventory_is_sorted_and_contained() {
    let dir = tempdir();
    fs::write(dir.join("b.mp3"), b"x").unwrap();
    fs::write(dir.join("a.mp3"), b"x").unwrap();
    let files = read_preview_inventory(&dir).unwrap();
    assert_eq!(files[0].file_name().unwrap(), "a.mp3");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn legacy_detection_covers_configured_root_and_ignores_empty_surfaces() {
    let dir = tempdir();
    let music = dir.join("music");
    fs::create_dir_all(&music).unwrap();
    fs::write(dir.join(".luno_config.json"), format!(r#"{{"music_root":"{}"}}"#, music.display())).unwrap();
    fs::write(music.join(".downloaded_vids"), "").unwrap();
    assert!(!has_legacy_data(&dir, Some(&dir.join(".luno_previews"))).unwrap());
    fs::write(music.join(".luno_allowed_dups.json"), "[\"track\"]").unwrap();
    assert!(has_legacy_data(&dir, None).unwrap());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn retry_inventory_reports_unknown_files_without_recursing_home() {
    let root = tempdir();
    let music = root.join("music");
    fs::create_dir_all(music.join("playlist")).unwrap();
    fs::write(root.join("unknown-home-input"), b"preserved").unwrap();
    fs::write(music.join("playlist/.downloaded_vids"), b"id\n").unwrap();
    fs::write(music.join("playlist/unknown-playlist-input"), b"preserved").unwrap();
    fs::create_dir_all(music.join("unmarked")).unwrap();
    fs::write(music.join("unmarked/should-not-be-scanned"), b"ignored").unwrap();
    let files = inventory_retry_files(&root, Some(&music), &root.join(".luno_previews"), &root.join("target")).unwrap();
    assert_eq!(files, vec![root.join("unknown-home-input"), music.join("playlist/.downloaded_vids"), music.join("playlist/unknown-playlist-input")]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn retry_inventory_budget_is_reserved_for_preview_and_music_roots() {
    let root = tempdir();
    let music = root.join("music");
    let preview = root.join(".luno_previews");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&preview).unwrap();
    for index in 0..(MAX_RETRY_ENTRIES / 4) {
        fs::write(music.join(format!("track-{index}")), b"x").unwrap();
        fs::write(preview.join(format!("preview-{index}")), b"x").unwrap();
    }
    let files = inventory_retry_files(&root, Some(&music), &preview, &root.join("target")).unwrap();
    assert!(files.iter().any(|path| path.starts_with(&music)));
    assert!(files.iter().any(|path| path.starts_with(&preview)));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn retry_inventory_rejects_oversized_preview_without_unbounded_listing() {
    let root = tempdir();
    let preview = root.join(".luno_previews");
    fs::create_dir_all(&preview).unwrap();
    for index in 0..=(MAX_RETRY_ENTRIES / 2) {
        fs::write(preview.join(format!("preview-{index}")), b"x").unwrap();
    }
    let error = inventory_retry_files(&root, None, &preview, &root.join("target")).unwrap_err();
    assert!(error.contains("retry inspection exceeded"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_detection_rejects_oversized_preview_without_unbounded_listing() {
    let root = tempdir();
    let preview = root.join(".luno_previews");
    fs::create_dir_all(&preview).unwrap();
    for index in 0..=MAX_RETRY_ENTRIES {
        fs::write(preview.join(format!("preview-{index}")), b"x").unwrap();
    }
    assert!(has_legacy_data(&root, Some(&preview)).unwrap());
    let state = read_legacy(&root, Some(&preview));
    assert_eq!(state.preview_files, Vec::<std::path::PathBuf>::new());
    assert!(state.warnings.iter().any(|warning| warning.message.contains("preview inventory exceeded")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn nested_marker_inventory_is_bounded_sorted_and_reports_overflow() {
    let root = tempdir();
    let music = root.join("music");
    fs::create_dir(&music).unwrap();
    fs::write(root.join(".luno_config.json"), format!(r#"{{"music_root":"{}"}}"#, music.display())).unwrap();
    for index in 0..(MAX_RETRY_ENTRIES / 2 + 1) {
        let directory = music.join(format!("playlist-{index:04}"));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join(if index == MAX_RETRY_ENTRIES / 2 { ".downloaded.txt" } else { ".downloaded_vids" }), if index == MAX_RETRY_ENTRIES / 2 { "old title VID1\n" } else { "VID2\n" }).unwrap();
    }
    assert!(marker_files(&root).unwrap_err().contains("legacy marker inspection exceeded"));
    assert!(has_legacy_data(&root, None).unwrap());
    let state = read_legacy(&root, None);
    assert!(state.downloaded_ids.contains("VID1"));
    assert!(state.warnings.iter().any(|warning| warning.message.contains("legacy marker inspection exceeded")));
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn nested_marker_inventory_does_not_follow_symlink_directories() {
    use std::os::unix::fs::symlink;
    let root = tempdir();
    let outside = tempdir();
    fs::write(outside.join(".downloaded_vids"), "OUTSIDE\n").unwrap();
    symlink(&outside, root.join("linked")).unwrap();
    assert!(marker_files(&root).unwrap().is_empty());
    assert!(!has_legacy_data(&root, None).unwrap());
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[cfg(unix)]
#[test]
fn dangling_preview_symlink_is_a_migration_trigger_for_main_gating() {
    use std::os::unix::fs::symlink;
    let dir = tempdir();
    let preview = dir.join(".luno_previews");
    symlink(dir.join("missing"), &preview).unwrap();
    assert!(has_legacy_data(&dir, Some(&preview)).unwrap());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn configured_music_root_outside_scope_is_rejected_before_scan() {
    let root = tempdir();
    let outside = tempdir();
    fs::write(outside.join(".downloaded_vids"), "OUTSIDE\n").unwrap();
    fs::write(root.join(".luno_config.json"), format!(r#"{{"music_root":"{}"}}"#, outside.display())).unwrap();
    let error = has_legacy_data(&root, None).unwrap_err();
    assert!(error.contains("outside the legacy discovery root"));
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn read_legacy_resolves_relative_configured_root() {
    let root = tempdir();
    let music = root.join("Music");
    fs::create_dir_all(&music).unwrap();
    fs::write(root.join(".luno_config.json"), r#"{"music_root":"Music"}"#).unwrap();
    fs::write(music.join(".luno_allowed_dups.json"), r#"["relative"]"#).unwrap();
    let state = read_legacy(&root, None);
    assert!(state.allowed_duplicate_keys.contains("relative"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn public_reader_rejects_unsafe_music_root_before_scanning() {
    let root = tempdir();
    let outside = tempdir();
    fs::write(outside.join(".luno_allowed_dups.json"), r#"["outside"]"#).unwrap();
    let state = read_legacy_with_music_root(&root, None, Some(&outside));
    assert!(state.allowed_duplicate_keys.is_empty());
    assert!(state.warnings.iter().any(|warning| warning.message.contains("outside the legacy discovery root")));
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn marker_traversal_stops_at_max_depth() {
    let root = tempdir();
    let mut current = root.clone();
    for _ in 0..=MAX_RETRY_DEPTH {
        current.push("nested");
        fs::create_dir(&current).unwrap();
    }
    fs::write(current.join(".downloaded_vids"), "TOO-DEEP\n").unwrap();
    assert!(marker_files(&root).unwrap().is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn legacy_readers_reject_symlinked_typed_files() {
    use std::os::unix::fs::symlink;
    let root = tempdir();
    let outside = tempdir();
    fs::write(outside.join("config"), r#"{"volume":0.5,"favorite_songs":["outside"]}"#).unwrap();
    fs::write(outside.join("urls"), r#"{"playlist":"https://outside"}"#).unwrap();
    fs::write(outside.join("dups"), r#"["outside"]"#).unwrap();
    fs::write(outside.join("markers"), "OUTSIDE\n").unwrap();
    symlink(outside.join("config"), root.join(".luno_config.json")).unwrap();
    symlink(outside.join("urls"), root.join(".luno_playlist_urls.json")).unwrap();
    symlink(outside.join("dups"), root.join(".luno_allowed_dups.json")).unwrap();
    symlink(outside.join("markers"), root.join(".downloaded.txt")).unwrap();

    let state = read_legacy(&root, None);
    assert!(state.config.is_none());
    assert!(state.playlist_urls.is_empty());
    assert!(state.allowed_duplicate_keys.is_empty());
    assert!(state.downloaded_ids.is_empty());

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}
