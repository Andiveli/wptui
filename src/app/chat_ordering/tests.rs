use super::*;
use crate::app::test_support::TestApp;

fn app() -> TestApp {
    TestApp::new()
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
fn out_of_order_history_is_sorted_oldest_first() {
    let mut app = app();
    let chat = wr::JID::from("chat@example.test".to_owned());
    for item in [("newest", 30), ("oldest", 10), ("middle", 20)] {
        app.add_message(message(&chat, item.0, item.1));
    }
    assert_eq!(
        app.chat_messages[&chat]
            .iter()
            .map(|id| id.as_ref())
            .collect::<Vec<_>>(),
        ["oldest", "middle", "newest"]
    );
    app.add_message(message(&chat, "middle", 40));
    assert_eq!(
        app.chat_messages[&chat].last().map(AsRef::as_ref),
        Some("middle")
    );
}

#[test]
fn equal_timestamps_are_tied_by_message_id() {
    let mut app = app();
    let chat = wr::JID::from("chat@example.test".to_owned());
    for id in ["message-c", "message-a", "message-b"] {
        app.add_message(message(&chat, id, 10));
    }
    assert_eq!(
        app.chat_messages[&chat]
            .iter()
            .map(|id| id.as_ref())
            .collect::<Vec<_>>(),
        ["message-a", "message-b", "message-c"]
    );
}

#[test]
fn new_message_preserves_selected_message_by_id() {
    let mut app = app();
    let chat = wr::JID::from("chat@example.test".to_owned());
    app.open_chat = Some(chat.clone());
    for item in [("oldest", 10), ("middle", 20), ("newest", 30)] {
        app.add_message(message(&chat, item.0, item.1));
    }
    app.message_list_state.select(Some(1));
    app.message_list_state.set_selected_message("middle".into());
    app.add_message(message(&chat, "newest-2", 40));
    assert_eq!(app.message_list_state.selected, Some(2));
    assert_eq!(
        app.message_list_state.get_selected_message(),
        Some("middle".into())
    );
}

#[test]
fn new_message_in_other_chat_leaves_selection_untouched() {
    let mut app = app();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let other = wr::JID::from("other@example.test".to_owned());
    app.open_chat = Some(chat.clone());
    for item in [("oldest", 10), ("middle", 20), ("newest", 30)] {
        app.add_message(message(&chat, item.0, item.1));
    }
    app.message_list_state.select(Some(1));
    app.message_list_state.set_selected_message("middle".into());
    app.message_list_state.selected = Some(1);
    app.add_message(message(&other, "other-msg", 40));
    assert_eq!(app.message_list_state.selected, Some(1));
    assert_eq!(
        app.message_list_state.get_selected_message(),
        Some("middle".into())
    );
}
