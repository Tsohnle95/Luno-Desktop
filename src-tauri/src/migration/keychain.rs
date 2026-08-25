//! macOS Keychain adapter for first-run secret migration.

use super::first_run::{Keychain, KeychainImport, MAX_LASTFM_KEY_LEN};
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
        { security::remove(&self.service, &self.account) }
        #[cfg(not(target_os = "macos"))]
        { Err("macOS Keychain is unavailable on this platform".into()) }
    }

    /// Stores (or replaces) the Last.fm API key in the macOS Keychain.
    pub fn save_lastfm_key(&mut self, key: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        { security::set(&self.service, &self.account, key.as_bytes()).map(|_| ()) }
        #[cfg(not(target_os = "macos"))]
        { let _ = key; Err("macOS Keychain is unavailable on this platform".into()) }
    }

    /// Silent read for the one-time seed of the plaintext key file. Passes
    /// kSecUseAuthenticationUI=Deny so this call is structurally incapable of
    /// raising a Keychain prompt: if the caller's identity lacks ACL access,
    /// it simply fails and startup proceeds without the key.
    pub fn try_read_lastfm_key_silent(&mut self) -> Option<Vec<u8>> {
        #[cfg(target_os = "macos")]
        { security::get_silent(&self.service, &self.account) }
        #[cfg(not(target_os = "macos"))]
        { None }
    }

    pub fn read_lastfm_key(&mut self) -> Result<Vec<u8>, String> {
        #[cfg(target_os = "macos")]
        { security::get(&self.service, &self.account) }
        #[cfg(not(target_os = "macos"))]
        { Err("macOS Keychain is unavailable on this platform".into()) }
    }
}

#[cfg(target_os = "macos")]
mod security {
    use super::KeychainImport;
    use std::ffi::c_void;
    use std::ptr;

    type OsStatus = i32;
    type UInt32 = u32;
    type SecKeychainItemRef = *mut c_void;

    const ERR_SEC_ITEM_NOT_FOUND: OsStatus = -25300;
    const ERR_SEC_DUPLICATE_ITEM: OsStatus = -25299;
    const ERR_SEC_CONFLICT: OsStatus = -25298;

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

    pub fn set(service_name: &str, account_name: &str, value: &[u8]) -> Result<KeychainImport, String> {
        let service = service_name.as_bytes();
        let account = account_name.as_bytes();
        let service_len = UInt32::try_from(service.len()).map_err(|_| "invalid Keychain namespace".to_owned())?;
        let account_len = UInt32::try_from(account.len()).map_err(|_| "invalid Keychain account".to_owned())?;
        let value_len = UInt32::try_from(value.len()).map_err(|_| "Keychain value is too large".to_owned())?;
        let mut item = ptr::null_mut();
        let mut existing_length = 0;
        let mut existing_data = ptr::null_mut();
        let status = unsafe {
            SecKeychainFindGenericPassword(
                ptr::null(), service_len, service.as_ptr(), account_len, account.as_ptr(),
                &mut existing_length, &mut existing_data, &mut item,
            )
        };

        if status == 0 {
            let existing = unsafe { std::slice::from_raw_parts(existing_data, existing_length as usize) };
            let same = existing == value;
            unsafe { SecKeychainItemFreeContent(ptr::null(), existing_data); }
            unsafe { CFRelease(item) };
            return if same { Ok(KeychainImport::Existing) } else { Err(status_error(ERR_SEC_CONFLICT)) };
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
            return Ok(KeychainImport::Created);
        }
        // Another process may have created the stable item between Find and Add.
        if added == ERR_SEC_DUPLICATE_ITEM {
            // The item was created between Find and Add. One re-read is enough
            // to converge without allowing an unbounded recursive retry.
            let mut existing = ptr::null_mut();
            let mut existing_length = 0;
            let mut existing_data = ptr::null_mut();
            let found = unsafe { SecKeychainFindGenericPassword(ptr::null(), service_len, service.as_ptr(), account_len, account.as_ptr(), &mut existing_length, &mut existing_data, &mut existing) };
            if found != 0 || existing.is_null() { return Err(status_error(if found == 0 { added } else { found })); }
            let same = unsafe { std::slice::from_raw_parts(existing_data, existing_length as usize) == value };
            unsafe { SecKeychainItemFreeContent(ptr::null(), existing_data); }
            unsafe { CFRelease(existing) };
            return if same { Ok(KeychainImport::Existing) } else { Err(status_error(ERR_SEC_CONFLICT)) };
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
        let result = unsafe { std::slice::from_raw_parts(data, length as usize) }.to_vec();
        unsafe { SecKeychainItemFreeContent(ptr::null(), data); CFRelease(item); }
        Ok(result)
    }

    /// Modern-API read that CANNOT prompt: kSecUseAuthenticationUI=Deny makes
    /// securityd refuse any UI interaction, so an identity without ACL access
    /// simply gets errSecInteractionNotAllowed instead of a dialog.
    pub fn get_silent(service_name: &str, account_name: &str) -> Option<Vec<u8>> {
        use std::ffi::CString;

        type CFTypeRef = *const c_void;
        type CFDictionaryRef = *const c_void;
        type CFDataRef = *const c_void;
        const K_CF_STRING_ENCODING_UTF8: UInt32 = 0x0800_0100;
        const ERR_SEC_ITEM_NOT_FOUND_SILENT: OsStatus = -25300;
        const ERR_SEC_INTERACTION_NOT_ALLOWED: OsStatus = -25308;

        #[link(name = "Security", kind = "framework")]
        unsafe extern "C" {
            static kSecClass: CFTypeRef;
            static kSecClassGenericPassword: CFTypeRef;
            static kSecAttrService: CFTypeRef;
            static kSecAttrAccount: CFTypeRef;
            static kSecReturnData: CFTypeRef;
            static kSecUseAuthenticationUI: CFTypeRef;
            fn SecItemCopyMatching(query: CFDictionaryRef, result: *mut CFTypeRef) -> OsStatus;
        }
        #[link(name = "CoreFoundation", kind = "framework")]
        unsafe extern "C" {
            static kCFBooleanTrue: CFTypeRef;
            static kCFCopyStringDictionaryKeyCallBacks: u8;
            static kCFTypeDictionaryValueCallBacks: u8;
            fn CFStringCreateWithCString(alloc: *const c_void, c_str: *const u8, encoding: UInt32) -> CFTypeRef;
            fn CFDictionaryCreateMutable(alloc: *const c_void, capacity: isize, key_callbacks: *const c_void, value_callbacks: *const c_void) -> *mut c_void;
            fn CFDictionarySetValue(dict: *mut c_void, key: CFTypeRef, value: CFTypeRef);
            fn CFDataGetLength(data: CFDataRef) -> isize;
            fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
        }

        let c_service = CString::new(service_name).ok();
        let c_account = CString::new(account_name).ok();
        let query = unsafe {
            CFDictionaryCreateMutable(
                ptr::null(), 0,
                &kCFCopyStringDictionaryKeyCallBacks as *const u8 as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const u8 as *const c_void,
            )
        };
        let (c_service, c_account) = match (c_service, c_account) {
            (Some(service), Some(account)) => (service, account),
            _ => { unsafe { CFRelease(query) }; return None; }
        };
        let deny = unsafe {
            // kSecUseAuthenticationUIDeny is not exported as a linkable symbol;
            // the framework compares string CONTENT, so build "Deny" directly.
            CFStringCreateWithCString(ptr::null(), b"Deny\0".as_ptr(), K_CF_STRING_ENCODING_UTF8)
        };
        let owned = [
            unsafe { CFStringCreateWithCString(ptr::null(), c_service.as_ptr() as *const u8, K_CF_STRING_ENCODING_UTF8) },
            unsafe { CFStringCreateWithCString(ptr::null(), c_account.as_ptr() as *const u8, K_CF_STRING_ENCODING_UTF8) },
            deny,
        ];
        if owned.iter().any(|item| item.is_null()) {
            unsafe { CFRelease(query) };
            owned.iter().for_each(|item| if !item.is_null() { unsafe { CFRelease(*item) } });
            return None;
        }
        unsafe {
            CFDictionarySetValue(query, kSecClass, kSecClassGenericPassword);
            CFDictionarySetValue(query, kSecAttrService, owned[0]);
            CFDictionarySetValue(query, kSecAttrAccount, owned[1]);
            CFDictionarySetValue(query, kSecReturnData, kCFBooleanTrue);
            CFDictionarySetValue(query, kSecUseAuthenticationUI, owned[2]);
        }

        let mut result: CFTypeRef = ptr::null();
        let status = unsafe { SecItemCopyMatching(query as CFDictionaryRef, &mut result) };
        unsafe { CFRelease(query) };
        owned.iter().for_each(|item| unsafe { CFRelease(*item) });
        if status != 0 || result.is_null() {
            // Item missing (-25300) or access denied without UI (-25308):
            // both mean "no silent key available" — never an error worth raising.
            let _ = (ERR_SEC_ITEM_NOT_FOUND_SILENT, ERR_SEC_INTERACTION_NOT_ALLOWED);
            return None;
        }
        let length = unsafe { CFDataGetLength(result as CFDataRef) };
        if length <= 0 { unsafe { CFRelease(result) }; return None; }
        let bytes = unsafe { std::slice::from_raw_parts(CFDataGetBytePtr(result as CFDataRef), length as usize) }.to_vec();
        unsafe { CFRelease(result) };
        Some(bytes)
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
    fn import_lastfm_key(&mut self, key: &str) -> Result<KeychainImport, String> {
        if key.trim().is_empty() { return Err("Last.fm key is empty".into()); }
        if key.len() > MAX_LASTFM_KEY_LEN { return Err("Last.fm key exceeds maximum length".into()); }
        #[cfg(target_os = "macos")]
        { security::set(&self.service, &self.account, key.as_bytes()) }
        #[cfg(not(target_os = "macos"))]
        { Err("macOS Keychain is unavailable on this platform".into()) }
    }

    fn rollback_lastfm_key(&mut self, import: KeychainImport) -> Result<(), String> {
        match import {
            KeychainImport::Created => self.remove_lastfm_key(),
            KeychainImport::Existing => Ok(()),
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an interactive macOS login Keychain; run explicitly"]
    fn real_keychain_round_trip() -> Result<(), String> {
        let mut keychain = MacOsKeychain::new_unique_for_test();
        let result = (|| {
            keychain.import_lastfm_key("luno-real-keychain-test")?;
            if keychain.read_lastfm_key()? != b"luno-real-keychain-test" { return Err("round-trip value mismatch".into()); }
            Ok(())
        })();
        let cleanup = keychain.remove_lastfm_key();
        if let Err(error) = cleanup { return Err(format!("round-trip cleanup failed: {error}")); }
        result
    }
}
