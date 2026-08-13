use luno_desktop_lib::download::csv::*;
use luno_desktop_lib::download::workflows::*;
use std::sync::atomic::AtomicBool;

#[test] fn validates_only_http_and_https_with_authority() {
    assert!(validate_http_url("https://example.test/a").is_ok());
    assert!(validate_http_url("ftp://example.test/a").is_err());
    assert!(validate_http_url("https:///a").is_err());
}

#[test] fn parses_bom_quotes_commas_and_multiline_fields() {
    let input = "\u{feff}Track Name,Artist Name(s),Album Name\n\"A, B\",Artist,\"Record\nTwo\"\n";
    let tracks = parse(input.as_bytes()).unwrap();
    assert_eq!(tracks[0].title, "A, B"); assert_eq!(tracks[0].album.as_deref(), Some("Record\nTwo"));
}

#[test] fn required_csv_fields_are_enforced() {
    assert!(matches!(parse(b"Track Name\nSong\n"), Err(CsvError::MissingColumn("Artist Name(s)"))));
    assert!(matches!(parse(b"Track Name,Artist Name(s)\nSong,\n"), Err(CsvError::MissingField { .. })));
}

#[test] fn url_queue_continues_after_failure_and_reports_truthful_terminal_state() {
    let urls = vec!["https://one.test".into(), "https://two.test".into(), "bad".into()];
    let events = run_url_queue(&urls, &AtomicBool::new(false), |url, _| if url.contains("two") { Err("network".into()) } else { Ok(()) });
    assert!(events.iter().any(|e| matches!(e, QueueEvent::Failed { index: 1, .. })));
    assert!(matches!(events.last(), Some(QueueEvent::Complete { succeeded: 1, failed: 2 })));
}

#[test] fn csv_soundcloud_falls_back_to_youtube() {
    let track = CsvTrack { title: "Song".into(), artist: "Artist".into(), album: None, url: None };
    assert_eq!(source_for_track(&track, true, None).unwrap().0, SearchProvider::YouTube);
}

#[test] fn search_orders_soundcloud_before_youtube_when_enabled() {
    let sources = search_sources("Artist Song", true).unwrap();
    assert!(matches!(sources[0], (SearchProvider::SoundCloud, _)));
    assert!(matches!(sources[1], (SearchProvider::YouTube, _)));
}
