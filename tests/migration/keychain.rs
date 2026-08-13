use luno_desktop_lib::migration::first_run::{Keychain, MAX_LASTFM_KEY_LEN};
use luno_desktop_lib::migration::keychain::{MacOsKeychain, LASTFM_ACCOUNT, SERVICE};

#[test]
fn namespace_is_stable() {
    assert_eq!(SERVICE, "com.ty.luno");
    assert_eq!(LASTFM_ACCOUNT, "lastfm-key");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn adapter_fails_explicitly_off_macos() {
    let mut keychain = MacOsKeychain::new_for_test();
    let error = keychain.import_lastfm_key("test-key").unwrap_err();
    assert!(error.contains("unavailable"));
}

#[test]
fn oversized_key_is_rejected_before_keychain_access() {
    let mut keychain = MacOsKeychain::new_for_test();
    let error = keychain.import_lastfm_key(&"x".repeat(MAX_LASTFM_KEY_LEN + 1)).unwrap_err();
    assert!(error.contains("exceeds maximum length"));
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires an interactive macOS login Keychain; run explicitly"]
fn real_adapter_check_is_explicit() {
    let mut keychain = MacOsKeychain::new_unique_for_test();
    let result = (|| {
        keychain.import_lastfm_key("luno-real-keychain-test")?;
        if keychain.read_lastfm_key()? != b"luno-real-keychain-test" {
            return Err("round-trip value mismatch".to_owned());
        }
        Ok::<(), String>(())
    })();
    let cleanup = keychain.remove_lastfm_key();
    if let Err(error) = cleanup {
        return Err(format!("round-trip cleanup failed: {error}"));
    }
    result.expect("Keychain access unavailable or denied: real check is inconclusive");
}
