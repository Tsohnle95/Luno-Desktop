//! Provider-neutral video source lookup.  The process/yt-dlp adapter supplies
//! candidates; this module owns query fallback, relevance, and identity rules.

use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

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
/// Staging tries at most three candidates, so three strict matches are enough
/// to stop launching heavyweight fallback searches.
pub const READY_RESULTS: usize = 3;

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

/// Search fallback forms until three strict, ID-unique results exist. If a
/// form cannot supply them, retain relevant relaxed candidates and continue.
/// The callback is deliberately provider-neutral and may perform blocking work.
///
/// Admission is two-tier so one refused upload cannot dead-end staging
/// (YouTube refuses SOME licensed videos to every downloader client):
/// strict matches — every title word plus artist evidence, the historical
/// rule — come first in query-form order, then relaxed matches anchored on
/// artist evidence with at least half (minimum 2) of the title words,
/// best-overlap first. The relaxed floor deliberately rejects word-soup
/// results (cartoons, game walkthroughs) that merely share common words.
pub fn lookup<F>(artist: Option<&str>, title: &str, mut search: F) -> Vec<VideoSource>
where
    F: FnMut(&str) -> Vec<VideoCandidate>,
{
    let wanted_title = words(title);
    let wanted_artist = artist.map(words).unwrap_or_default();
    let floor = fallback_word_floor(wanted_title.len());
    let mut seen = HashSet::new();
    let mut strict: Vec<VideoSource> = Vec::new();
    let mut relaxed: Vec<(usize, VideoSource)> = Vec::new();
    for query in query_forms(artist, title) {
        for candidate in search(&query) {
            let id = canonical_id(&candidate.id, &candidate.url);
            if id.is_empty() { continue; }
            let overlap = title_overlap(&candidate.title, &wanted_title);
            // Relevance precedes dedupe: rejected candidates never reserve ids.
            if overlap < floor || !has_artist_evidence(&candidate, &wanted_artist) || !seen.insert(id.clone()) { continue; }
            let source = VideoSource {
                id: id.clone(),
                title: candidate.title,
                artist: candidate.artist,
                url: canonical_url(&id),
            };
            if overlap == wanted_title.len() {
                strict.push(source);
                if strict.len() == READY_RESULTS { return strict; }
            } else {
                relaxed.push((overlap, source));
            }
        }
    }
    // Stable sort: equal overlaps stay in query-form encounter order.
    relaxed.sort_by(|a, b| b.0.cmp(&a.0));
    strict.extend(relaxed.into_iter().map(|(_, source)| source));
    strict.truncate(MAX_RESULTS);
    strict
}

/// Minimum title-word overlap for admission. Full overlap always qualifies;
/// partial matches need at least half the words AND at least 2 so one-word
/// titles keep working while generic single-word bait does not.
fn fallback_word_floor(title_len: usize) -> usize {
    std::cmp::min(title_len, std::cmp::max(2, title_len.div_ceil(2)))
}

fn title_overlap(candidate_title: &str, wanted_title: &[String]) -> usize {
    let candidate_words = words(candidate_title);
    wanted_title.iter().filter(|word| candidate_words.contains(word)).count()
}

/// The artist must surface somewhere: in the uploader name or, when the
/// result carries no uploader, in the title itself (all artist words).
fn has_artist_evidence(candidate: &VideoCandidate, wanted_artist: &[String]) -> bool {
    if wanted_artist.is_empty() { return true; }
    let evidence = words(candidate.artist.as_deref().unwrap_or(&candidate.title));
    let candidate_title = words(&candidate.title);
    wanted_artist.iter().any(|word| evidence.contains(word))
        || wanted_artist.iter().all(|word| candidate_title.contains(word))
}

pub fn canonical_url(id: &str) -> String { format!("https://www.youtube.com/watch?v={id}") }

fn canonical_id(id: &str, url: &str) -> String {
    let id = id.trim();
    if !id.is_empty() { return id.to_owned(); }
    url.split("v=").nth(1).and_then(|v| v.split('&').next()).or_else(|| url.split("youtu.be/").nth(1).map(|v| v.split('?').next().unwrap_or(v))).unwrap_or("").to_owned()
}

fn words(value: &str) -> Vec<String> {
    // macOS filenames commonly decompose accents. Normalize before splitting:
    // a combining mark would otherwise split an artist into different words
    // than the canonically equivalent title returned by the video provider.
    let normalized: String = value.nfkc().collect();
    normalized.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty()).map(|word| word.to_lowercase()).collect()
}
