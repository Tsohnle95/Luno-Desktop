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
fn keychain_failure_restores_preexisting_target_records() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    fs::create_dir_all(&p.target_root).unwrap();
    fs::write(p.state_file(), "old-state").unwrap();
    fs::write(p.version_record(), r#"{"migration":"old","version":9,"completed":false}"#).unwrap();
    fs::write(p.legacy_root.join(".luno_config.json"), r#"{"lastfm_key":"secret"}"#).unwrap();
    assert!(matches!(migrate(&p, &MigrationOptions::default(), &mut FailingKeychain), Err(MigrationError::Keychain(_))));
    assert_eq!(fs::read_to_string(p.state_file()).unwrap(), "old-state");
    assert!(fs::read_to_string(p.version_record()).unwrap().contains("\"version\":9"));
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

#[cfg(target_os = "macos")]
#[test]
fn malformed_or_non_object_config_is_not_snapshotted() {
    for contents in ["not-json", "[\"secret\"]"] {
        let root = tempdir();
        let p = paths(&root);
        fs::create_dir_all(&p.legacy_root).unwrap();
        fs::write(p.legacy_root.join(".luno_config.json"), contents).unwrap();
        let result = migrate(&p, &MigrationOptions::default(), &mut TestKeychain).unwrap();
        let backup = fs::read_to_string(result.snapshot.join("config-snapshot-omitted.txt")).unwrap();
        assert!(backup.contains("snapshot omitted"));
        assert!(!fs::read_to_string(result.snapshot.join(".luno_config.json")).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(target_os = "macos")]
#[test]
fn version_record_crash_rolls_back_state_and_version() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    let result = migrate(&p, &MigrationOptions { crash_at: Some(CrashStage::VersionRecorded), ..Default::default() }, &mut TestKeychain);
    assert!(matches!(result, Err(MigrationError::InjectedCrash(CrashStage::VersionRecorded))));
    assert!(!p.state_file().exists());
    assert!(!p.version_record().exists());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn every_precommit_crash_restores_quarantined_records() {
    for stage in [CrashStage::Preflight, CrashStage::Snapshotted, CrashStage::Converted, CrashStage::VersionRecorded, CrashStage::PreviewCleaned] {
        let root = tempdir();
        let p = paths(&root);
        fs::create_dir_all(&p.legacy_root).unwrap();
        fs::create_dir_all(&p.target_root).unwrap();
        fs::write(p.state_file(), "old-state").unwrap();
        fs::write(p.version_record(), "old-version").unwrap();
        let result = migrate(&p, &MigrationOptions { crash_at: Some(stage), ..Default::default() }, &mut TestKeychain);
        if stage == CrashStage::Preflight {
            assert!(matches!(result, Err(MigrationError::InjectedCrash(CrashStage::Preflight))));
        } else {
            assert!(matches!(result, Err(MigrationError::InjectedCrash(_))));
            assert_eq!(fs::read_to_string(p.state_file()).unwrap(), "old-state");
            assert_eq!(fs::read_to_string(p.version_record()).unwrap(), "old-version");
        }
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(target_os = "macos")]
#[test]
fn absent_preview_directory_is_a_noop() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    let receipt = migrate(&p, &MigrationOptions::default(), &mut TestKeychain).unwrap();
    assert!(receipt.removed_previews.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn post_commit_quarantine_cleanup_is_only_a_warning() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    fs::create_dir_all(&p.target_root).unwrap();
    fs::write(p.state_file(), "old-state").unwrap();
    let receipt = migrate(&p, &MigrationOptions { fail_discard_quarantine: true, ..Default::default() }, &mut TestKeychain).unwrap();
    assert!(receipt.warning_messages.iter().any(|message| message.contains("committed")));
    assert!(p.state_file().is_file());
    assert!(p.version_record().is_file());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(all(target_os = "macos", unix))]
#[test]
fn dangling_preview_symlink_is_rejected() {
    use std::os::unix::fs::symlink;
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    symlink(root.join("missing-preview"), &p.preview_dir).unwrap();
    assert!(matches!(migrate(&p, &MigrationOptions::default(), &mut TestKeychain), Err(MigrationError::UnsafePath(_))));
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn preview_cleanup_failure_rolls_back_every_migration_artifact() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    fs::create_dir_all(p.preview_dir.join("unexpected-directory")).unwrap();

    let result = migrate(&p, &MigrationOptions::default(), &mut TestKeychain);
    let error = result.unwrap_err();
    assert!(matches!(error, MigrationError::Cleanup(message) if message.contains("preview cleanup failed")));
    assert!(!p.state_file().exists());
    assert!(!p.version_record().exists());
    assert!(fs::read_dir(&p.target_root).unwrap().next().is_none());
    fs::remove_dir_all(root).unwrap();
}
