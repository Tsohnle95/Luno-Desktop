use std::collections::HashMap;
use reqwest::blocking::Client;
use serde_json::Value;

pub const DESIGN_COUNT: u32 = 75;

/// Artwork lookup used when Last.fm has no usable image. `None` is cached so
/// repeated missing-art rows do not repeatedly hit Deezer.
pub struct ArtworkService {
    base_url: String,
    client: Client,
    cache: HashMap<String, Option<String>>,
}

impl ArtworkService {
    pub fn new() -> Self { Self::with_base_url("https://api.deezer.com/search") }
    pub fn with_base_url(base_url: impl Into<String>) -> Self { Self { base_url: base_url.into(), client: Client::new(), cache: HashMap::new() } }
    pub fn find_artwork(&mut self, artist: &str, title: &str) -> Option<String> {
        let key = format!("{}|{}", artist.trim().to_lowercase(), title.trim().to_lowercase());
        if let Some(value) = self.cache.get(&key) { return value.clone(); }
        let result = self.lookup(artist, title);
        self.cache.insert(key, result.clone());
        result
    }
    fn lookup(&self, artist: &str, title: &str) -> Option<String> {
        for query in [format!("\"{artist}\" \"{title}\""), format!("{artist} {title}")] {
            let response = match self.client.get(&self.base_url).query(&[("q", query), ("limit", "1".into())]).send().and_then(|r| r.error_for_status()).and_then(|r| r.json::<Value>()) { Ok(value) => value, Err(_) => continue };
            let Some(album) = response.get("data").and_then(Value::as_array).and_then(|v| v.first()).and_then(|v| v.get("album")) else { continue };
            let raw = ["cover_xl", "cover_big", "cover_medium"].iter().find_map(|key| album.get(*key).and_then(Value::as_str)).unwrap_or("").trim();
            if !raw.is_empty() { return Some(normalize_url(raw)); }
        }
        None
    }
    pub fn clear_cache(&mut self) { self.cache.clear(); }
}

fn normalize_url(url: &str) -> String { url.strip_prefix("http://").map_or_else(|| url.to_owned(), |rest| format!("https://{rest}")) }

/// Stable abstract-cover assignment. The returned index is suitable for a
/// renderer in any client and is invariant across processes.
pub fn design_index(artist: &str, title: &str) -> u32 { crc32fast::hash(&crate::discovery::service::normalized_key(artist, title).into_bytes()) % DESIGN_COUNT }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Design { pub index: u32, pub size: u32, pub start: (u8, u8, u8), pub end: (u8, u8, u8) }

/// Returns deterministic design metadata; the presentation layer draws the
/// gradient/geometric overlay without requiring a native image dependency.
pub fn render_design(index: u32, size: u32) -> Design {
    const PALETTES: [(u8, u8, u8, u8, u8, u8); 12] = [
        (18,24,22,30,215,96), (26,18,24,138,92,232), (18,22,30,79,168,255),
        (28,22,16,255,159,67), (22,26,30,79,216,208), (30,20,20,255,107,107),
        (20,26,20,155,232,92), (22,20,28,216,143,255), (20,26,28,107,208,255),
        (28,26,20,255,208,107), (24,18,24,255,119,200), (18,24,26,119,255,200),
    ];
    let p = PALETTES[(index as usize) % PALETTES.len()];
    Design { index: index % DESIGN_COUNT, size, start: (p.0, p.1, p.2), end: (p.3, p.4, p.5) }
}
