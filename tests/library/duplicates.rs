use luno_desktop_lib::library::duplicates::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[test]
fn normalizes_tags_and_groups_across_playlists() {
    let mut library = BTreeMap::new();
    library.insert("One".into(), vec![PathBuf::from("Artist - Song (Official Audio).mp3")]);
    library.insert("Two".into(), vec![PathBuf::from("Artist - Song [Remastered]!.flac")]);
    let groups = find_duplicates(&library, &BTreeSet::new());
    assert_eq!(groups.len(), 1);
    let mut allowed = BTreeSet::new(); allowed.insert(groups[0].key.clone());
    assert!(find_duplicates(&library, &allowed).is_empty());
}

#[test]
fn backups_are_strict_and_additive() {
    let mut keys = BTreeSet::from(["abc123".into()]);
    assert_eq!(import_backup_into(r#"["abc123","def"]"#, &mut keys).unwrap(), 1);
    assert!(import_backup(r#"["ok", 4]"#).is_err());
    assert!(import_backup(r#"{"keys":[]}"#).is_err());
    assert_eq!(import_backup(&export_backup(&keys).unwrap()).unwrap(), keys);
}

#[test]
fn decisions_validate_allow_and_delete() {
    assert!(matches!(allow_decision("abc"), Ok(DuplicateDecision::Allow { .. })));
    assert!(delete_decision(Vec::new()).is_err());
    assert!(delete_decision(vec![PathBuf::from("a"), PathBuf::from("a")]).is_err());
}
