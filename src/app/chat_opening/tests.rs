use super::App;
use crate::app::read_receipts::VisibilityPlan;
use crate::app::test_support::TestApp;
use ratatui::{Terminal, backend::TestBackend};
use whatsrust as wr;

#[test]
fn is_group_chat_detects_group_jids() {
    assert!(App::is_group_chat(&wr::JID::from("123@g.us".to_owned())));
    assert!(!App::is_group_chat(&wr::JID::from(
        "123@s.whatsapp.net".to_owned()
    )));
    assert!(!App::is_group_chat(&wr::JID::from(
        "status@broadcast".to_owned()
    )));
}

#[test]
fn open_chat_by_jid_registers_recipient_in_chat_list() {
    let mut app = TestApp::new();
    let recipient = wr::JID::from("alice@s.whatsapp.net".to_owned());

    app.open_chat_by_jid(recipient.clone());

    assert_eq!(app.open_chat(), Some(recipient.clone()));
    assert!(app.chats.contains_key(&recipient));
    assert!(app.sorted_chats.contains(&recipient));
}

#[test]
fn opening_chat_with_restored_cursor_renders_unread_messages() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("alice@s.whatsapp.net".to_owned());

    app.add_message(crate::app::test_support::message(&chat, "read", 1));
    app.mark_chat_read_at_latest(&chat);
    app.add_message(crate::app::test_support::message(&chat, "unread", 2));
    app.open_chat_by_jid(chat);

    let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();
    let mut media_render_plan = crate::app::events::MediaRenderPlan::default();
    let mut visibility_plan = VisibilityPlan::default();
    terminal
        .draw(|frame| {
            crate::ui::draw_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                &mut visibility_plan,
            );
        })
        .unwrap();
    assert_eq!(
        app.message_list_state.get_selected_message(),
        Some("read".into())
    );
}
