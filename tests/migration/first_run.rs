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

struct FailingKeychain;
impl Keychain for FailingKeychain { fn import_lastfm_key(&mut self, _: &str) -> Result<(), String> { Err("access denied".into()) } }

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

#[cfg(target_os = "macos")]
#[test]
fn keychain_failure_writes_no_target_state_or_version() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    fs::write(p.legacy_root.join(".luno_config.json"), r#"{"lastfm_key":"secret"}"#).unwrap();

    let result = migrate(&p, &MigrationOptions::default(), &mut FailingKeychain);
    assert!(matches!(result, Err(MigrationError::Keychain(_))));
    assert!(!p.state_file().exists());
    assert!(!p.version_record().exists());
    assert!(fs::read_dir(&p.target_root).unwrap().next().is_none());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn orphaned_state_without_completed_record_is_recovered() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    fs::create_dir_all(&p.target_root).unwrap();
    fs::write(p.state_file(), "orphaned").unwrap();

    let result = migrate(&p, &MigrationOptions::default(), &mut TestKeychain).unwrap();
    assert_eq!(result.state, p.state_file());
    assert!(p.state_file().is_file());
    assert!(p.version_record().is_file());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn snapshot_redacts_key_and_invalid_record_is_recoverable() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    fs::write(p.legacy_root.join(".luno_config.json"), r#"{"lastfm_key":"secret","volume":0.5}"#).unwrap();
    fs::create_dir_all(&p.target_root).unwrap();
    fs::write(p.version_record(), r#"{"migration":"desktop-first-run","version":1,"completed":false}"#).unwrap();

    let result = migrate(&p, &MigrationOptions::default(), &mut TestKeychain).unwrap();
    let snapshot = fs::read_to_string(result.snapshot).unwrap();
    assert!(snapshot.contains("[REDACTED]"));
    assert!(!snapshot.contains("secret"));
    assert_eq!(fs::read_to_string(p.legacy_root.join(".luno_config.json")).unwrap(), r#"{"lastfm_key":"secret","volume":0.5}"#);
    fs::remove_dir_all(root).unwrap();
}
