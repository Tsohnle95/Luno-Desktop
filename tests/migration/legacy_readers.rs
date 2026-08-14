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
    assert!(!has_legacy_data(&dir, Some(&dir.join(".luno_previews"))));
    fs::write(music.join(".luno_allowed_dups.json"), "[\"track\"]").unwrap();
    assert!(has_legacy_data(&dir, None));
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

#[cfg(unix)]
#[test]
fn dangling_preview_symlink_is_a_migration_trigger_for_main_gating() {
    use std::os::unix::fs::symlink;
    let dir = tempdir();
    let preview = dir.join(".luno_previews");
    symlink(dir.join("missing"), &preview).unwrap();
    assert!(has_legacy_data(&dir, Some(&preview)));
    fs::remove_dir_all(dir).unwrap();
}
