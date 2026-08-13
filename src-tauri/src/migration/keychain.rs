//! macOS Keychain adapter for first-run secret migration.

use super::first_run::Keychain;

pub const SERVICE: &str = "com.ty.luno";
pub const LASTFM_ACCOUNT: &str = "lastfm-key";

/// Store the legacy Last.fm key in the user's login Keychain.
pub struct MacOsKeychain;

impl MacOsKeychain {
    pub fn new() -> Self { Self }
}

#[cfg(target_os = "macos")]
mod security {
    use super::{LASTFM_ACCOUNT, SERVICE};
    use std::ffi::c_void;
    use std::ptr;

    type OsStatus = i32;
    type UInt32 = u32;
    type SecKeychainItemRef = *mut c_void;

    const ERR_SEC_ITEM_NOT_FOUND: OsStatus = -25300;
    const ERR_SEC_DUPLICATE_ITEM: OsStatus = -25299;

    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SecKeychainFindGenericPassword(
            keychain: *const c_void,
            service_name_length: UInt32,
            service_name: *const u8,
            account_name_length: UInt32,
            account_name: *const u8,
            password_length: *mut UInt32,
            password_data: *mut *mut u8,
            item_ref: *mut SecKeychainItemRef,
        ) -> OsStatus;
        fn SecKeychainAddGenericPassword(
            keychain: *const c_void,
            service_name_length: UInt32,
            service_name: *const u8,
            account_name_length: UInt32,
            account_name: *const u8,
            password_length: UInt32,
            password_data: *const u8,
            item_ref: *mut SecKeychainItemRef,
        ) -> OsStatus;
        fn SecKeychainItemModifyAttributesAndData(
            item_ref: SecKeychainItemRef,
            attr_list: *const c_void,
            length: UInt32,
            data: *const u8,
        ) -> OsStatus;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    fn status_error(status: OsStatus) -> String {
        // Do not expose Keychain-provided text: it can contain account details.
        format!("macOS Keychain operation failed (status {status})")
    }

    pub fn set(value: &[u8]) -> Result<(), String> {
        let service = SERVICE.as_bytes();
        let account = LASTFM_ACCOUNT.as_bytes();
        let service_len = UInt32::try_from(service.len()).map_err(|_| "invalid Keychain namespace".to_owned())?;
        let account_len = UInt32::try_from(account.len()).map_err(|_| "invalid Keychain account".to_owned())?;
        let value_len = UInt32::try_from(value.len()).map_err(|_| "Keychain value is too large".to_owned())?;
        let mut item = ptr::null_mut();
        let status = unsafe {
            SecKeychainFindGenericPassword(
                ptr::null(), service_len, service.as_ptr(), account_len, account.as_ptr(),
                ptr::null_mut(), ptr::null_mut(), &mut item,
            )
        };

        if status == 0 {
            let modified = unsafe { SecKeychainItemModifyAttributesAndData(item, ptr::null(), value_len, value.as_ptr()) };
            unsafe { CFRelease(item) };
            return if modified == 0 { Ok(()) } else { Err(status_error(modified)) };
        }
        if status != ERR_SEC_ITEM_NOT_FOUND { return Err(status_error(status)); }

        let added = unsafe {
            SecKeychainAddGenericPassword(
                ptr::null(), service_len, service.as_ptr(), account_len, account.as_ptr(),
                value_len, value.as_ptr(), &mut item,
            )
        };
        if added == 0 {
            if !item.is_null() { unsafe { CFRelease(item) }; }
            return Ok(());
        }
        // Another process may have created the stable item between Find and Add.
        if added == ERR_SEC_DUPLICATE_ITEM {
            return set(value);
        }
        Err(status_error(added))
    }
}

impl Keychain for MacOsKeychain {
    fn import_lastfm_key(&mut self, key: &str) -> Result<(), String> {
        if key.trim().is_empty() { return Err("Last.fm key is empty".into()); }
        #[cfg(target_os = "macos")]
        { security::set(key.as_bytes()) }
        #[cfg(not(target_os = "macos"))]
        { Err("macOS Keychain is unavailable on this platform".into()) }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an interactive macOS login Keychain; run explicitly"]
    fn real_keychain_round_trip() {
        let mut keychain = MacOsKeychain::new();
        keychain.import_lastfm_key("luno-real-keychain-test")
            .expect("Keychain access unavailable or denied: real check is inconclusive");
    }
}
