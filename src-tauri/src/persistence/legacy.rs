//! Read-only codecs for the Python application's legacy persistence files.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct LegacyConfig {
    pub music_root: Option<String>,
    pub lastfm_key: Option<String>,
    pub auto_discovery: Option<bool>,
    pub volume: Option<f64>,
    pub sleep_timer: Option<Value>,
    pub global_keys: Option<bool>,
    pub favorite_songs: Option<Vec<String>>,
    pub video_mode: Option<bool>,
    pub youtube_cookies_browser: Option<String>,
}

/// Parse config while retaining only known fields. A malformed or non-object
/// file is an error and never causes a write or replacement of the input.
pub fn read_config(path: &Path) -> Result<LegacyConfig, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let object = value.as_object().ok_or_else(|| "config must be a JSON object".to_string())?;
    let mut config = LegacyConfig::default();
    macro_rules! string { ($name:literal, $field:ident) => { if let Some(v) = object.get($name) { if let Some(s) = v.as_str() { config.$field = Some(s.to_owned()); } } }; }
    string!("music_root", music_root); string!("lastfm_key", lastfm_key); string!("youtube_cookies_browser", youtube_cookies_browser);
    if let Some(v) = object.get("auto_discovery").and_then(Value::as_bool) { config.auto_discovery = Some(v); }
    if let Some(v) = object.get("global_keys").and_then(Value::as_bool) { config.global_keys = Some(v); }
    if let Some(v) = object.get("video_mode").and_then(Value::as_bool) { config.video_mode = Some(v); }
    if let Some(v) = object.get("volume").and_then(Value::as_f64).filter(|v| v.is_finite()) { config.volume = Some(v); }
    // `crossfade` was intentionally retired in the desktop migration. It is
    // not represented in target state and is therefore ignored as an unknown
    // legacy field.
    config.sleep_timer = object.get("sleep_timer").cloned();
    if let Some(values) = object.get("favorite_songs").and_then(Value::as_array) {
        config.favorite_songs = Some(values.iter().filter_map(Value::as_str).map(str::to_owned).collect());
    }
    Ok(config)
}

pub fn read_string_map(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let value: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let object = value.as_object().ok_or_else(|| "expected a JSON object".to_string())?;
    Ok(object.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned()))).collect())
}

pub fn read_string_set(path: &Path) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let values = value.as_array().ok_or_else(|| "expected a JSON array".to_string())?;
    Ok(values.iter().filter_map(Value::as_str).map(str::to_owned).collect())
}
