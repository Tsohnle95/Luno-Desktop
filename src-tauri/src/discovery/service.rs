use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

const MAX_CACHE: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    pub artist: String,
    pub title: String,
    pub match_score: f64,
    pub artwork_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryError { MissingApiKey, Request(String) }

#[derive(Debug, Clone, PartialEq)]
pub enum DiscoveryResult { Success(Vec<Recommendation>), Error(DiscoveryError) }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey { artist: String, title: String, limit: usize }

/// Last.fm client. The cache deliberately stores only successful non-empty results.
pub struct DiscoveryService {
    api_key: Option<String>,
    base_url: String,
    client: Client,
    cache: HashMap<CacheKey, Vec<Recommendation>>,
    order: VecDeque<CacheKey>,
    generation: AtomicU64,
}

impl DiscoveryService {
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base_url(api_key, "https://ws.audioscrobbler.com/2.0/")
    }
    pub fn with_base_url(api_key: Option<String>, base_url: impl Into<String>) -> Self {
        Self { api_key, base_url: base_url.into(), client: Self::client(), cache: HashMap::new(), order: VecDeque::new(), generation: AtomicU64::new(0) }
    }
    fn client() -> Client {
        Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("discovery HTTP client")
    }
    pub fn set_api_key(&mut self, key: Option<String>) { self.api_key = key; self.clear_cache(); }
    pub fn clear_cache(&mut self) { self.cache.clear(); self.order.clear(); }
    pub fn begin_request(&self) -> u64 { self.generation.fetch_add(1, Ordering::AcqRel) + 1 }
    pub fn is_current(&self, generation: u64) -> bool { self.generation.load(Ordering::Acquire) == generation }

    pub fn get_similar(&mut self, artist: &str, title: &str, limit: usize, library: &[Recommendation]) -> DiscoveryResult {
        let key = CacheKey { artist: artist.trim().to_lowercase(), title: title.trim().to_lowercase(), limit };
        if self.api_key.as_deref().map(str::trim).unwrap_or("").is_empty() { return DiscoveryResult::Error(DiscoveryError::MissingApiKey); }
        if key.artist.is_empty() || key.title.is_empty() { return DiscoveryResult::Success(Vec::new()); }
        if let Some(items) = self.cache.get(&key).cloned() { return DiscoveryResult::Success(filter_recommendations(items, library)); }
        let mut items = match self.call("track.getsimilar", artist, Some(title), limit) {
            Ok(v) => v,
            Err(e) if Self::is_not_found(&e) => Vec::new(),
            Err(e) => return DiscoveryResult::Error(e),
        };
        if items.is_empty() {
            items = match self.call("artist.gettoptracks", artist, None, limit) {
                Ok(v) => v.into_iter().map(|mut r| { r.match_score = 0.8; r }).collect(),
                Err(e) if Self::is_not_found(&e) => Vec::new(),
                Err(e) => return DiscoveryResult::Error(e),
            };
        }
        items = deduplicate(items);
        if !items.is_empty() { self.store(key, items.clone()); }
        DiscoveryResult::Success(filter_recommendations(items, library))
    }

    fn is_not_found(error: &DiscoveryError) -> bool {
        if let DiscoveryError::Request(message) = error {
            let lower = message.to_lowercase();
            lower.contains("not found") || lower.contains("could not be found") || lower.contains("no such")
        } else {
            false
        }
    }

    fn call(&self, method: &str, artist: &str, title: Option<&str>, limit: usize) -> Result<Vec<Recommendation>, DiscoveryError> {
        let mut params = vec![("method", method.to_owned()), ("artist", artist.to_owned()), ("api_key", self.api_key.clone().unwrap_or_default()), ("format", "json".into()), ("limit", limit.to_string())];
        if let Some(title) = title { params.push(("track", title.to_owned())); }
        let value: serde_json::Value = self.client.get(&self.base_url).query(&params).send().and_then(|r| r.error_for_status()).map_err(|e| DiscoveryError::Request(e.to_string()))?.json().map_err(|e| DiscoveryError::Request(e.to_string()))?;
        if let Some(message) = value.get("message").and_then(|v| v.as_str()).filter(|_| value.get("error").is_some()) { return Err(DiscoveryError::Request(message.to_owned())); }
        let entries = value.get(if method == "track.getsimilar" { "similartracks" } else { "toptracks" }).and_then(|v| v.get("track")).map(as_array).unwrap_or_default();
        Ok(entries.into_iter().filter_map(|entry| {
            let artist = entry.get("artist").and_then(|v| v.get("name").or(Some(v))).and_then(|v| v.as_str())?.trim().to_owned();
            let title = entry.get("name").and_then(|v| v.as_str())?.trim().to_owned();
            if artist.is_empty() || title.is_empty() { return None; }
            Some(Recommendation { artist, title, match_score: entry.get("match").and_then(|v| v.as_f64()).unwrap_or(0.0), artwork_url: pick_image(entry) })
        }).collect())
    }
    fn store(&mut self, key: CacheKey, value: Vec<Recommendation>) {
        if self.cache.len() >= MAX_CACHE { self.cache.clear(); self.order.clear(); }
        self.order.push_back(key.clone()); self.cache.insert(key, value);
    }
}

fn as_array(value: &serde_json::Value) -> Vec<&serde_json::Value> { value.as_array().map(|v| v.iter().collect()).or_else(|| (!value.is_null()).then(|| vec![value])).unwrap_or_default() }
fn pick_image(entry: &serde_json::Value) -> Option<String> {
    let images = entry.get("image").and_then(|v| v.as_array())?;
    for size in ["mega", "extralarge", "large", "medium", "small"] { if let Some(url) = images.iter().find(|x| x.get("size").and_then(|v| v.as_str()) == Some(size)).and_then(image_url) { return Some(url); } }
    images.iter().find_map(image_url)
}
fn image_url(image: &serde_json::Value) -> Option<String> {
    let raw = image.get("#text").or_else(|| image.get("url"))?.as_str()?.trim();
    let url = if let Some(rest) = raw.strip_prefix("http://") { format!("https://{rest}") } else if raw.starts_with("https://") { raw.to_owned() } else { return None };
    let lower = url.to_lowercase();
    if (lower.contains("last.fm") || lower.contains("lastfm.freetls.fastly.net")) && ["/default", "noimage", "no_image", "2a96cbd8b46e442fc41c2b86b821562f"].iter().any(|m| lower.contains(m)) { None } else { Some(url) }
}
pub fn normalized_key(artist: &str, title: &str) -> String {
    let normalize = |value: &str| value.nfkd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    format!("{}|{}", normalize(artist), normalize(title))
}
pub fn deduplicate(items: Vec<Recommendation>) -> Vec<Recommendation> { let mut seen = HashSet::new(); items.into_iter().filter(|r| seen.insert(normalized_key(&r.artist, &r.title))).collect() }
pub fn filter_recommendations(items: Vec<Recommendation>, library: &[Recommendation]) -> Vec<Recommendation> { let known: HashSet<_> = library.iter().map(|r| normalized_key(&r.artist, &r.title)).collect(); deduplicate(items).into_iter().filter(|r| !known.contains(&normalized_key(&r.artist, &r.title))).collect() }

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    fn start_mock_server(responses: Vec<(String, String)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        let responses = Arc::new(Mutex::new(responses));
        thread::spawn(move || {
            for stream in listener.incoming().take(5) {
                let mut stream = stream.unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let mut response_body = r#"{"error":6,"message":"Track not found"}"#.to_string();
                {
                    let mut lock = responses.lock().unwrap();
                    if !lock.is_empty() {
                        let (needle, body) = lock.remove(0);
                        if request.contains(&needle) {
                            response_body = body;
                        } else {
                            // keep it for next request if not matched
                            lock.insert(0, (needle, body));
                        }
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{addr}/")
    }

    #[test]
    fn track_not_found_falls_back_to_artist_top_tracks() {
        // First call (track.getsimilar) -> Track not found, second call (artist.gettoptracks) -> one result
        let server = start_mock_server(vec![
            ("track.getsimilar".into(), r#"{"error":6,"message":"Track not found"}"#.into()),
            (
                "artist.gettoptracks".into(),
                r#"{"toptracks":{"track":[{"name":"Fallback Song","artist":{"name":"Fallback Artist"},"match":0.5,"image":[]}]}}"#.into(),
            ),
        ]);
        // small delay to ensure server is listening
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut service = DiscoveryService::with_base_url(Some("test_key".into()), server);
        let result = service.get_similar("Some Artist", "Missing Track", 5, &[]);
        assert!(matches!(result, DiscoveryResult::Success(items) if items.len() == 1 && items[0].title == "Fallback Song"));
    }

    #[test]
    fn track_not_found_with_empty_fallback_returns_empty_success() {
        let server = start_mock_server(vec![
            ("track.getsimilar".into(), r#"{"error":6,"message":"Track not found"}"#.into()),
            ("artist.gettoptracks".into(), r#"{"error":6,"message":"The artist you supplied could not be found"}"#.into()),
        ]);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut service = DiscoveryService::with_base_url(Some("test_key".into()), server);
        let result = service.get_similar("Unknown Artist", "Unknown Track", 5, &[]);
        assert!(matches!(result, DiscoveryResult::Success(items) if items.is_empty()));
    }
}
