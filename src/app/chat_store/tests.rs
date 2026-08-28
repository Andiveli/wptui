use super::*;
use crate::app::test_support::TestApp;

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
