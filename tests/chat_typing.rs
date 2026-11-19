use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use whatsrust::{Event, JID};
use wp_tui::{
    app::{App, events::MediaRenderPlan, read_receipts::VisibilityPlan},
    ui::message_list::render_messages_with_plan,
};

mod common;
use common::TestApp;

fn rows(app: &mut App) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(64, 12)).unwrap();
    terminal
        .draw(|frame| {
            render_messages_with_plan(
                frame,
                app,
                &mut MediaRenderPlan::default(),
                &mut VisibilityPlan::default(),
                Rect::new(0, 0, 64, 12),
            );
        })
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(64)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

#[test]
fn incoming_typing_is_visible_only_in_its_conversation_and_tracks_each_sender() {
    let direct = JID::from("123@s.whatsapp.net".to_owned());
    let group = JID::from("456@g.us".to_owned());
    let bob = JID::from("789@s.whatsapp.net".to_owned());
    let mut app = TestApp::new();
    app.contacts.insert(direct.clone(), "Alice".into());
    app.contacts.insert(bob.clone(), "Bob".into());
    app.contacts.insert(group.clone(), "Group".into());
    app.open_chat = Some(direct.clone());
    let event = |chat: &JID, sender: &JID, composing| Event::ChatTyping {
        chat: chat.clone(),
        sender: sender.clone(),
        composing,
    };
    let has_label = |app: &mut App, text: &str| rows(app).iter().any(|r| r.contains(text));

    assert!(app.handle_whatsapp_event(event(&direct, &direct, true)));
    assert!(has_label(&mut app, "Alice is typing…"));
    assert!(!app.handle_whatsapp_event(event(&group, &direct, true)));
    assert!(!app.handle_whatsapp_event(event(&group, &bob, true)));
    assert!(!has_label(&mut app, "Bob"));

    app.open_chat = Some(group.clone());
    assert!(has_label(&mut app, "Alice, Bob are typing…"));
    assert!(!app.handle_whatsapp_event(event(&group, &bob, true)));
    assert!(app.handle_whatsapp_event(event(&group, &direct, false)));
    assert!(has_label(&mut app, "Bob is typing…"));
    assert!(app.handle_whatsapp_event(event(&group, &bob, false)));
    assert!(!has_label(&mut app, "typing"));
    assert!(
        app.chat_messages
            .values()
            .all(|messages| messages.is_empty())
    );
}
