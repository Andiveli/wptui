use super::*;

fn app() -> (App<'static>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let app = App::with_data_dir(dir.path(), dir.path());
    app.db_handler.init();
    (app, dir)
}

fn message(chat: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: chat.clone(),
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
fn contact_name_falls_back_to_the_jid() {
    let (app, _dir) = app();
    let jid = wr::JID::from("alice@example.test".to_owned());

    assert_eq!(app.contact_name(&jid).as_ref(), "alice@example.test");
}

#[test]
fn adding_a_message_registers_chat_and_indexes_message() {
    let (mut app, _dir) = app();
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
    let (mut app, _dir) = app();
    let chat = wr::JID::from("chat@example.test".to_owned());
    app.add_message(message(&chat, "message", 10));
    app.add_message(message(&chat, "message", 20));

    assert_eq!(
        app.messages[&wr::MessageId::from("message")].info.timestamp,
        20
    );
    assert_eq!(app.chat_messages[&chat].len(), 1);
}
