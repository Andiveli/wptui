use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

static PRESENCE_CALLBACK_INGRESS: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn record_callback_ingress() {
    PRESENCE_CALLBACK_INGRESS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) unsafe fn take_owned_c_string(
    value: *mut c_char,
    free: unsafe extern "C" fn(*mut c_char),
) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let result = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned();
    unsafe { free(value) };
    Some(result)
}

pub fn drain_raw_presence_diagnostics() -> Option<String> {
    unsafe {
        let mut report = take_owned_c_string(
            C_DrainRawPresenceDiagnostics(),
            C_FreeRawPresenceDiagnostics,
        );
        if std::env::var("WPTUI_PRESENCE_DEBUG").as_deref() == Ok("1") {
            let ingress = PRESENCE_CALLBACK_INGRESS.swap(0, Ordering::Relaxed);
            report.get_or_insert_default().push_str(&format!(
                "Rust callback ingress Presence events: {ingress}\n"
            ));
        }
        report
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SubscribePresenceResult {
    Accepted = 0,
    NoPrivacyToken = 1,
    Rejected = 2,
}

pub fn subscribe_presence(jid: &JID) -> SubscribePresenceResult {
    let jid = CString::new(jid.0.as_ref()).unwrap();
    match unsafe { C_SubscribePresence(jid.as_ptr()) } {
        0 => SubscribePresenceResult::Accepted,
        1 => SubscribePresenceResult::NoPrivacyToken,
        _ => SubscribePresenceResult::Rejected,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::{CString, c_char},
        sync::atomic::{AtomicBool, Ordering},
    };

    use super::take_owned_c_string;

    static FREED: AtomicBool = AtomicBool::new(false);

    unsafe extern "C" fn free_test_string(value: *mut c_char) {
        drop(unsafe { CString::from_raw(value) });
        FREED.store(true, Ordering::SeqCst);
    }

    #[test]
    fn owned_diagnostic_string_is_copied_and_freed_once() {
        FREED.store(false, Ordering::SeqCst);
        let value = CString::new("raw presence events received: 0\n")
            .unwrap()
            .into_raw();
        let report = unsafe { take_owned_c_string(value, free_test_string) };
        assert_eq!(report.as_deref(), Some("raw presence events received: 0\n"));
        assert!(FREED.load(Ordering::SeqCst));
        assert!(unsafe { take_owned_c_string(std::ptr::null_mut(), free_test_string) }.is_none());
    }
}
