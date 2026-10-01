//! User-triggered artwork lookup using MusicBrainz and Cover Art Archive.
//!
//! Candidate images stay in memory until explicitly applied. This keeps
//! provider lookup separate from the local missing-art scan and prevents any
//! remote match from changing a music file without a user decision.

use reqwest::blocking::{Client, Response};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const MAX_METADATA_BYTES: usize = 1_000_000;
const MAX_PROVIDER_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_RESULTS: usize = 8;
const MAX_RELEASE_GROUPS_TO_CHECK: usize = 8;
const MAX_REVIEW_CANDIDATES: usize = 5;
const MAX_PENDING_CANDIDATES: usize = 40;
const CANDIDATE_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Serialize)]
pub struct ArtworkCandidate {
    pub candidate_id: String,
    pub release: String,
    pub artist: String,
    pub year: Option<String>,
    pub score: u8,
    pub exact_match: bool,
    pub artwork_data_url: String,
}

#[derive(Clone)]
pub struct PendingArtwork {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

#[derive(Clone)]
struct PendingCandidate {
    track_id: String,
    created_at: Instant,
    artwork: PendingArtwork,
}

struct ReleaseGroupMatch {
    id: String,
    title: String,
    artist: String,
    year: Option<String>,
    score: u8,
    exact_recording: bool,
    album_match: bool,
}

pub struct ArtworkSearchService {
    client: Client,
    user_agent: String,
    request_start: Mutex<Option<Instant>>,
    next_candidate: AtomicU64,
    pending: Mutex<HashMap<String, PendingCandidate>>,
}

impl Default for ArtworkSearchService {
    fn default() -> Self {
        let agent = format!(
            "Luno Desktop/{} (https://github.com/Tsohnle95/Luno-Desktop)",
            env!("CARGO_PKG_VERSION")
        );
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            client,
            user_agent: agent,
            request_start: Mutex::new(None),
            next_candidate: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
        }
    }
}

impl ArtworkSearchService {
    pub fn search(
        &self,
        track_id: &str,
        title: &str,
        artist: &str,
        album: Option<&str>,
    ) -> Result<Vec<ArtworkCandidate>, String> {
        let title = title.trim();
        let artist = artist.trim();
        let album = album.map(str::trim).filter(|album| !album.is_empty());
        if title.is_empty() || artist.is_empty() {
            return Err("A song title and artist are required to search for artwork.".into());
        }
        if title.chars().count() > 512
            || artist.chars().count() > 256
            || album.is_some_and(|album| album.chars().count() > 512)
        {
            return Err("The song title or artist name is too long to search.".into());
        }

        {
            let now = Instant::now();
            let mut pending = self.pending.lock().unwrap_or_else(|error| error.into_inner());
            pending.retain(|_, candidate| {
                now.saturating_duration_since(candidate.created_at) < CANDIDATE_TTL
                    && candidate.track_id != track_id
            });
        }

        let query = format!(
            "recording:\"{}\" AND artist:\"{}\"",
            escape_query(title),
            escape_query(artist)
        );
        self.wait_for_provider_slot();
        let response = self
            .client
            .get("https://musicbrainz.org/ws/2/recording/")
            .header(reqwest::header::USER_AGENT, &self.user_agent)
            .query(&[("query", query.as_str()), ("fmt", "json"), ("limit", "8")])
            .send()
            .map_err(|error| format!("MusicBrainz lookup failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("MusicBrainz returned HTTP {}.", response.status()));
        }
        let body = read_limited(response, MAX_METADATA_BYTES)?;
        let document: Value = serde_json::from_slice(&body)
            .map_err(|error| format!("MusicBrainz returned invalid data: {error}"))?;
        let recordings = document
            .get("recordings")
            .and_then(Value::as_array)
            .ok_or_else(|| "MusicBrainz returned no recording matches.".to_string())?;

        let mut release_groups = Vec::new();
        let mut seen_groups = std::collections::HashSet::new();
        for recording in recordings.iter().take(MAX_RESULTS) {
            let recording_title = recording.get("title").and_then(Value::as_str).unwrap_or("");
            let recording_artist = artist_credit(recording.get("artist-credit"));
            let exact_match = normalize(title) == normalize(recording_title)
                && normalize(artist) == normalize(&recording_artist);
            let score = parse_score(recording.get("score"));
            let Some(releases) = recording.get("releases").and_then(Value::as_array) else {
                continue;
            };
            for release in releases {
                let Some(group_id) = release
                    .get("release-group")
                    .and_then(|group| group.get("id"))
                    .and_then(Value::as_str)
                else {
                    continue;
                };
                if !valid_musicbrainz_id(group_id) || !seen_groups.insert(group_id.to_owned()) {
                    continue;
                }
                let Some(release_title) = release.get("title").and_then(Value::as_str) else {
                    continue;
                };
                let release_artist = artist_credit(release.get("artist-credit"));
                let release_artist = if release_artist.is_empty() {
                    recording_artist.clone()
                } else {
                    release_artist
                };
                let year = release
                    .get("date")
                    .and_then(Value::as_str)
                    .or_else(|| {
                        release
                            .get("release-group")
                            .and_then(|group| group.get("first-release-date"))
                            .and_then(Value::as_str)
                    })
                    .and_then(|date| date.get(..4))
                    .map(str::to_owned);
                release_groups.push(ReleaseGroupMatch {
                    id: group_id.to_owned(),
                    title: release_title.to_owned(),
                    artist: release_artist,
                    year,
                    score,
                    exact_recording: exact_match,
                    album_match: album.is_some_and(|album| normalize(album) == normalize(release_title)),
                });
            }
        }
        release_groups.sort_by(|left, right| {
            right
                .album_match
                .cmp(&left.album_match)
                .then_with(|| right.score.cmp(&left.score))
        });
        release_groups.truncate(MAX_RELEASE_GROUPS_TO_CHECK);

        let mut candidates = Vec::new();
        for matched in release_groups {
            self.wait_for_provider_slot();
            let image_response = match self
                .client
                .get(format!(
                    "https://coverartarchive.org/release-group/{}/front-500",
                    matched.id
                ))
                .header(reqwest::header::USER_AGENT, &self.user_agent)
                .send()
            {
                Ok(response) if response.status().is_success() => response,
                Ok(_) => continue,
                Err(_) => continue,
            };
            let bytes = match read_limited(image_response, MAX_PROVIDER_IMAGE_BYTES) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            let (bytes, mime, artwork_data_url) =
                match super::artwork::normalize_provider_image(&bytes) {
                    Ok(image) => image,
                    Err(_) => continue,
                };

            let candidate_id = format!(
                "artwork-{}",
                self.next_candidate.fetch_add(1, Ordering::Relaxed)
            );
            self.pending
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(
                    candidate_id.clone(),
                    PendingCandidate {
                        track_id: track_id.to_owned(),
                        created_at: Instant::now(),
                        artwork: PendingArtwork { bytes, mime },
                    },
                );
            candidates.push(ArtworkCandidate {
                candidate_id,
                release: matched.title,
                artist: matched.artist,
                year: matched.year,
                score: matched.score,
                exact_match: matched.exact_recording,
                artwork_data_url,
            });
            if candidates.len() >= MAX_REVIEW_CANDIDATES {
                break;
            }
        }

        let mut pending = self.pending.lock().unwrap_or_else(|error| error.into_inner());
        while pending.len() > MAX_PENDING_CANDIDATES {
            let oldest = pending
                .iter()
                .min_by_key(|(_, candidate)| candidate.created_at)
                .map(|(candidate_id, _)| candidate_id.clone());
            let Some(oldest) = oldest else { break };
            pending.remove(&oldest);
        }
        Ok(candidates)
    }

    pub fn candidate(&self, track_id: &str, candidate_id: &str) -> Result<PendingArtwork, String> {
        let pending = self.pending.lock().unwrap_or_else(|error| error.into_inner());
        let candidate = pending
            .get(candidate_id)
            .filter(|candidate| candidate.track_id == track_id)
            .ok_or_else(|| "That artwork match expired. Search for it again.".to_string())?;
        if Instant::now().saturating_duration_since(candidate.created_at) >= CANDIDATE_TTL {
            return Err("That artwork match expired. Search for it again.".into());
        }
        Ok(candidate.artwork.clone())
    }

    pub fn forget_candidate(&self, candidate_id: &str) {
        self.pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(candidate_id);
    }

    fn wait_for_provider_slot(&self) {
        let mut previous = self
            .request_start
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(started) = *previous {
            let elapsed = started.elapsed();
            if elapsed < Duration::from_secs(1) {
                thread::sleep(Duration::from_secs(1) - elapsed);
            }
        }
        *previous = Some(Instant::now());
    }
}

fn read_limited(mut response: Response, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(limit.min(128 * 1024));
    response
        .by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read provider response: {error}"))?;
    if bytes.len() > limit {
        return Err("The artwork provider returned an oversized response.".into());
    }
    Ok(bytes)
}

fn escape_query(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '+' | '-' | '&' | '|' | '!' | '(' | ')' | '{' | '}' | '[' | ']' | '^' | '"' | '~' | '*' | '?' | ':' | '\\' | '/') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn parse_score(value: Option<&Value>) -> u8 {
    value
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|score| score.parse().ok()))
        })
        .unwrap_or(0)
        .min(100) as u8
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn valid_musicbrainz_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn artist_credit(credit: Option<&Value>) -> String {
    credit
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry
                .get("artist")
                .and_then(|artist| artist.get("name"))
                .or_else(|| entry.get("name"))
                .and_then(Value::as_str)
        })
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(" & ")
}
