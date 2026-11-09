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

    use crate::abi::{CMessageActionEvent, CReactionEvent};
    use crate::actions::{edit_to_ffi, reaction_to_ffi, revoke_to_ffi};
    use crate::events::{message_action_event_from_ffi, reaction_event_from_ffi};
    use crate::{Event, JID, MessageActionKind};

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
mod native_link_preview_boundary_tests {
    use crate::{
        CallbackTranslator, Message, MessageContent,
        abi::{CIncomingTextMessage, CMessage, CMessageInfo, CNativeLinkPreview},
    };
    use std::{ffi::CString, ptr};

    #[test]
    fn incoming_translator_owns_preview_after_all_callback_buffers_are_released() {
        let message = {
            let text = CString::new("原文 https://example.test/native").unwrap();
            let matched = CString::new("https://example.test/native").unwrap();
            let title = CString::new("标题 🙂 שלום").unwrap();
            let description = CString::new("Native description").unwrap();
            let thumbnail = vec![0xff, 0xd8, 0xff, 0xd9];
            let native = CNativeLinkPreview {
                matched_text: matched.as_ptr(),
                matched_text_len: matched.as_bytes().len(),
                title: title.as_ptr(),
                title_len: title.as_bytes().len(),
                description: description.as_ptr(),
                description_len: description.as_bytes().len(),
                jpeg_thumbnail: thumbnail.as_ptr(),
                jpeg_thumbnail_len: thumbnail.len(),
            };
            let content = CIncomingTextMessage {
                text: text.as_ptr(),
                mention_ranges: ptr::null(),
                mention_range_count: 0,
                native_preview: &native,
            };
            let id = CString::new("native-ffi-owned").unwrap();
            let chat = CString::new("native-ffi@example.test").unwrap();
            let quote = CString::new("quoted").unwrap();
            let input = CMessage {
                info: CMessageInfo {
                    id: id.as_ptr(),
                    chat: chat.as_ptr(),
                    sender: chat.as_ptr(),
                    push_name: ptr::null(),
                    mentions_self: false,
                    timestamp: 42,
                    is_from_me: false,
                    quote_id: quote.as_ptr(),
                    read_by: 0,
                    is_forwarded: false,
                    forwarding_score: 0,
                },
                message_type: 0,
                message: (&content as *const CIncomingTextMessage).cast(),
                forward_source: ptr::null(),
                forward_source_len: 0,
            };
            unsafe { Message::to_rust(&input) }
        };
        assert_eq!(message.info.quote_id.as_deref(), Some("quoted"));
        assert_eq!(message.info.timestamp, 42);
        assert!(!message.info.is_from_me);
        let MessageContent::Text(text) = &message.message else {
            panic!("text contract changed");
        };
        assert_eq!(text.as_ref(), "原文 https://example.test/native");
        let native = crate::take_native_link_preview(&message.info, text).unwrap();
        assert_eq!(native.matched_text.as_ref(), "https://example.test/native");
        assert_eq!(native.title.as_ref(), "标题 🙂 שלום");
        assert_eq!(native.description.as_ref(), "Native description");
        assert_eq!(native.jpeg_thumbnail, [0xff, 0xd8, 0xff, 0xd9]);
        assert!(crate::take_native_link_preview(&message.info, text).is_none());
    }

    #[test]
    fn ffi_lengths_and_nulls_are_checked_before_slicing() {
        use crate::native_link_preview::copy_from_ffi;
        let matched = CString::new("https://example.test").unwrap();
        let mut native = CNativeLinkPreview {
            matched_text: matched.as_ptr(),
            matched_text_len: matched.as_bytes().len(),
            title: ptr::null(),
            title_len: 0,
            description: ptr::null(),
            description_len: 0,
            jpeg_thumbnail: ptr::null(),
            jpeg_thumbnail_len: 0,
        };
        assert!(unsafe { copy_from_ffi(ptr::null()) }.is_none());
        assert!(
            unsafe { copy_from_ffi(&native) }
                .unwrap()
                .jpeg_thumbnail
                .is_empty()
        );
        native.title_len = crate::NATIVE_TITLE_LIMIT + 1;
        assert!(unsafe { copy_from_ffi(&native) }.is_none());
        native.title_len = 1;
        assert!(unsafe { copy_from_ffi(&native) }.is_none());
        native.title_len = 0;
        native.jpeg_thumbnail_len = 1;
        assert!(unsafe { copy_from_ffi(&native) }.is_none());
        native.jpeg_thumbnail_len = crate::NATIVE_THUMBNAIL_LIMIT + 1;
        assert!(
            unsafe { copy_from_ffi(&native) }
                .unwrap()
                .jpeg_thumbnail
                .is_empty()
        );
        native.matched_text_len = crate::NATIVE_MATCHED_TEXT_LIMIT + 1;
        assert!(unsafe { copy_from_ffi(&native) }.is_none());
    }
}

#[cfg(test)]
mod presence_event_tests {
    use std::{
        ffi::CString,
        sync::{Mutex, mpsc},
    };

    use crate::{
        PresenceUpdate,
        abi::{C_TestEmitPresenceEvent, C_TestEmitPresenceEventsConcurrently},
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
