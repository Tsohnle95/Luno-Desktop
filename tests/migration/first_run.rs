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
impl Keychain for TestKeychain {
    fn import_lastfm_key(&mut self, _: &str) -> Result<KeychainImport, String> { Ok(KeychainImport::Existing) }
    fn rollback_lastfm_key(&mut self, _: KeychainImport) -> Result<(), String> { Ok(()) }
}

struct FailingKeychain;
impl Keychain for FailingKeychain {
    fn import_lastfm_key(&mut self, _: &str) -> Result<KeychainImport, String> { Err("access denied".into()) }
    fn rollback_lastfm_key(&mut self, _: KeychainImport) -> Result<(), String> { Ok(()) }
}

struct TrackingKeychain { rollback_count: usize }
impl Keychain for TrackingKeychain {
    fn import_lastfm_key(&mut self, _: &str) -> Result<KeychainImport, String> { Ok(KeychainImport::Created) }
    fn rollback_lastfm_key(&mut self, _: KeychainImport) -> Result<(), String> { self.rollback_count += 1; Ok(()) }
}

struct ConfigurableKeychain { import: KeychainImport, rollback_error: bool, rollback_count: usize }
impl Keychain for ConfigurableKeychain {
    fn import_lastfm_key(&mut self, _: &str) -> Result<KeychainImport, String> { Ok(self.import) }
    fn rollback_lastfm_key(&mut self, _: KeychainImport) -> Result<(), String> {
        self.rollback_count += 1;
        if self.rollback_error { Err("rollback unavailable".into()) } else { Ok(()) }
    }
}

fn paths(root: &PathBuf) -> MigrationPaths {
    MigrationPaths { legacy_root: root.join("legacy"), legacy_data_root: Some(root.join("legacy")), target_root: root.join("target"), preview_dir: root.join("legacy").join(".luno_previews"), current_preview: Vec::new(), promoted_assets: Vec::new() }
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

#[cfg(target_os = "macos")]
#[test]
fn home_discovery_root_allows_home_app_data_target() {
    let root = tempdir();
    let legacy = root.join(".legacy-data");
    let preview = root.join(".luno_previews");
    let target = root.join("Library/Application Support/com.ty.luno");
    fs::create_dir_all(&legacy).unwrap();
    fs::create_dir_all(&preview).unwrap();
    fs::write(legacy.join(".luno_config.json"), r#"{}"#).unwrap();
    let p = MigrationPaths { legacy_root: root.clone(), legacy_data_root: Some(legacy), target_root: target, preview_dir: preview, current_preview: Vec::new(), promoted_assets: Vec::new() };
    assert!(migrate(&p, &MigrationOptions::default(), &mut TestKeychain).is_ok());
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
    let snapshot = fs::read_to_string(result.snapshot.unwrap()).unwrap();
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
        let snapshot = result.snapshot.as_ref().unwrap();
        let backup = fs::read_to_string(snapshot.join("config-snapshot-omitted.txt")).unwrap();
        assert!(backup.contains("snapshot omitted"));
        assert!(!fs::read_to_string(snapshot.join(".luno_config.json")).is_ok());
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

#[cfg(target_os = "macos")]
#[test]
fn completed_retry_does_not_delete_unowned_recovery_quarantine() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.target_root).unwrap();
    fs::write(p.version_record(), r#"{"migration":"desktop-first-run","version":1,"completed":true}"#).unwrap();
    let leftover = p.target_root.join(".migration-recovery-leftover");
    fs::write(&leftover, "old").unwrap();
    let result = migrate(&p, &MigrationOptions::default(), &mut TestKeychain);
    let receipt = result.unwrap();
    assert!(receipt.snapshot.is_none());
    assert!(receipt.warning_messages.is_empty());
    assert!(leftover.exists());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn completed_migration_is_idempotent_and_preserves_target_state() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    let first = migrate(&p, &MigrationOptions::default(), &mut TestKeychain).unwrap();
    fs::write(p.state_file(), "user-owned target").unwrap();
    fs::create_dir_all(p.legacy_root.join("unknown-subdirectory")).unwrap();
    fs::write(p.legacy_root.join("unknown-subdirectory/new-legacy-input"), "preserved").unwrap();
    let second = migrate(&p, &MigrationOptions::default(), &mut TestKeychain).unwrap();
    assert_eq!(fs::read_to_string(p.state_file()).unwrap(), "user-owned target");
    assert_eq!(first.version, second.version);
    assert!(second.warning_messages.iter().any(|warning| warning.contains("already complete") && warning.contains("unknown-subdirectory/new-legacy-input")));
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn completed_retry_rejects_oversized_preview_inventory() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.target_root).unwrap();
    fs::create_dir_all(&p.preview_dir).unwrap();
    fs::write(p.version_record(), r#"{"migration":"desktop-first-run","version":1,"completed":true}"#).unwrap();
    for index in 0..=luno_desktop_lib::migration::legacy_readers::MAX_RETRY_ENTRIES {
        fs::write(p.preview_dir.join(format!("preview-{index}")), b"x").unwrap();
    }
    let error = migrate(&p, &MigrationOptions::default(), &mut TestKeychain).unwrap_err();
    assert!(matches!(error, MigrationError::Io(message) if message.contains("legacy input inventory failed") && message.contains("retry inspection exceeded")));
    assert!(p.version_record().is_file());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn migration_warning_paths_are_relative_to_their_explicit_root() {
    let root = tempdir();
    let music = root.join("music");
    let p = MigrationPaths { legacy_root: root.clone(), legacy_data_root: Some(music.clone()), target_root: root.join("target"), preview_dir: root.join(".luno_previews"), current_preview: Vec::new(), promoted_assets: Vec::new() };
    fs::create_dir_all(&music).unwrap();
    fs::write(root.join(".luno_config.json"), format!(r#"{{"music_root":"{}"}}"#, music.display())).unwrap();
    fs::write(music.join(".luno_playlist_urls.json"), "not-json").unwrap();
    let receipt = migrate(&p, &MigrationOptions::default(), &mut TestKeychain).unwrap();
    assert!(receipt.warning_messages.iter().any(|warning| warning.starts_with(".luno_playlist_urls.json: ")));
    assert!(receipt.warning_messages.iter().all(|warning| !warning.contains(root.to_str().unwrap())));
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

#[cfg(all(target_os = "macos", unix))]
#[test]
fn snapshot_excludes_target_descendants_when_legacy_root_is_home() {
    let root = tempdir();
    let legacy = root.join(".legacy-data");
    let target = root.join("Library/Application Support/com.ty.luno");
    let preview = root.join(".luno_previews");
    fs::create_dir_all(&legacy).unwrap();
    fs::create_dir_all(&target).unwrap();
    fs::create_dir_all(&preview).unwrap();
    fs::write(legacy.join(".luno_config.json"), format!(r#"{{"music_root":"{}"}}"#, legacy.display())).unwrap();
    fs::create_dir_all(target.join("nested-marker")).unwrap();
    fs::write(target.join("nested-marker/.downloaded_vids"), "TARGET\n").unwrap();
    let p = MigrationPaths { legacy_root: root.clone(), legacy_data_root: Some(legacy), target_root: target.clone(), preview_dir: preview, current_preview: Vec::new(), promoted_assets: Vec::new() };

    let receipt = migrate(&p, &MigrationOptions::default(), &mut TestKeychain).unwrap();
    let snapshot = receipt.snapshot.unwrap();
    assert!(!snapshot.join("Library/Application Support/com.ty.luno/nested-marker/.downloaded_vids").exists());
    assert!(!target.join("nested-marker/.downloaded_vids").exists());
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

#[cfg(target_os = "macos")]
#[test]
fn post_import_failure_rolls_back_new_keychain_item() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    fs::write(p.legacy_root.join(".luno_config.json"), r#"{"lastfm_key":"secret"}"#).unwrap();
    let mut keychain = TrackingKeychain { rollback_count: 0 };
    let result = migrate(&p, &MigrationOptions { crash_at: Some(CrashStage::Converted), ..Default::default() }, &mut keychain);
    assert!(matches!(result, Err(MigrationError::InjectedCrash(CrashStage::Converted))));
    assert_eq!(keychain.rollback_count, 1);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn snapshot_failure_restores_quarantined_records() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    fs::create_dir_all(&p.target_root).unwrap();
    fs::write(p.state_file(), "old-state").unwrap();
    fs::write(p.version_record(), "old-version").unwrap();
    let result = migrate(&p, &MigrationOptions { fail_snapshot: true, ..Default::default() }, &mut TestKeychain);
    assert!(matches!(result, Err(MigrationError::Io(_))));
    assert_eq!(fs::read_to_string(p.state_file()).unwrap(), "old-state");
    assert_eq!(fs::read_to_string(p.version_record()).unwrap(), "old-version");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn partial_state_creation_is_rolled_back() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    let result = migrate(&p, &MigrationOptions { fail_state_create: true, ..Default::default() }, &mut TestKeychain);
    assert!(matches!(result, Err(MigrationError::Io(_))));
    assert!(!p.state_file().exists());
    assert!(!p.version_record().exists());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn every_post_import_crash_rolls_back_new_keychain_item_and_records() {
    for stage in [CrashStage::Converted, CrashStage::VersionRecorded, CrashStage::PreviewCleaned] {
        let root = tempdir();
        let p = paths(&root);
        fs::create_dir_all(&p.legacy_root).unwrap();
        fs::write(p.legacy_root.join(".luno_config.json"), r#"{"lastfm_key":"secret"}"#).unwrap();
        let mut keychain = TrackingKeychain { rollback_count: 0 };
        let result = migrate(&p, &MigrationOptions { crash_at: Some(stage), ..Default::default() }, &mut keychain);
        assert!(matches!(result, Err(MigrationError::InjectedCrash(_))));
        assert_eq!(keychain.rollback_count, 1);
        assert!(!p.state_file().exists());
        assert!(!p.version_record().exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(target_os = "macos")]
#[test]
fn preexisting_keychain_value_is_not_rolled_back() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    fs::write(p.legacy_root.join(".luno_config.json"), r#"{"lastfm_key":"secret"}"#).unwrap();
    let mut keychain = ConfigurableKeychain { import: KeychainImport::Existing, rollback_error: false, rollback_count: 0 };
    let result = migrate(&p, &MigrationOptions { crash_at: Some(CrashStage::Converted), ..Default::default() }, &mut keychain);
    assert!(matches!(result, Err(MigrationError::InjectedCrash(CrashStage::Converted))));
    assert_eq!(keychain.rollback_count, 0);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn rollback_aggregates_primary_keychain_and_preview_errors() {
    let root = tempdir();
    let p = paths(&root);
    fs::create_dir_all(&p.legacy_root).unwrap();
    fs::create_dir_all(&p.preview_dir).unwrap();
    fs::write(p.preview_dir.join("preview.mp3"), "preview").unwrap();
    let mut keychain = ConfigurableKeychain { import: KeychainImport::Created, rollback_error: true, rollback_count: 0 };
    let result = migrate(&p, &MigrationOptions { crash_at: Some(CrashStage::PreviewCleaned), fail_preview_restore: true, ..Default::default() }, &mut keychain);
    let error = result.unwrap_err();
    assert!(matches!(error, MigrationError::Cleanup(message) if message.contains("InjectedCrash") && message.contains("Keychain rollback failed") && message.contains("preview restoration failed")));
    assert_eq!(keychain.rollback_count, 1);
    fs::remove_dir_all(root).unwrap();
}
