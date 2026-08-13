use luno_desktop_lib::migration::first_run::Keychain;
use luno_desktop_lib::migration::keychain::{MacOsKeychain, LASTFM_ACCOUNT, SERVICE};

#[test]
fn namespace_is_stable() {
    assert_eq!(SERVICE, "com.ty.luno");
    assert_eq!(LASTFM_ACCOUNT, "lastfm-key");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn adapter_fails_explicitly_off_macos() {
    let mut keychain = MacOsKeychain::new();
    let error = keychain.import_lastfm_key("test-key").unwrap_err();
    assert!(error.contains("unavailable"));
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires an interactive macOS login Keychain; run explicitly"]
fn real_adapter_check_is_explicit() {
    let mut keychain = MacOsKeychain::new();
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = MacOsKeychain::new().remove_lastfm_key();
        }
    }
    let cleanup = Cleanup;
    let result = (|| {
        keychain.import_lastfm_key("luno-real-keychain-test")?;
        if keychain.read_lastfm_key()? != b"luno-real-keychain-test" {
            return Err("round-trip value mismatch".to_owned());
        }
        Ok::<(), String>(())
    })();
    drop(cleanup);
    result.expect("Keychain access unavailable or denied: real check is inconclusive");
}
