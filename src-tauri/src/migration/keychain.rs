//! macOS Keychain adapter for first-run secret migration.

use super::first_run::{Keychain, MAX_LASTFM_KEY_LEN};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SERVICE: &str = "com.ty.luno";
pub const LASTFM_ACCOUNT: &str = "lastfm-key";

/// Store the legacy Last.fm key in the user's login Keychain.
pub struct MacOsKeychain { service: String, account: String }

impl MacOsKeychain {
    pub fn new() -> Self { Self { service: SERVICE.into(), account: LASTFM_ACCOUNT.into() } }

    pub fn new_for_test() -> Self { Self { service: "com.ty.luno.test".into(), account: "lastfm-key-test".into() } }

    /// Use a fresh namespace so an interactive test can never overwrite user data.
    pub fn new_unique_for_test() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        Self { service: format!("com.ty.luno.test.{nonce}"), account: "lastfm-key-test".into() }
    }

    pub fn remove_lastfm_key(&mut self) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        { security::remove(self.service, self.account) }
        #[cfg(not(target_os = "macos"))]
        { Err("macOS Keychain is unavailable on this platform".into()) }
    }

    pub fn read_lastfm_key(&mut self) -> Result<Vec<u8>, String> {
        #[cfg(target_os = "macos")]
        { security::get(self.service, self.account) }
        #[cfg(not(target_os = "macos"))]
        { Err("macOS Keychain is unavailable on this platform".into()) }
    }
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
        fn SecKeychainItemFreeContent(attr_list: *const c_void, data: *mut u8) -> OsStatus;
        fn SecKeychainItemDelete(item_ref: SecKeychainItemRef) -> OsStatus;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    fn status_error(status: OsStatus) -> String {
        // Do not expose Keychain-provided text: it can contain account details.
        format!("macOS Keychain operation failed (status {status})")
    }

    pub fn set(service_name: &str, account_name: &str, value: &[u8]) -> Result<(), String> {
        let service = service_name.as_bytes();
        let account = account_name.as_bytes();
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
            // The item was created between Find and Add. One re-read is enough
            // to converge without allowing an unbounded recursive retry.
            let mut existing = ptr::null_mut();
            let found = unsafe { SecKeychainFindGenericPassword(ptr::null(), service_len, service.as_ptr(), account_len, account.as_ptr(), ptr::null_mut(), ptr::null_mut(), &mut existing) };
            if found != 0 || existing.is_null() { return Err(status_error(if found == 0 { added } else { found })); }
            let modified = unsafe { SecKeychainItemModifyAttributesAndData(existing, ptr::null(), value_len, value.as_ptr()) };
            unsafe { CFRelease(existing) };
            return if modified == 0 { Ok(()) } else { Err(status_error(modified)) };
        }
        Err(status_error(added))
    }

    pub fn get(service_name: &str, account_name: &str) -> Result<Vec<u8>, String> {
        let service = service_name.as_bytes();
        let account = account_name.as_bytes();
        let mut length = 0;
        let mut data = ptr::null_mut();
        let mut item = ptr::null_mut();
        let status = unsafe { SecKeychainFindGenericPassword(ptr::null(), service.len() as UInt32, service.as_ptr(), account.len() as UInt32, account.as_ptr(), &mut length, &mut data, &mut item) };
        if status != 0 { return Err(status_error(status)); }
        let result = std::slice::from_raw_parts(data, length as usize).to_vec();
        unsafe { SecKeychainItemFreeContent(ptr::null(), data); CFRelease(item); }
        Ok(result)
    }

    pub fn remove(service_name: &str, account_name: &str) -> Result<(), String> {
        let service = service_name.as_bytes();
        let account = account_name.as_bytes();
        let mut item = ptr::null_mut();
        let status = unsafe { SecKeychainFindGenericPassword(ptr::null(), service.len() as UInt32, service.as_ptr(), account.len() as UInt32, account.as_ptr(), ptr::null_mut(), ptr::null_mut(), &mut item) };
        if status == ERR_SEC_ITEM_NOT_FOUND { return Ok(()); }
        if status != 0 { return Err(status_error(status)); }
        let deleted = unsafe { SecKeychainItemDelete(item) };
        unsafe { CFRelease(item); }
        if deleted == 0 { Ok(()) } else { Err(status_error(deleted)) }
    }
}

impl Keychain for MacOsKeychain {
    fn import_lastfm_key(&mut self, key: &str) -> Result<(), String> {
        if key.trim().is_empty() { return Err("Last.fm key is empty".into()); }
        if key.len() > MAX_LASTFM_KEY_LEN { return Err("Last.fm key exceeds maximum length".into()); }
        #[cfg(target_os = "macos")]
        { security::set(self.service, self.account, key.as_bytes()) }
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
        let mut keychain = MacOsKeychain::new_unique_for_test();
        let result = (|| {
            keychain.import_lastfm_key("luno-real-keychain-test")?;
            if keychain.read_lastfm_key()? != b"luno-real-keychain-test" { return Err("round-trip value mismatch".into()); }
            Ok(())
        })();
        let cleanup = keychain.remove_lastfm_key();
        if let Err(error) = cleanup { return Err(format!("round-trip cleanup failed: {error}")); }
        result.expect("Keychain access unavailable or denied: real check is inconclusive");
    }
}
