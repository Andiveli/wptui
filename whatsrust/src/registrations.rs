use std::ffi::c_char;

use super::*;

impl CallbackTranslator<bool> for bool {
    unsafe fn to_rust(value: bool) -> Self {
        value
    }
}

impl CallbackTranslator<u64> for u64 {
    unsafe fn to_rust(value: u64) -> Self {
        value
    }
}

impl CallbackTranslator<u8> for u8 {
    unsafe fn to_rust(value: u8) -> Self {
        value
    }
}

impl CallbackTranslator<i64> for i64 {
    unsafe fn to_rust(value: i64) -> Self {
        value
    }
}

pub fn set_presence_handler<F>(mut callback: F)
where
    F: FnMut(PresenceUpdate) + 'static,
{
    setup_presence_handler(move |from, unavailable, last_seen| {
        if std::env::var("WPTUI_PRESENCE_DEBUG").as_deref() == Ok("1") {
            super::presence::record_callback_ingress();
        }
        callback(PresenceUpdate {
            from,
            unavailable,
            last_seen,
        });
    });
}

setup_handler!(
    setup_presence_handler,
    C_SetPresenceHandler,
    from: CJID => JID,
    unavailable: bool => bool,
    last_seen: i64 => i64
);

setup_handler!(
    set_message_handler,
    C_SetMessageHandler,
    msg: *const CMessage => Message,
    is_sync: bool => bool
);

setup_handler!(
    set_optimistic_text_sent_handler,
    C_SetOptimisticTextSentHandler,
    local_send_id: u64 => u64,
    msg: *const CMessage => Message
);

setup_handler!(
    set_log_handler,
    C_SetLogHandler,
    msg: *const c_char => String,
    level: u8 => u8
);
