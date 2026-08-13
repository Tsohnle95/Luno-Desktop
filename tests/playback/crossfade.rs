use luno_desktop_lib::persistence::legacy::read_config;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn approved_removal_ignores_legacy_crossfade_without_rewriting_config() {
    let path = std::env::temp_dir().join(format!(
        "luno-crossfade-removal-{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    let input = br#"{"crossfade":12,"volume":0.75}"#;
    fs::write(&path, input).unwrap();

    let config = read_config(&path).unwrap();

    assert_eq!(config.volume, Some(0.75));
    assert_eq!(config.sleep_timer, None);
    assert_eq!(fs::read(&path).unwrap(), input);
    fs::remove_file(path).unwrap();
}
