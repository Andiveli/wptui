use super::hydration_port::{ChatStoreHydration, ChatStoreHydrationPort};
use super::*;
use crate::app::{Chat, test_support::TestApp};

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
    assert!(read_state.contains("db_handler.read_cursors"));
    assert!(!hydration.contains("db_handler.read_cursors"));
    assert!(hydration.contains("self.restore_read_cursors()"));
}

#[test]
fn persisted_chat_cursor_restores_into_read_state() {
    let directory = tempfile::tempdir().unwrap();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let message_id = wr::MessageId::from("read-message");
    {
        let app = TestApp::with_database(directory.path());
        app.db_handler
            .set_last_read_cursor(&chat, Some(message_id.clone()), 42);
    }

    let mut app = TestApp::with_database(directory.path());
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
    app.add_message(message(&chat, "latest", 42));

    assert!(app.mark_chat_read_at_latest(&chat));
    assert_eq!(app.mark_chat_read_at_latest(&chat), false);
}

#[test]
fn status_cursor_never_schedules_chat_app_state_sync() {
    let mut app = TestApp::new();
    let status = wr::JID::from(super::super::STATUS_BROADCAST_CHAT.to_owned());
    app.add_message(message(&status, "status", 42));

    assert!(!app.mark_chat_read_at_latest(&status));
}
