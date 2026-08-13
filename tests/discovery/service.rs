use luno_desktop_lib::discovery::service::*;
use luno_desktop_lib::discovery::artwork::*;

#[test]
fn missing_key_and_empty_seed_are_honest() {
    let mut service = DiscoveryService::with_base_url(None, "http://unused");
    assert!(matches!(service.get_similar("A", "B", 20, &[]), DiscoveryResult::Error(DiscoveryError::MissingApiKey)));
    service.set_api_key(Some("key".into()));
    assert_eq!(service.get_similar(" ", "B", 20, &[]), DiscoveryResult::Success(vec![]));
}

#[test]
fn filtering_deduplicates_library_and_result_variants() {
    let items = vec![
        Recommendation { artist: "Beyoncé".into(), title: "Halo Live".into(), match_score: .9, artwork_url: None },
        Recommendation { artist: "Beyonce".into(), title: "Halo Live".into(), match_score: .8, artwork_url: None },
        Recommendation { artist: "Other".into(), title: "Song".into(), match_score: .7, artwork_url: None },
    ];
    let library = vec![Recommendation { artist: "BEYONCE".into(), title: "Halo Live".into(), match_score: 0.0, artwork_url: None }];
    assert_eq!(filter_recommendations(items, &library).len(), 1);
}

#[test]
fn generation_only_latest_request_is_current() {
    let service = DiscoveryService::new(None);
    let first = service.begin_request();
    let second = service.begin_request();
    assert!(!service.is_current(first));
    assert!(service.is_current(second));
}

#[test]
fn artwork_design_is_stable_and_bounded() {
    assert_eq!(design_index("Artist", "Song"), design_index("artist", "song"));
    assert!(design_index("Artist", "Song") < DESIGN_COUNT);
    assert_eq!(render_design(76, 48).index, 1);
}
