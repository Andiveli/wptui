use std::ffi::CStr;

use crate::abi::{
    C_SetEventHandler, CChatEvent, CEvent, CLogoutResultEvent, CMarkChatAsReadEvent,
    CMessageActionEvent, CReactionEvent, CReceipt, EventType, LogoutStatus, ReceiptKind,
};
use crate::callbacks::CallbackTranslator;
use crate::{Event, JID, MessageActionKind};

impl CallbackTranslator<*const CEvent> for Event {
    unsafe fn to_rust(ptr: *const CEvent) -> Self {
        let event = unsafe { &(*ptr) };
        match EventType::from_repr(event.event_type).unwrap() {
            EventType::SyncProgress => {
                let percent = unsafe { *(event.data as *const u8) };
                Event::SyncProgress(percent)
            }
            EventType::AppStateSyncComplete => Event::AppStateSyncComplete,
            EventType::Receipt => {
                let receipt = unsafe { &(*(event.data as *const CReceipt)) };
                let chat: JID = (&receipt.chat).into();
                let message_ids = unsafe {
                    std::slice::from_raw_parts(receipt.message_ids, receipt.count as usize)
                }
                .iter()
                .map(|&id| {
                    unsafe { CStr::from_ptr(id) }
                        .to_string_lossy()
                        .into_owned()
                        .into()
                })
                .collect();

                Event::Receipt {
                    kind: ReceiptKind::from_repr(receipt.kind).unwrap_or(ReceiptKind::Read),
                    chat,
                    message_ids,
                }
            }
            EventType::Reaction => unsafe {
                reaction_event_from_ffi(&*(event.data as *const CReactionEvent))
            },
            EventType::Connected => Event::Connected,
            EventType::MessageAction => unsafe {
                message_action_event_from_ffi(&*(event.data as *const CMessageActionEvent))
            },
            EventType::Chat => unsafe { chat_event_from_ffi(&*(event.data as *const CChatEvent)) },
            EventType::LogoutResult => unsafe {
                let result = &(*(event.data as *const CLogoutResultEvent));
                Event::LogoutResult(
                    LogoutStatus::from_repr(result.status).unwrap_or(LogoutStatus::Failed),
                )
            },
            EventType::MarkChatAsRead => unsafe {
                let event = &*(event.data as *const CMarkChatAsReadEvent);
                Event::MarkChatAsRead {
                    chat: (&event.chat).into(),
                    message_id: CStr::from_ptr(event.message_id)
                        .to_string_lossy()
                        .into_owned()
                        .into(),
                    read: event.read,
                    timestamp: event.timestamp,
                    from_me: event.from_me,
                    participant: (!event.participant.is_null())
                        .then(|| (&event.participant).into()),
                }
            },
        }
    }
}

unsafe fn chat_event_from_ffi(event: &CChatEvent) -> Event {
    Event::Chat {
        jid: (&event.chat).into(),
        last_message_time: event.last_message_time,
    }
}

unsafe fn reaction_event_from_ffi(event: &CReactionEvent) -> Event {
    Event::Reaction {
        chat: (&event.chat).into(),
        target_message_id: unsafe { CStr::from_ptr(event.target_message_id) }
            .to_string_lossy()
            .into_owned()
            .into(),
        participant: (&event.participant).into(),
        text: unsafe { CStr::from_ptr(event.text) }
            .to_string_lossy()
            .into_owned()
            .into(),
        is_from_me: event.is_from_me,
    }
}

unsafe fn message_action_event_from_ffi(event: &CMessageActionEvent) -> Event {
    let kind = match event.kind {
        0 => MessageActionKind::Edit {
            replacement: unsafe { CStr::from_ptr(event.replacement) }
                .to_string_lossy()
                .into_owned()
                .into(),
        },
        1 => MessageActionKind::Delete,
        _ => unreachable!("Go only dispatches supported message action kinds"),
    };
    Event::MessageAction {
        action_id: unsafe { CStr::from_ptr(event.action_id) }
            .to_string_lossy()
            .into_owned()
            .into(),
        target_message_id: unsafe { CStr::from_ptr(event.target_message_id) }
            .to_string_lossy()
            .into_owned()
            .into(),
        chat: (&event.chat).into(),
        sender: (&event.sender).into(),
        kind,
        occurred_at: event.occurred_at,
        arrival_order: event.arrival_order,
    }
}

setup_handler!(
    set_event_handler,
    C_SetEventHandler,
    event: *const CEvent => Event
);
