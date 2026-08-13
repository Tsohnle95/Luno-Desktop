use luno_desktop_lib::migration::first_run::*;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn tempdir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("luno-first-run-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir(&path).unwrap();
    path
}

struct TestKeychain;
impl Keychain for TestKeychain { fn import_lastfm_key(&mut self, _: &str) -> Result<(), String> { Ok(()) } }

fn paths(root: &PathBuf) -> MigrationPaths {
    MigrationPaths { legacy_root: root.join("legacy"), target_root: root.join("target"), preview_dir: root.join("previews"), current_preview: Vec::new(), promoted_assets: Vec::new() }
}

#[test]
fn preflight_rejects_overlapping_data_roots() {
    let root = tempdir();
    let mut p = paths(&root);
    p.target_root = p.legacy_root.join("target");
    let result = migrate(&p, &MigrationOptions::default(), &mut TestKeychain);
    assert!(matches!(result, Err(MigrationError::UnsafePath(_))));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn migration_is_explicitly_macos_only() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    let result = migrate(&p, &MigrationOptions::default(), &mut TestKeychain);
    if cfg!(target_os = "macos") { assert!(result.is_ok()); } else { assert!(matches!(result, Err(MigrationError::UnsupportedPlatform))); }
    fs::remove_dir_all(root).unwrap();
}
