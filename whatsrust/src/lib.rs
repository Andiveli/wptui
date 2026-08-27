use std::ffi::CString;

#[macro_use]
mod callbacks;
mod abi;
mod actions;
mod caches;
mod events;
mod incoming;
mod lifecycle;
mod media;
mod message_send;
mod models;
mod presence;
mod queries;
mod read_sync;
mod registrations;
use abi::*;
pub use abi::{LogoutStatus, ReceiptKind};
pub use actions::{edit_message, react_to_message, react_to_message_in_chat, revoke_message};
pub use callbacks::CallbackTranslator;
pub use events::set_event_handler;
pub use lifecycle::{connect, disconnect, logout, new_client, pair_phone};
pub use media::{download_file, get_community_profile_picture, get_profile_picture};
pub use message_send::{TextSendResult, forward_message, send_message, send_text_message};
pub(crate) use models::file_kind_discriminant;
pub use models::{
    ChatSettings, CommunitiesError, CommunityInfo, Contact, DownloadFailed, Event, FileContent,
    FileId, FileKind, ForwardingInfo, GroupInfo, GroupInfoError, GroupParticipant, JID,
    LogoutError, Mention, Message, MessageActionFailed, MessageActionKind, MessageContent,
    MessageId, MessageInfo, PresenceUpdate, ProfilePicture, ProfilePictureAvailability,
    ProfilePictureError,
};
pub use presence::{SubscribePresenceResult, drain_raw_presence_diagnostics, subscribe_presence};
pub use queries::{
    get_chat_settings, get_communities, get_contacts, get_group_info, get_group_participants,
    resolve_dm_chat,
};
pub use read_sync::{MarkAsReadError, mark_as_read, sync_chat_read};
pub use registrations::{
    set_log_handler, set_message_handler, set_optimistic_text_sent_handler, set_presence_handler,
};
use strum::FromRepr;

#[cfg(test)]
mod file_kind_tests {
    use super::{FileKind, file_kind_discriminant};

    #[test]
    fn ffi_file_kind_discriminants_remain_stable_for_all_media_types() {
        let cases = [
            (FileKind::Image, 0),
            (FileKind::Video, 1),
            (FileKind::Audio, 2),
            (FileKind::Document, 3),
            (FileKind::Sticker, 4),
        ];

        for (kind, expected) in cases {
            assert_eq!(file_kind_discriminant(&kind), expected);
        }
    }
}

#[cfg(test)]
mod message_action_tests {
    use std::ffi::CString;

    use super::actions::{edit_to_ffi, reaction_to_ffi, revoke_to_ffi};
    use super::events::{message_action_event_from_ffi, reaction_event_from_ffi};
    use super::{CMessageActionEvent, CReactionEvent, Event, JID, MessageActionKind};

    #[test]
    fn reaction_mapping_preserves_every_ordinary_message_field() {
        let chat = JID::from("1234567890@s.whatsapp.net".to_owned());
        let sender = JID::from("0987654321@s.whatsapp.net".to_owned());
        let message_id = "message".into();
        let (target, destination, sender, id, reaction) =
            reaction_to_ffi(&chat, &chat, &sender, &message_id, "👍").unwrap();
        assert_eq!(target.to_str().unwrap(), "1234567890@s.whatsapp.net");
        assert_eq!(destination.to_str().unwrap(), "1234567890@s.whatsapp.net");
        assert_eq!(chat.0.as_ref(), "1234567890@s.whatsapp.net");
        assert_eq!(sender.to_str().unwrap(), "0987654321@s.whatsapp.net");
        assert_eq!(id.to_str().unwrap(), "message");
        assert_eq!(reaction.to_str().unwrap(), "👍");
    }

    #[test]
    fn reaction_mapping_rejects_nul_before_the_ffi_boundary() {
        let jid = JID::from("1234567890@s.whatsapp.net".to_owned());
        let invalid_id = "message\0id".into();
        let message_id = "message".into();

        assert!(reaction_to_ffi(&jid, &jid, &jid, &invalid_id, "👍").is_err());
        assert!(reaction_to_ffi(&jid, &jid, &jid, &message_id, "👍\0").is_err());
    }

    #[test]
    fn reaction_event_copies_reactor_and_empty_removal_payload() {
        let chat = CString::new("group@g.us").unwrap();
        let target = CString::new("target").unwrap();
        let reactor = CString::new("reactor@s.whatsapp.net").unwrap();
        for (text, expected) in [("👍", "👍"), ("", "")] {
            let text = CString::new(text).unwrap();
            let event = CReactionEvent {
                chat: chat.as_ptr(),
                target_message_id: target.as_ptr(),
                participant: reactor.as_ptr(),
                text: text.as_ptr(),
                is_from_me: false,
            };
            let Event::Reaction {
                chat,
                target_message_id,
                participant,
                text,
                is_from_me,
            } = (unsafe { reaction_event_from_ffi(&event) })
            else {
                panic!("expected reaction event");
            };
            assert_eq!(chat.0.as_ref(), "group@g.us");
            assert_eq!(target_message_id.as_ref(), "target");
            assert_eq!(participant.0.as_ref(), "reactor@s.whatsapp.net");
            assert_eq!(text.as_ref(), expected);
            assert!(!is_from_me);
        }
    }

    #[test]
    fn edit_mapping_preserves_fields_and_rejects_invalid_replacements() {
        let input_chat = JID::from("1234567890@s.whatsapp.net".to_owned());
        let message_id = "message".into();
        let (chat, id, replacement) = edit_to_ffi(&input_chat, &message_id, "replacement").unwrap();

        assert_eq!(chat.to_str().unwrap(), "1234567890@s.whatsapp.net");
        assert_eq!(id.to_str().unwrap(), "message");
        assert_eq!(replacement.to_str().unwrap(), "replacement");
        assert!(edit_to_ffi(&input_chat, &message_id, "").is_err());
        assert!(edit_to_ffi(&input_chat, &message_id, " \t").is_err());
        assert!(edit_to_ffi(&input_chat, &"message\0id".into(), "replacement").is_err());
    }

    #[test]
    fn revoke_mapping_preserves_every_field() {
        let chat = JID::from("1234567890@s.whatsapp.net".to_owned());
        let sender = JID::from("0987654321@s.whatsapp.net".to_owned());
        let message_id: super::MessageId = "message".into();
        let (cchat, csender, cid) = revoke_to_ffi(&chat, &sender, &message_id).unwrap();

        assert_eq!(cchat.to_str().unwrap(), "1234567890@s.whatsapp.net");
        assert_eq!(csender.to_str().unwrap(), "0987654321@s.whatsapp.net");
        assert_eq!(cid.to_str().unwrap(), "message");
    }

    #[test]
    fn revoke_mapping_rejects_nul_before_the_ffi_boundary() {
        let jid = JID::from("1234567890@s.whatsapp.net".to_owned());
        let invalid_id: super::MessageId = "message\0id".into();
        let valid_id: super::MessageId = "message".into();

        assert!(revoke_to_ffi(&jid, &jid, &invalid_id).is_err());
        assert!(revoke_to_ffi(&jid, &jid, &valid_id).is_ok());
    }

    #[test]
    fn message_action_event_copies_ffi_buffers_before_go_releases_them() {
        let action_id = CString::new("edit-1").unwrap();
        let chat = CString::new("chat@example.test").unwrap();
        let sender = CString::new("sender@example.test").unwrap();
        let target = CString::new("target").unwrap();
        let replacement = CString::new("new body").unwrap();
        let event = CMessageActionEvent {
            action_id: action_id.as_ptr(),
            chat: chat.as_ptr(),
            sender: sender.as_ptr(),
            target_message_id: target.as_ptr(),
            replacement: replacement.as_ptr(),
            occurred_at: 42,
            arrival_order: 7,
            kind: 0,
        };

        let Event::MessageAction {
            action_id,
            target_message_id,
            chat,
            sender,
            kind,
            occurred_at,
            arrival_order,
        } = (unsafe { message_action_event_from_ffi(&event) })
        else {
            panic!("expected message action");
        };
        assert_eq!(action_id.as_ref(), "edit-1");
        assert_eq!(target_message_id.as_ref(), "target");
        assert_eq!(chat.0.as_ref(), "chat@example.test");
        assert_eq!(sender.0.as_ref(), "sender@example.test");
        assert_eq!(
            kind,
            MessageActionKind::Edit {
                replacement: "new body".into()
            }
        );
        assert_eq!((occurred_at, arrival_order), (42, 7));
    }
}

#[cfg(test)]
mod presence_event_tests {
    use std::{
        ffi::CString,
        sync::{Mutex, mpsc},
    };

    use super::{
        C_TestEmitPresenceEvent, C_TestEmitPresenceEventsConcurrently, PresenceUpdate,
        set_presence_handler,
    };

    static PRESENCE_HANDLER_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn go_trampoline_reaches_dedicated_rust_presence_handler() {
        let _guard = PRESENCE_HANDLER_TEST_LOCK.lock().unwrap();
        let (sender, receiver) = mpsc::channel();
        set_presence_handler(move |update| sender.send(update).unwrap());
        let from = CString::new("alice@s.whatsapp.net").unwrap();

        unsafe { C_TestEmitPresenceEvent(from.as_ptr(), true, 42) };

        let PresenceUpdate {
            from,
            unavailable,
            last_seen,
        } = receiver.recv().unwrap();
        assert_eq!(from.0.as_ref(), "alice@s.whatsapp.net");
        assert!(unavailable);
        assert_eq!(last_seen, 42);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn concurrent_go_goroutines_reach_dedicated_rust_presence_handler() {
        let _guard = PRESENCE_HANDLER_TEST_LOCK.lock().unwrap();
        const COUNT: u32 = 64;
        let (sender, receiver) = mpsc::channel();
        set_presence_handler(move |update| sender.send(update).unwrap());
        let from = CString::new("alice@s.whatsapp.net").unwrap();

        unsafe { C_TestEmitPresenceEventsConcurrently(from.as_ptr(), COUNT) };

        let mut last_seen = (0..COUNT as i64).collect::<Vec<_>>();
        let mut received = (0..COUNT)
            .map(|_| receiver.recv().unwrap())
            .collect::<Vec<_>>();
        received.sort_by_key(|update| update.last_seen);
        for (update, expected) in received.iter().zip(last_seen.drain(..)) {
            assert_eq!(update.from.0.as_ref(), "alice@s.whatsapp.net");
            assert!(!update.unavailable);
            assert_eq!(update.last_seen, expected);
        }
    }
}

pub use caches::{forward_source, message_mention_ranges, message_push_name};
pub use caches::{
    remove_forward_source, store_forward_source, store_message_mention_ranges,
    store_message_push_name,
};

pub const VIEW_ONCE_UNAVAILABLE_DESCRIPTION: &str =
    "View-once media is unavailable here. View it on your phone.";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ForwardReport {
    pub succeeded: usize,
    pub failed: usize,
    pub failure: ForwardFailure,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, FromRepr)]
#[repr(u8)]
pub enum ForwardFailure {
    #[default]
    None = 0,
    SourceUnavailable = 1,
    InvalidSource = 2,
    InvalidDestination = 3,
    SendFailed = 4,
}

impl ForwardReport {
    pub fn with_reason(succeeded: usize, failed: usize, failure: ForwardFailure) -> Self {
        Self {
            succeeded,
            failed,
            failure,
        }
    }
}

impl CallbackTranslator<CJID> for JID {
    unsafe fn to_rust(from: CJID) -> Self {
        (&from).into()
    }
}

impl CallbackTranslator<i64> for i64 {
    unsafe fn to_rust(value: i64) -> Self {
        value
    }
}
