//! Legacy-compatible keys used for synchronization and duplicate detection.

/// Python's `split_track_name`: the first `" - "` is the separator and the
/// extension is removed before parsing.
pub fn split_track_name(path: &str) -> (String, String) {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    match stem.find(" - ") {
        Some(index) => (stem[index + 3..].trim().to_owned(), stem[..index].trim().to_owned()),
        None => (stem.to_owned(), String::new()),
    }
}

/// The relatively weak key used by playlist synchronization/import fallback.
pub fn sync_key(artist: &str, title: &str) -> String {
    artist.chars().chain(title.chars())
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn tag_word(value: &str) -> String {
    value.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// Stronger Python `_normalize_for_dupe` equivalent. Only ASCII letters and
/// digits survive the final pass, matching the reference regex.
pub fn duplicate_key(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '(' || c == '[' {
            let close = if c == '(' { ')' } else { ']' };
            let mut tag = String::new();
            let mut found = false;
            while let Some(next) = chars.next() {
                if next == close { found = true; break; }
                tag.push(next);
            }
            let normalized = tag_word(&tag).replace(' ', "");
            let removable = ["officialaudio", "officialvideo", "lyrics", "lyric", "audio", "hd", "hq", "4k", "remaster", "remastered", "clean", "explicit", "radioedit", "extendedmix", "extendedversion", "musicvideo", "visualizer", "bonustrack", "deluxe", "live", "acoustic", "remix"];
            if found && removable.contains(&normalized.as_str()) { continue; }
            output.push(c);
            output.push_str(&tag);
            if found { output.push(close); }
        } else { output.push(c); }
    }
    let lower = output.to_lowercase().replace("featuring", "feat").replace("feat.", "feat").replace("ft.", "feat");
    lower.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

pub fn duplicate_metadata_key(artist: &str, title: &str) -> String {
    duplicate_key(&format!("{artist}{title}"))
}
