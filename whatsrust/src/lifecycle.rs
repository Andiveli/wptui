use std::ffi::{CStr, CString, c_char};

use super::*;
use crate::abi::{
    C_Connect, C_Disconnect, C_FreePairPhoneResult, C_Logout, C_NewClient, C_PairPhone,
};

impl CallbackTranslator<*const c_char> for String {
    unsafe fn to_rust(ptr: *const c_char) -> String {
        let c_str = unsafe { CStr::from_ptr(ptr) };
        c_str.to_string_lossy().into_owned()
    }
}

setup_handler!(connect, C_Connect, qr: *const c_char => String);

pub fn new_client(db_path: &str) {
    let db_path_c = CString::new(db_path).unwrap();
    unsafe { C_NewClient(db_path_c.as_ptr()) }
}

pub fn disconnect() {
    unsafe { C_Disconnect() }
}

pub fn logout() {
    // Performs a deterministic local sign-out on the Go side (disconnect +
    // clear the persisted device). The result arrives via Event::LogoutResult
    // so the app can remove the DB file and quit. No network round-trip, so
    // the UI never blocks.
    unsafe { C_Logout() };
}

pub fn pair_phone(phone: &str) -> String {
    let phone_c = CString::new(phone).unwrap();
    let result = unsafe { C_PairPhone(phone_c.as_ptr()) };
    let result_str = unsafe { CStr::from_ptr(result) }
        .to_string_lossy()
        .into_owned();
    unsafe { C_FreePairPhoneResult(result) };
    result_str
}
