use luno_desktop_lib::transfer::codec::{decode, encode};

#[test]
fn frozen_valid_fixture_round_trips_without_reordering() {
    let input = include_str!("../../contracts/library-export/v1/valid.json");
    let manifest = decode(input).expect("valid fixture");
    let output = encode(&manifest).expect("encode");
    let reparsed = decode(&output).expect("round trip");
    assert_eq!(manifest, reparsed);
    assert_eq!(manifest.tracks[0].r#ref, "src:one");
    assert_eq!(manifest.playlists[0].track_refs, vec!["src:one".to_string()]);
}

#[test]
fn unknown_fields_are_rejected() {
    let input = include_str!("../../contracts/library-export/v1/invalid-unknown-field.json");
    assert!(decode(input).is_err());
}

#[test]
fn source_less_track_requires_ambiguity_confirmation() {
    let input = r#"{"format":"luno.library.export","manifest_version":1,"scope":"library","created_at":"2026-08-10T00:00:00Z","tracks":[{"ref":"x","title":"T","artist":"","album":"","duration_ms":0,"source":null,"ambiguity_confirmation_required":false}],"playlists":[],"unassigned_track_refs":["x"]}"#;
    assert!(decode(input).is_err());
}
