use super::super::actions::{ConversationMode, FocusPane, Section};
use super::super::test_support::TestApp;
use whatsrust as wr;

fn app_with_messages() -> TestApp {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    app.open_chat = Some(chat.clone());
    app.chat_messages.insert(
        chat,
        vec!["newest".into(), "middle".into(), "oldest".into()],
    );
    app.focus_pane = FocusPane::Conversation;
    app.selected_section = Section::Chats;
    app.conversation_mode = ConversationMode::MessageNavigation;
    app
}

#[test]
fn message_navigation_is_bounded_and_requests_visible_selection_update() {
    let mut app = app_with_messages();

    app.select_previous();
    assert_eq!(app.message_list_state.selected, Some(0));
    assert!(app.message_list_state.update_selected);

    app.message_list_state.update_selected = false;
    app.jump_top();
    assert_eq!(app.message_list_state.selected, Some(2));
    assert!(app.message_list_state.update_selected);

    app.jump_bottom();
    assert_eq!(app.message_list_state.selected, Some(0));
}

#[test]
fn empty_message_navigation_clears_selection_and_offset() {
    let mut app = app_with_messages();
    app.chat_messages.values_mut().next().unwrap().clear();
    app.message_list_state.selected = Some(2);
    app.message_list_state.offset = 9;
    app.message_list_state.set_selected_message("middle".into());

    app.select_next();

    assert_eq!(app.message_list_state.selected, None);
    assert_eq!(app.message_list_state.offset, 0);
    assert_eq!(app.message_list_state.get_selected_message(), None);
}

#[test]
fn navigation_outside_conversation_does_not_move_message_selection() {
    let mut app = app_with_messages();
    app.message_list_state.select(Some(1));
    app.focus_pane = FocusPane::SectionRail;

    app.select_next();

    assert_eq!(app.message_list_state.selected, Some(1));
}
