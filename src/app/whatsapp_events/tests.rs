use super::*;
use crate::app::{
    chat_store::write_port::{ChatStoreWritePort, PersistChat, PersistChatMessage, PersistMessage},
    test_support::{
        FakeChatReadCursorPort, FakeCommunityQuery, FakeStatusCursorPort, TestApp, message,
    },
};
use std::{
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
};

struct RecordingChatStoreWritePort(Arc<Mutex<Vec<Chat>>>);

impl ChatStoreWritePort for RecordingChatStoreWritePort {
    fn persist(&self, _: PersistChatMessage) {}

    fn persist_chat(&self, command: PersistChat) {
        self.0.lock().unwrap().push(command.chat);
    }

    fn persist_message(&self, _: PersistMessage) {}
}

struct PanickingChatStoreWritePort;

impl ChatStoreWritePort for PanickingChatStoreWritePort {
    fn persist(&self, _: PersistChatMessage) {}
    fn persist_chat(&self, _: PersistChat) {
        panic!("chat persistence failed")
    }
    fn persist_message(&self, _: PersistMessage) {}
}

fn status_message(sender: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
    let mut message = message(&wr::JID::from("status@broadcast".to_owned()), id, timestamp);
    message.info.sender = sender.clone();
    message
}

#[test]
fn sync_progress_event_updates_history_progress() {
    let mut app = TestApp::new();

    assert!(app.handle_whatsapp_event(wr::Event::SyncProgress(42)));
    assert_eq!(app.history_sync_percent, Some(42));
}

#[test]
fn connection_and_sync_complete_each_query_communities_once() {
    let mut app = TestApp::new();
    let query = FakeCommunityQuery::default();
    query.results.lock().unwrap().extend([
        Err(wr::CommunitiesError::BridgeUnavailable),
        Err(wr::CommunitiesError::BridgeUnavailable),
    ]);
    app.set_community_query(Box::new(query.clone()));

    app.handle_whatsapp_event(wr::Event::Connected);
    assert_eq!(*query.calls.lock().unwrap(), 1);
    app.handle_whatsapp_event(wr::Event::AppStateSyncComplete);
    assert_eq!(*query.calls.lock().unwrap(), 2);
}

#[test]
fn chat_event_updates_memory_and_persists_each_current_canonical_chat() {
    let mut app = TestApp::new();
    let jid = wr::JID::from("chat@example.test".to_owned());
    let persisted = Arc::new(Mutex::new(Vec::new()));
    app.chat_store_write = Box::new(RecordingChatStoreWritePort(persisted.clone()));

    for timestamp in [0, 12, 7, 12] {
        assert!(app.handle_whatsapp_event(wr::Event::Chat {
            jid: jid.clone(),
            last_message_time: timestamp,
        }));
    }

    assert_eq!(app.chats[&jid].last_message_time, Some(12));
    assert_eq!(
        persisted
            .lock()
            .unwrap()
            .iter()
            .map(|chat| chat.last_message_time)
            .collect::<Vec<_>>(),
        [None, Some(12), Some(12), Some(12)]
    );
}

#[test]
fn chat_event_updates_memory_before_persistence_panics() {
    let mut app = TestApp::new();
    let jid = wr::JID::from("chat@example.test".to_owned());
    app.chat_store_write = Box::new(PanickingChatStoreWritePort);

    assert!(
        std::panic::catch_unwind(AssertUnwindSafe(|| {
            app.handle_whatsapp_event(wr::Event::Chat {
                jid: jid.clone(),
                last_message_time: 12,
            });
        }))
        .is_err()
    );
    assert_eq!(app.chats[&jid].last_message_time, Some(12));
}

#[test]
fn reaction_event_is_translated_into_reaction_projection() {
    let mut app = TestApp::new();
    let message_id: wr::MessageId = "message".into();
    let participant = wr::JID::from("participant@example.test".to_owned());

    assert!(app.handle_whatsapp_event(wr::Event::Reaction {
        chat: wr::JID::from("chat@example.test".to_owned()),
        target_message_id: message_id.clone(),
        participant: participant.clone(),
        text: Arc::from("👍"),
        is_from_me: false,
    }));
    assert_eq!(app.reactions[&message_id][&participant].as_ref(), "👍");
}

#[test]
fn remote_read_clears_only_the_covered_unread_range() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let cursor = FakeChatReadCursorPort::default();
    app.chat_read_cursor = Box::new(cursor.clone());
    for (id, timestamp) in [("first", 10), ("middle", 20), ("latest", 30)] {
        app.add_message(message(&chat, id, timestamp));
    }
    assert_eq!(app.pending_new_messages(&chat), 3);

    app.handle_whatsapp_event(wr::Event::MarkChatAsRead {
        chat: chat.clone(),
        message_id: "middle".into(),
        read: true,
        timestamp: 20,
        from_me: false,
        participant: None,
    });

    assert_eq!(app.pending_new_messages(&chat), 1);
    assert_eq!(
        app.timeline[&chat].last_read_message.as_deref(),
        Some("middle")
    );
    assert_eq!(app.timeline[&chat].last_read_at, Some(20));
    let stored = cursor.stored.lock().unwrap();
    assert_eq!(
        (&stored[0].chat, &stored[0].message_id, stored[0].timestamp),
        (&chat, &Some("middle".into()), 20)
    );
}

#[test]
fn remote_read_mutates_timeline_before_cursor_storage_panics() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    app.add_message(message(&chat, "read", 20));
    let cursor = FakeChatReadCursorPort::default();
    *cursor.panic_on_store.lock().unwrap() = true;
    app.chat_read_cursor = Box::new(cursor);

    assert!(
        std::panic::catch_unwind(AssertUnwindSafe(|| {
            app.apply_remote_chat_read(chat.clone(), "read".into(), true, 20, false, None);
        }))
        .is_err()
    );
    assert_eq!(
        app.timeline[&chat].last_read_message.as_deref(),
        Some("read")
    );
    assert_eq!(app.pending_new_messages(&chat), 0);
}

#[test]
fn stale_or_unread_events_do_not_move_the_cursor() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    app.add_message(message(&chat, "old", 10));
    app.add_message(message(&chat, "new", 20));
    app.handle_whatsapp_event(wr::Event::MarkChatAsRead {
        chat: chat.clone(),
        message_id: "new".into(),
        read: true,
        timestamp: 20,
        from_me: false,
        participant: None,
    });
    app.handle_whatsapp_event(wr::Event::MarkChatAsRead {
        chat: chat.clone(),
        message_id: "old".into(),
        read: true,
        timestamp: 10,
        from_me: false,
        participant: None,
    });
    app.handle_whatsapp_event(wr::Event::MarkChatAsRead {
        chat: chat.clone(),
        message_id: "old".into(),
        read: false,
        timestamp: 10,
        from_me: false,
        participant: None,
    });

    assert_eq!(
        app.timeline[&chat].last_read_message.as_deref(),
        Some("new")
    );
    assert_eq!(app.timeline[&chat].last_read_at, Some(20));
}

#[test]
fn remote_read_before_message_is_observed_after_message_arrival() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());

    app.handle_whatsapp_event(wr::Event::MarkChatAsRead {
        chat: chat.clone(),
        message_id: "arriving-later".into(),
        read: true,
        timestamp: 20,
        from_me: false,
        participant: None,
    });
    assert_eq!(app.timeline.get(&chat), None);

    app.add_message(message(&chat, "arriving-later", 20));
    app.handle_whatsapp_event(wr::Event::MarkChatAsRead {
        chat: chat.clone(),
        message_id: "arriving-later".into(),
        read: true,
        timestamp: 20,
        from_me: false,
        participant: None,
    });
    assert_eq!(app.timeline[&chat].last_read_at, Some(20));
}

#[test]
fn remote_read_with_mismatched_key_or_timestamp_is_ignored() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    app.add_message(message(&chat, "known", 20));

    for (message_id, timestamp) in [("unknown", 20), ("known", 21)] {
        app.handle_whatsapp_event(wr::Event::MarkChatAsRead {
            chat: chat.clone(),
            message_id: message_id.into(),
            read: true,
            timestamp,
            from_me: false,
            participant: None,
        });
    }

    assert_eq!(app.timeline[&chat].last_read_message, None);
    assert_eq!(app.timeline[&chat].last_read_at, None);
}

#[test]
fn read_self_advances_chat_cursor_without_mark_chat_as_read_and_keeps_later_unread() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    for (id, timestamp) in [("first", 10), ("covered", 20), ("later", 30)] {
        app.add_message(message(&chat, id, timestamp));
    }

    app.handle_whatsapp_event(wr::Event::Receipt {
        kind: wr::ReceiptKind::ReadSelf,
        chat: chat.clone(),
        message_ids: vec!["covered".into()],
    });

    assert_eq!(
        app.timeline[&chat].last_read_message.as_deref(),
        Some("covered")
    );
    assert_eq!(app.timeline[&chat].last_read_at, Some(20));
    assert_eq!(app.pending_new_messages(&chat), 1);
    assert_eq!(app.messages["covered"].info.read_by, 0);
}

#[test]
fn ordinary_read_and_read_self_have_separate_semantics() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let mut sent = message(&chat, "message", 10);
    sent.info.is_from_me = true;
    app.add_message(sent);

    app.handle_whatsapp_event(wr::Event::Receipt {
        kind: wr::ReceiptKind::Read,
        chat: chat.clone(),
        message_ids: vec!["message".into()],
    });
    assert_eq!(app.messages["message"].info.read_by, 1);

    app.handle_whatsapp_event(wr::Event::Receipt {
        kind: wr::ReceiptKind::ReadSelf,
        chat: chat.clone(),
        message_ids: vec!["message".into()],
    });
    assert_eq!(app.messages["message"].info.read_by, 1);
    assert_eq!(app.timeline[&chat].last_read_at, Some(10));
}

#[test]
fn ordinary_read_of_outgoing_message_updates_peer_read_state() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let mut sent = message(&chat, "sent", 10);
    sent.info.is_from_me = true;
    app.add_message(sent);

    app.handle_whatsapp_event(wr::Event::Receipt {
        kind: wr::ReceiptKind::Read,
        chat: chat.clone(),
        message_ids: vec!["sent".into(), "sent".into()],
    });

    assert_eq!(app.messages["sent"].info.read_by, 2);
}

#[test]
fn ordinary_read_of_incoming_message_advances_local_cursor_and_preserves_later_unread() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let cursor = FakeChatReadCursorPort::default();
    let status_cursor = FakeStatusCursorPort::default();
    app.chat_read_cursor = Box::new(cursor.clone());
    app.status_cursor = Box::new(status_cursor.clone());
    for (id, timestamp) in [("old", 10), ("read", 20), ("later", 30)] {
        app.add_message(message(&chat, id, timestamp));
    }

    app.handle_whatsapp_event(wr::Event::Receipt {
        kind: wr::ReceiptKind::Read,
        chat: chat.clone(),
        message_ids: vec!["read".into()],
    });

    assert_eq!(
        app.timeline[&chat].last_read_message.as_deref(),
        Some("read")
    );
    assert_eq!(app.timeline[&chat].last_read_at, Some(20));
    assert_eq!(app.pending_new_messages(&chat), 1);
    assert_eq!(app.messages["read"].info.read_by, 0);
    let stored = cursor.stored.lock().unwrap();
    assert_eq!(
        (&stored[0].chat, &stored[0].message_id, stored[0].timestamp),
        (&chat, &Some("read".into()), 20)
    );
    assert!(status_cursor.stored.lock().unwrap().is_empty());
}

#[test]
fn ordinary_read_classifies_mixed_ids_independently_and_rejects_wrong_chat() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let other = wr::JID::from("other@example.test".to_owned());
    let mut sent = message(&chat, "sent", 10);
    sent.info.is_from_me = true;
    app.add_message(sent);
    app.add_message(message(&chat, "incoming", 20));
    app.add_message(message(&other, "other", 30));

    app.handle_whatsapp_event(wr::Event::Receipt {
        kind: wr::ReceiptKind::Read,
        chat: chat.clone(),
        message_ids: vec![
            "sent".into(),
            "incoming".into(),
            "other".into(),
            "unknown".into(),
        ],
    });

    assert_eq!(app.messages["sent"].info.read_by, 1);
    assert_eq!(app.messages["incoming"].info.read_by, 0);
    assert_eq!(
        app.timeline[&chat].last_read_message.as_deref(),
        Some("incoming")
    );
    assert_eq!(app.timeline[&chat].last_read_at, Some(20));
    assert_eq!(
        app.timeline
            .get(&other)
            .and_then(|state| state.last_read_at),
        None
    );
}

#[test]
fn ordinary_read_cursor_is_monotonic_for_out_of_order_ids() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    for (id, timestamp) in [("old", 10), ("new", 20)] {
        app.add_message(message(&chat, id, timestamp));
    }

    for ids in [vec!["new".into()], vec!["old".into()], vec!["new".into()]] {
        app.handle_whatsapp_event(wr::Event::Receipt {
            kind: wr::ReceiptKind::Read,
            chat: chat.clone(),
            message_ids: ids,
        });
    }

    assert_eq!(
        app.timeline[&chat].last_read_message.as_deref(),
        Some("new")
    );
    assert_eq!(app.timeline[&chat].last_read_at, Some(20));
}

#[test]
fn read_self_rejects_unknown_and_wrong_chat_ids_and_is_monotonic() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let other = wr::JID::from("other@example.test".to_owned());
    app.add_message(message(&chat, "known", 20));
    app.add_message(message(&other, "other", 30));

    for ids in [vec!["unknown".into()], vec!["other".into()]] {
        app.handle_whatsapp_event(wr::Event::Receipt {
            kind: wr::ReceiptKind::ReadSelf,
            chat: chat.clone(),
            message_ids: ids,
        });
    }
    assert_eq!(app.timeline[&chat].last_read_message, None);
    assert_eq!(app.timeline[&chat].last_read_at, None);

    for ids in [vec!["known".into()], vec!["known".into()]] {
        app.handle_whatsapp_event(wr::Event::Receipt {
            kind: wr::ReceiptKind::ReadSelf,
            chat: chat.clone(),
            message_ids: ids,
        });
    }
    assert_eq!(
        app.timeline[&chat].last_read_message.as_deref(),
        Some("known")
    );
    assert_eq!(app.timeline[&chat].last_read_at, Some(20));
}

#[test]
fn read_self_advances_status_cursors_by_sender_without_read_by() {
    let mut app = TestApp::new();
    let alice = wr::JID::from("alice@example.test".to_owned());
    let status_chat = wr::JID::from("status@broadcast".to_owned());
    app.add_message(status_message(&alice, "status-old", 100));
    app.add_message(status_message(&alice, "status-new", 200));

    for ids in [
        vec!["status-new".into()],
        vec!["status-old".into()],
        vec!["status-new".into()],
    ] {
        app.handle_whatsapp_event(wr::Event::Receipt {
            kind: wr::ReceiptKind::ReadSelf,
            chat: status_chat.clone(),
            message_ids: ids,
        });
    }

    assert_eq!(app.status_last_seen.get(&alice), Some(&200));
    assert!(!app.has_unseen_statuses(&alice));
}

#[test]
fn ordinary_read_of_status_message_persists_view_by_sender() {
    let mut app = TestApp::new();
    let alice = wr::JID::from("alice@example.test".to_owned());
    let status_chat = wr::JID::from("status@broadcast".to_owned());
    app.add_message(status_message(&alice, "status", 200));

    app.handle_whatsapp_event(wr::Event::Receipt {
        kind: wr::ReceiptKind::Read,
        chat: status_chat,
        message_ids: vec!["status".into()],
    });

    assert_eq!(app.status_last_seen.get(&alice), Some(&200));
    assert_eq!(app.messages["status"].info.read_by, 0);
}
