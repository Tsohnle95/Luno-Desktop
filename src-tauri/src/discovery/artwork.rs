use reqwest::blocking::Client;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use super::service::Recommendation;

pub const DESIGN_COUNT: u32 = 75;
pub const MAX_CACHE: usize = 128;

/// Bounded cover lookup used when Last.fm has no usable image.
/// Definitive misses are cached; transport/provider failures remain retryable.
pub struct ArtworkService {
    base_url: String,
    client: Client,
    cache: HashMap<String, Option<String>>,
    order: VecDeque<String>,
}

impl ArtworkService {
    pub fn new() -> Self {
        Self::with_base_url("https://api.deezer.com/search")
    }
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: Client::new(),
            cache: HashMap::new(),
            order: VecDeque::new(),
        }
    }
    pub fn find_artwork(&mut self, artist: &str, title: &str) -> Option<String> {
        let key = format!(
            "{}|{}",
            artist.trim().to_lowercase(),
            title.trim().to_lowercase()
        );
        if let Some(value) = self.cache.get(&key).cloned() {
            self.order.retain(|cached| cached != &key);
            self.order.push_back(key);
            return value;
        }
        let result = self.lookup(artist, title, Instant::now() + Duration::from_secs(4)).ok()?;
        self.store(key, result.clone());
        result
    }
    fn store(&mut self, key: String, result: Option<String>) {
        if self.cache.len() >= MAX_CACHE {
            if let Some(oldest) = self.order.pop_front() {
                self.cache.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.cache.insert(key, result.clone());
    }

    pub fn resolve_missing(&mut self, items: &mut [Recommendation]) {
        let mut pending = Vec::new();
        for (index, item) in items.iter_mut().enumerate() {
            if item.artwork_url.is_some() { continue; }
            let key = format!("{}|{}", item.artist.trim().to_lowercase(), item.title.trim().to_lowercase());
            if let Some(cached) = self.cache.get(&key) {
                item.artwork_url = cached.clone();
                self.order.retain(|entry| entry != &key);
                self.order.push_back(key);
            } else {
                pending.push((index, key, item.artist.clone(), item.title.clone()));
            }
        }
        let deadline = Instant::now() + Duration::from_secs(6);
        let next = AtomicUsize::new(0);
        let service = &*self;
        let results = std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for _ in 0..pending.len().min(4) {
                let pending = &pending;
                let next = &next;
                if let Ok(worker) = std::thread::Builder::new().name("recommendation-art".into()).spawn_scoped(scope, move || {
                    let mut results = Vec::new();
                    while Instant::now() < deadline {
                        let Some((index, key, artist, title)) = pending.get(next.fetch_add(1, Ordering::Relaxed)) else { break; };
                        if let Ok(cover) = service.lookup(artist, title, deadline) {
                            results.push((*index, key.clone(), cover));
                        }
                    }
                    results
                }) { workers.push(worker); }
            }
            workers.into_iter().flat_map(|worker| worker.join().unwrap_or_default()).collect::<Vec<_>>()
        });
        for (index, key, cover) in results {
            items[index].artwork_url = cover.clone();
            self.store(key, cover);
        }
    }

    fn lookup(&self, artist: &str, title: &str, deadline: Instant) -> Result<Option<String>, ()> {
        for query in [
            format!("\"{artist}\" \"{title}\""),
            format!("{artist} {title}"),
        ] {
            let remaining = deadline.checked_duration_since(Instant::now()).ok_or(())?;
            let response = match self
                .client
                .get(&self.base_url)
                .timeout(remaining.min(Duration::from_secs(2)))
                .query(&[("q", query), ("limit", "1".into())])
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.json::<Value>())
            {
                Ok(value) => value,
                Err(_) => return Err(()),
            };
            if response.get("error").is_some() || !response.get("data").is_some_and(Value::is_array) { return Err(()); }
            let Some(album) = response
                .get("data")
                .and_then(Value::as_array)
                .and_then(|v| v.first())
                .and_then(|v| v.get("album"))
            else {
                continue;
            };
            let raw = ["cover_xl", "cover_big", "cover_medium"]
                .iter()
                .find_map(|key| album.get(*key).and_then(Value::as_str))
                .unwrap_or("")
                .trim();
            if !raw.is_empty() {
                return Ok(Some(normalize_url(raw)));
            }
        }
        Ok(None)
    }
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.order.clear();
    }
}

fn normalize_url(url: &str) -> String {
    url.strip_prefix("http://")
        .map_or_else(|| url.to_owned(), |rest| format!("https://{rest}"))
}

/// Stable abstract-cover assignment. The returned index is suitable for a
/// renderer in any client and is invariant across processes.
pub fn design_index(artist: &str, title: &str) -> u32 {
    crc32fast::hash(&crate::discovery::service::normalized_key(artist, title).into_bytes())
        % DESIGN_COUNT
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Design {
    pub index: u32,
    pub size: u32,
    pub start: (u8, u8, u8),
    pub end: (u8, u8, u8),
}

/// Returns deterministic design metadata; the presentation layer draws the
/// gradient/geometric overlay without requiring a native image dependency.
pub fn render_design(index: u32, size: u32) -> Design {
    const PALETTES: [(u8, u8, u8, u8, u8, u8); 12] = [
        (18, 24, 22, 30, 215, 96),
        (26, 18, 24, 138, 92, 232),
        (18, 22, 30, 79, 168, 255),
        (28, 22, 16, 255, 159, 67),
        (22, 26, 30, 79, 216, 208),
        (30, 20, 20, 255, 107, 107),
        (20, 26, 20, 155, 232, 92),
        (22, 20, 28, 216, 143, 255),
        (20, 26, 28, 107, 208, 255),
        (28, 26, 20, 255, 208, 107),
        (24, 18, 24, 255, 119, 200),
        (18, 24, 26, 119, 255, 200),
    ];
    let p = PALETTES[(index as usize) % PALETTES.len()];
    Design {
        index: index % DESIGN_COUNT,
        size,
        start: (p.0, p.1, p.2),
        end: (p.3, p.4, p.5),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn provider_failure_is_retryable_and_success_is_cached() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for (status, body) in [
                (500, r#"{"error":"temporary"}"#),
                (200, r#"{"data":[{"album":{"cover_big":"https://img.test/real.jpg"}}]}"#),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                stream.read(&mut request).unwrap();
                write!(stream, "HTTP/1.1 {status} Status\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        let mut service = ArtworkService::with_base_url(format!("http://{address}"));
        let mut items = vec![Recommendation { artist: "Artist".into(), title: "Song".into(), match_score: 1.0, artwork_url: None }];
        service.resolve_missing(&mut items);
        assert!(items[0].artwork_url.is_none());
        assert!(service.cache.is_empty());
        service.resolve_missing(&mut items);
        assert_eq!(items[0].artwork_url.as_deref(), Some("https://img.test/real.jpg"));
        server.join().unwrap();
        items[0].artwork_url = None;
        service.resolve_missing(&mut items);
        assert_eq!(items[0].artwork_url.as_deref(), Some("https://img.test/real.jpg"));
    }

    #[test]
    fn expired_budget_does_not_start_a_request() {
        let service = ArtworkService::with_base_url("http://127.0.0.1:1");
        assert!(service.lookup("Artist", "Song", Instant::now() - Duration::from_secs(1)).is_err());
    }

    #[test]
    fn batch_resolves_all_covers_with_at_most_four_concurrent_requests() {
        use std::sync::Arc;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let current = active.clone();
        let peak = maximum.clone();
        let server = std::thread::spawn(move || {
            let mut handlers = Vec::new();
            for _ in 0..9 {
                let (mut stream, _) = listener.accept().unwrap();
                let current = current.clone();
                let peak = peak.clone();
                handlers.push(std::thread::spawn(move || {
                    let mut request = [0; 4096];
                    stream.read(&mut request).unwrap();
                    let count = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(count, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(30));
                    let body = r#"{"data":[{"album":{"cover_medium":"https://img.test/cover.jpg"}}]}"#;
                    current.fetch_sub(1, Ordering::SeqCst);
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }));
            }
            for handler in handlers { handler.join().unwrap(); }
        });
        let mut service = ArtworkService::with_base_url(format!("http://{address}"));
        let mut items: Vec<_> = (0..9).map(|i| Recommendation { artist: "Artist".into(), title: format!("Song {i}"), match_score: 1.0, artwork_url: None }).collect();
        service.resolve_missing(&mut items);
        assert!(items.iter().all(|item| item.artwork_url.as_deref() == Some("https://img.test/cover.jpg")));
        server.join().unwrap();
        assert!(maximum.load(Ordering::SeqCst) <= 4);
        assert!(maximum.load(Ordering::SeqCst) > 1);
    }

    #[test]
    fn definitive_miss_is_cached() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                stream.read(&mut request).unwrap();
                let body = r#"{"data":[]}"#;
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        let mut service = ArtworkService::with_base_url(format!("http://{address}"));
        assert!(service.find_artwork("Artist", "Missing").is_none());
        server.join().unwrap();
        assert_eq!(service.cache.get("artist|missing"), Some(&None));
        assert!(service.find_artwork("Artist", "Missing").is_none());
    }
}
