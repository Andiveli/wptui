use super::super::Chat;
use crate::app::actions::FocusPane;
use crate::app::test_support::TestApp;
use crate::key_handler::Key;
use ratatui::crossterm::event::KeyCode;
use whatsrust as wr;

fn jid(value: &str) -> wr::JID {
    wr::JID::from(value.to_owned())
}

#[test]
fn select_chat_reanchors_a_row_without_changing_open_chat_or_message_selection() {
    let mut app = TestApp::new();
    let first = jid("first@example.test");
    let second = jid("second@example.test");
    app.chats.insert(
        first.clone(),
        Chat {
            jid: first.clone(),
            last_message_time: Some(2),
        },
    );
    app.chats.insert(
        second.clone(),
        Chat {
            jid: second.clone(),
            last_message_time: Some(1),
        },
    );
    app.sorted_chats = vec![first, second.clone()];
    app.open_chat = Some(jid("open@example.test"));
    app.message_list_state.selected = Some(3);

    app.select_chat(Some(second.clone()));

    assert_eq!(app.get_selected_chat(), Some(second));
    assert_eq!(app.open_chat(), Some(jid("open@example.test")));
    assert_eq!(app.message_list_state.selected, Some(3));
}

#[test]
fn chat_list_movement_keeps_open_conversation_and_message_selection() {
    let mut app = TestApp::new();
    let first = jid("first@example.test");
    let second = jid("second@example.test");
    app.chats.insert(
        first.clone(),
        Chat {
            jid: first.clone(),
            last_message_time: Some(2),
        },
    );
    app.chats.insert(
        second.clone(),
        Chat {
            jid: second.clone(),
            last_message_time: Some(1),
        },
    );
    app.sorted_chats = vec![first.clone(), second.clone()];
    app.chat_list_state.select(Some(0));
    app.open_chat = Some(first.clone());
    app.message_list_state.selected = Some(2);
    app.focus_pane = FocusPane::ChatList;

    app.move_selection_next();

    assert_eq!(app.get_selected_chat(), Some(second));
    assert_eq!(app.open_chat(), Some(first));
    assert_eq!(app.message_list_state.selected, Some(2));
}

#[test]
fn leaving_search_restores_the_pre_search_chat_selection() {
    let mut app = TestApp::new();
    let first = jid("alice@example.test");
    let second = jid("bob@example.test");
    app.chats.insert(
        first.clone(),
        Chat {
            jid: first.clone(),
            last_message_time: Some(2),
        },
    );
    app.chats.insert(
        second.clone(),
        Chat {
            jid: second.clone(),
            last_message_time: Some(1),
        },
    );
    app.sorted_chats = vec![first, second.clone()];
    app.chat_list_state.select(Some(1));
    app.contact_search_active = true;

    app.handle_chat_search_key(Key::k(KeyCode::Esc));

    assert_eq!(app.get_selected_chat(), Some(second));
    assert!(!app.contact_search_active);
}
