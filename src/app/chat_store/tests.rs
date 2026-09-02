use super::hydration_port::{ChatStoreHydration, ChatStoreHydrationPort};
use super::*;
use crate::app::{
    Chat,
    test_support::{FakeChatReadCursorPort, FakeContactSource, FakeStatusCursorPort, TestApp},
};
use std::{
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct RecordingContactWriter(Arc<Mutex<Vec<PersistContact>>>);

impl ContactWritePort for RecordingContactWriter {
    fn persist(&self, command: PersistContact) {
        self.0.lock().unwrap().push(command);
    }
}

struct PanickingContactWriter;

impl ContactWritePort for PanickingContactWriter {
    fn persist(&self, _: PersistContact) {
        panic!("persistence failed");
    }
}

struct FakeChatStoreHydrationPort {
    chat: Chat,
    contact: wr::JID,
    message: wr::Message,
    reaction_participant: wr::JID,
}

impl ChatStoreHydrationPort for FakeChatStoreHydrationPort {
    fn load(&self) -> ChatStoreHydration {
        ChatStoreHydration {
            chats: vec![self.chat.clone()],
            contacts: vec![(self.contact.clone(), "Persisted Contact".into())],
            messages: vec![self.message.clone()],
            reactions: vec![(
                self.message.info.id.clone(),
                self.reaction_participant.clone(),
                "✨".into(),
            )],
        }
    }
}

#[test]
fn contact_refresh_updates_memory_and_persists_ordered_commands() {
    let mut app = TestApp::new();
    let writer = RecordingContactWriter::default();
    app.contact_write = Box::new(writer.clone());
    let first = wr::JID::from("first@example.test".to_owned());
    let second = wr::JID::from("second@example.test".to_owned());

    app.apply_contact_refresh(vec![
        (first.clone(), "First".into()),
        (second.clone(), "Second".into()),
    ]);

    assert_eq!(app.contacts[&first].as_ref(), "First");
    assert_eq!(app.contacts[&second].as_ref(), "Second");
    let commands = writer.0.lock().unwrap();
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].jid, first);
    assert_eq!(commands[0].name.as_ref(), "First");
    assert_eq!(commands[1].jid, second);
    assert_eq!(commands[1].name.as_ref(), "Second");
}

#[test]
fn get_contacts_queries_source_once_and_applies_persisted_rows() {
    let mut app = TestApp::new();
    let source = FakeContactSource::default();
    let writer = RecordingContactWriter::default();
    let jid = wr::JID::from("alice@example.test".to_owned());
    source
        .rows
        .lock()
        .unwrap()
        .push((jid.clone(), "Alice".into()));
    app.contact_write = Box::new(writer.clone());
    app.set_contact_source(Box::new(source.clone()));

    app.get_contacts();

    assert_eq!(*source.calls.lock().unwrap(), 1);
    assert_eq!(app.contacts[&jid].as_ref(), "Alice");
    assert_eq!(writer.0.lock().unwrap()[0].jid, jid);
}

#[test]
fn apply_contact_refresh_does_not_query_source() {
    let mut app = TestApp::new();
    let source = FakeContactSource::default();
    app.set_contact_source(Box::new(source.clone()));

    app.apply_contact_refresh(vec![(
        "alice@example.test".to_owned().into(),
        "Alice".into(),
    )]);

    assert_eq!(*source.calls.lock().unwrap(), 0);
}

#[test]
fn contact_refresh_persists_raw_names_but_displays_canonical_names() {
    let mut app = TestApp::new();
    let writer = RecordingContactWriter::default();
    app.contact_write = Box::new(writer.clone());
    let jid = wr::JID::from("alice@example.test".to_owned());

    app.apply_contact_refresh(vec![(jid.clone(), "  ~ Alice  ".into())]);

    assert_eq!(app.contacts[&jid].as_ref(), "  ~ Alice  ");
    assert_eq!(app.contact_name(&jid).as_ref(), "Alice");
    assert_eq!(writer.0.lock().unwrap()[0].name.as_ref(), "  ~ Alice  ");
}

#[test]
fn contact_refresh_keeps_stale_contacts_and_updates_memory_before_persistence() {
    let mut app = TestApp::new();
    let stale = wr::JID::from("stale@example.test".to_owned());
    let fresh = wr::JID::from("fresh@example.test".to_owned());
    app.contacts.insert(stale.clone(), "Stale".into());
    app.contact_write = Box::new(PanickingContactWriter);

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        app.apply_contact_refresh(vec![(fresh.clone(), "Fresh".into())]);
    }));

    assert!(result.is_err());
    assert_eq!(app.contacts[&fresh].as_ref(), "Fresh");
    assert_eq!(app.contacts[&stale].as_ref(), "Stale");
}

fn message(chat: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: chat.clone(),
            mentions_self: false,
            timestamp,
            forwarding: Default::default(),
            is_from_me: false,
            quote_id: None,
            read_by: 0,
        },
        message: wr::MessageContent::Text(id.into()),
    }
}

#[test]
fn receipt_coordination_has_a_dedicated_owner() {
    let chat_store = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/app/chat_store.rs"
    ))
    .expect("chat store source should be readable");
    let receipts = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/app/chat_store/receipts.rs"
    ))
    .expect("receipt owner source should be readable");

    assert!(receipts.contains("pub(crate) fn apply_receipt"));
    assert!(receipts.contains("fn apply_local_read_cursor"));
    assert!(!chat_store.contains("pub(crate) fn apply_receipt"));
}

#[test]
fn read_state_policy_owns_read_transitions_and_cursor_restoration() {
    let chat_store = include_str!("../chat_store.rs");
    let hydration = include_str!("hydration.rs");
    let read_state = include_str!("read_state.rs");

    for method in [
        "fn is_viewing_latest_message",
        "pub fn mark_chat_read_at_latest",
        "pub(crate) fn apply_remote_chat_read",
        "pub fn unread_boundary",
        "pub(super) fn restore_read_cursors",
    ] {
        assert!(read_state.contains(method));
        assert!(!chat_store.contains(method));
    }
    assert!(read_state.contains("chat_read_cursor.load"));
    assert!(hydration.contains("self.restore_read_cursors()"));
}

#[test]
fn persisted_chat_cursor_restores_into_read_state() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let message_id = wr::MessageId::from("read-message");
    let cursor = FakeChatReadCursorPort::default();
    cursor
        .loaded
        .lock()
        .unwrap()
        .push((chat.clone(), message_id.clone(), 42));
    app.chat_read_cursor = Box::new(cursor);

    app.restore_read_cursors();

    assert_eq!(app.timeline[&chat].last_read_message, Some(message_id));
    assert_eq!(app.timeline[&chat].last_read_at, Some(42));
    assert_eq!(app.timeline[&chat].pending_new_messages, 0);
}

#[test]
fn persisted_chat_cursor_survives_hydration_reload() {
    let directory = tempfile::tempdir().unwrap();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let message_id = wr::MessageId::from("read-message");
    {
        let app = TestApp::with_database(directory.path());
        app.db_handler
            .set_last_read_cursor(&chat, Some(message_id.clone()), 42);
    }

    let mut app = TestApp::with_database(directory.path());
    app.load_data_from_db();

    assert_eq!(app.timeline[&chat].last_read_message, Some(message_id));
    assert_eq!(app.timeline[&chat].last_read_at, Some(42));
}

#[test]
fn hydration_port_populates_selected_persisted_projections_without_out_of_scope_state() {
    let mut app = TestApp::new();
    let chat_jid = wr::JID::from("hydrated-chat@example.test".to_owned());
    let contact = wr::JID::from("persisted-contact@example.test".to_owned());
    let reaction_participant = wr::JID::from("reactor@example.test".to_owned());
    let hydrated_message = message(&chat_jid, "hydrated-message", 42);

    app.chat_store_hydration = Box::new(FakeChatStoreHydrationPort {
        chat: Chat {
            jid: chat_jid.clone(),
            last_message_time: Some(42),
        },
        contact: contact.clone(),
        message: hydrated_message.clone(),
        reaction_participant: reaction_participant.clone(),
    });

    app.load_data_from_db();

    assert_eq!(app.chats[&chat_jid].last_message_time, Some(42));
    assert_eq!(app.contacts[&contact].as_ref(), "Persisted Contact");
    let loaded_message = &app.messages[&hydrated_message.info.id];
    assert_eq!(loaded_message.info.id, hydrated_message.info.id);
    assert_eq!(loaded_message.info.chat, hydrated_message.info.chat);
    assert_eq!(
        loaded_message.info.timestamp,
        hydrated_message.info.timestamp
    );
    assert_eq!(
        app.reactions[&wr::MessageId::from("hydrated-message")][&reaction_participant].as_ref(),
        "✨"
    );
    assert!(app.message_actions.is_empty());
    assert!(app.status_last_seen.is_empty());
}

#[test]
fn adding_a_message_registers_chat_and_indexes_message() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());

    app.add_message(message(&chat, "message", 42));

    assert_eq!(app.chats[&chat].last_message_time, Some(42));
    assert_eq!(
        app.chat_messages[&chat],
        vec![wr::MessageId::from("message")]
    );
}

#[test]
fn newer_message_revision_replaces_existing_body_without_duplicate_index() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    app.add_message(message(&chat, "message", 10));
    app.add_message(message(&chat, "message", 20));

    assert_eq!(
        app.messages[&wr::MessageId::from("message")].info.timestamp,
        20
    );
    assert_eq!(app.chat_messages[&chat].len(), 1);
}

#[test]
fn newer_same_id_echo_cannot_downgrade_ownership() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let mut own = message(&chat, "message", 10);
    own.info.is_from_me = true;
    app.add_message(own);

    app.add_message(message(&chat, "message", 20));

    let stored = &app.messages[&wr::MessageId::from("message")];
    assert_eq!(stored.info.timestamp, 20);
    assert!(stored.info.is_from_me);
}

#[test]
fn chat_cursor_sync_schedules_only_on_a_cursor_transition() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@g.us".to_owned());
    let cursor = FakeChatReadCursorPort::default();
    app.chat_read_cursor = Box::new(cursor.clone());
    app.add_message(message(&chat, "latest", 42));

    assert!(app.mark_chat_read_at_latest(&chat));
    let stored = cursor.stored.lock().unwrap();
    assert_eq!(
        (&stored[0].chat, &stored[0].message_id, stored[0].timestamp),
        (&chat, &Some("latest".into()), 42)
    );
    drop(stored);
    assert_eq!(app.mark_chat_read_at_latest(&chat), false);
}

#[test]
fn status_cursor_never_schedules_chat_app_state_sync() {
    let mut app = TestApp::new();
    let status = wr::JID::from(super::super::STATUS_BROADCAST_CHAT.to_owned());
    app.add_message(message(&status, "status", 42));

    assert!(!app.mark_chat_read_at_latest(&status));
}

#[test]
fn status_receipt_keeps_memory_unchanged_when_cursor_storage_fails() {
    let mut app = TestApp::new();
    let cursor = FakeStatusCursorPort::default();
    *cursor.fails.lock().unwrap() = true;
    app.status_cursor = Box::new(cursor.clone());
    let status = wr::JID::from(super::super::STATUS_BROADCAST_CHAT.to_owned());
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    let mut receipt = message(&status, "status", 42);
    receipt.info.sender = alice.clone();
    app.add_message(receipt);

    app.apply_receipt(wr::ReceiptKind::Read, status, vec!["status".into()]);

    let stored = cursor.stored.lock().unwrap();
    assert_eq!(
        (stored[0].contact.clone(), stored[0].timestamp),
        (alice.clone(), 42)
    );
    assert!(!app.status_last_seen.contains_key(&alice));
}
