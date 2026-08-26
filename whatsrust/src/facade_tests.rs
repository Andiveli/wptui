#[cfg(test)]
mod file_kind_tests {
    use crate::{FileKind, file_kind_discriminant};

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

    use crate::actions::{edit_to_ffi, reaction_to_ffi, revoke_to_ffi};
    use crate::events::{message_action_event_from_ffi, reaction_event_from_ffi};
    use crate::{CMessageActionEvent, CReactionEvent, Event, JID, MessageActionKind};

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
        let message_id: crate::MessageId = "message".into();
        let (cchat, csender, cid) = revoke_to_ffi(&chat, &sender, &message_id).unwrap();

        assert_eq!(cchat.to_str().unwrap(), "1234567890@s.whatsapp.net");
        assert_eq!(csender.to_str().unwrap(), "0987654321@s.whatsapp.net");
        assert_eq!(cid.to_str().unwrap(), "message");
    }

    #[test]
    fn revoke_mapping_rejects_nul_before_the_ffi_boundary() {
        let jid = JID::from("1234567890@s.whatsapp.net".to_owned());
        let invalid_id: crate::MessageId = "message\0id".into();
        let valid_id: crate::MessageId = "message".into();

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

    use crate::{
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
