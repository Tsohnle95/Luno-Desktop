//! Provider-neutral video source lookup.  The process/yt-dlp adapter supplies
//! candidates; this module owns query fallback, relevance, and identity rules.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoCandidate {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoSource {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub url: String,
}

pub const MAX_RESULTS: usize = 10;

pub fn query_forms(artist: Option<&str>, title: &str) -> Vec<String> {
    let title = title.trim();
    let mut forms = Vec::new();
    if let Some(artist) = artist.map(str::trim).filter(|s| !s.is_empty()) {
        forms.push(format!("{artist} - {title}"));
        forms.push(format!("{title} {artist}"));
    }
    forms.push(title.to_owned());
    forms
}

/// Search each fallback form until ten relevant, ID-unique results exist.
/// The callback is deliberately provider-neutral and may perform blocking work.
pub fn lookup<F>(artist: Option<&str>, title: &str, mut search: F) -> Vec<VideoSource>
where F: FnMut(&str) -> Vec<VideoCandidate> {
    let wanted_title = words(title);
    let wanted_artist = artist.map(words).unwrap_or_default();
    let mut seen = HashSet::new();
    let mut results = Vec::new();
    for query in query_forms(artist, title) {
        for candidate in search(&query) {
            let id = canonical_id(&candidate.id, &candidate.url);
            if id.is_empty() || !relevant(&candidate, &wanted_title, &wanted_artist) || !seen.insert(id.clone()) { continue; }
            results.push(VideoSource { id: id.clone(), title: candidate.title, artist: candidate.artist, url: canonical_url(&id) });
            if results.len() == MAX_RESULTS { return results; }
        }
    }
    results
}

pub fn canonical_url(id: &str) -> String { format!("https://www.youtube.com/watch?v={id}") }

fn canonical_id(id: &str, url: &str) -> String {
    let id = id.trim();
    if !id.is_empty() { return id.to_owned(); }
    url.split("v=").nth(1).and_then(|v| v.split('&').next()).or_else(|| url.split("youtu.be/").nth(1).map(|v| v.split('?').next().unwrap_or(v))).unwrap_or("").to_owned()
}

fn relevant(candidate: &VideoCandidate, title: &[String], artist: &[String]) -> bool {
    let candidate_title = words(&candidate.title);
    if title.iter().any(|word| !candidate_title.contains(word)) { return false; }
    if artist.is_empty() { return true; }
    let evidence = candidate.artist.as_deref().unwrap_or(&candidate.title);
    let evidence = words(evidence);
    artist.iter().any(|word| evidence.contains(word)) || artist.iter().all(|word| candidate_title.contains(word))
}

fn words(value: &str) -> Vec<String> {
    value.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty()).map(|word| word.to_lowercase()).collect()
}
