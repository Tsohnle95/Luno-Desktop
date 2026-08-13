#[cfg(test)]
mod tests {
    use luno_desktop_lib::domain::{identity::*, normalization::*};

    #[test]
    fn first_separator_and_extension_match_python() {
        assert_eq!(split_track_name("/music/A - B - C.mp3"), ("B - C".into(), "A".into()));
        assert_eq!(split_track_name("untitled.flac"), ("untitled".into(), "".into()));
    }

    #[test]
    fn policies_are_distinct() {
        assert_eq!(sync_key("A.B", "Song!"), "absong");
        assert_eq!(duplicate_key("Artist - Song (Official Audio)"), "artistsong");
        assert_ne!(sync_key("A", "B"), duplicate_key("A (Official Audio)"));
    }

    #[test]
    fn provider_first_and_ambiguous_fallback() {
        let wanted = TrackMetadata { title: "Song".into(), artist: "Artist".into(), duration_ms: 1000, provider: None };
        let candidates = vec![("a".into(), TrackMetadata { duration_ms: 1000, ..wanted.clone() }), ("b".into(), TrackMetadata { duration_ms: 1001, ..wanted.clone() })];
        assert!(matches!(match_track(&wanted, &candidates), MatchResult::Ambiguous(_)));
        let provider = ProviderIdentity { provider: "youtube".into(), id: "x".into() };
        let requested = TrackMetadata { provider: Some(provider.clone()), ..wanted };
        let local = TrackMetadata { provider: Some(provider), duration_ms: 99999, ..requested.clone() };
        assert_eq!(match_track(&requested, &[("local".into(), local)]), MatchResult::Match("local".into()));
    }
}
