use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use whatsrust::{JID, Message, MessageContent, MessageInfo};
use wp_tui::{app::events::MediaRenderPlan, ui::render_chats_with_plan};

mod common;
use common::TestApp;

#[test]
fn chat_panel_shows_andiveli_logo_when_no_chat_is_open() {
    let mut app = TestApp::new();
    assert!(app.open_chat().is_none());

    let backend = TestBackend::new(60, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            render_chats_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                Rect::new(0, 0, 60, 30),
            )
        })
        .expect("chat panel should render");

    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..30)
        .map(|y| (0..60).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect();

    assert!(
        rows.iter().any(|row| row.contains("Andiveli")),
        "expected the Andiveli label in the empty chat panel"
    );
    assert!(
        rows.iter().any(|row| row.contains('⣿')),
        "expected the logo glyphs in the empty chat panel"
    );
}

#[test]
fn logo_hidden_once_a_chat_is_opened() {
    let mut app = TestApp::new();
    let chat = whatsrust::JID::from("chat@example.test".to_owned());
    app.contacts.insert(chat.clone(), "Alice".into());
    app.sorted_chats.push(chat.clone());
    app.open_chat = Some(chat);

    let backend = TestBackend::new(60, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            render_chats_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                Rect::new(0, 0, 60, 30),
            )
        })
        .expect("chat panel should render");

    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..30)
        .map(|y| (0..60).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect();

    assert!(
        !rows.iter().any(|row| row.contains("Andiveli")),
        "logo label must not show when a chat is open"
    );
    assert!(
        !rows.iter().any(|row| row.contains('⣿')),
        "logo glyphs must not show when a chat is open"
    );
}

#[test]
fn opened_chat_with_no_unread_messages_renders_without_panicking() {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    let message = Message {
        info: MessageInfo {
            id: "message-1".into(),
            chat: chat.clone(),
            sender: chat.clone(),
            mentions_self: false,
            timestamp: 1,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::Text("hello".into()),
    };
    app.contacts.insert(chat.clone(), "Alice".into());
    app.sorted_chats.push(chat.clone());
    app.open_chat = Some(chat.clone());
    app.messages.insert(message.info.id.clone(), message);
    app.chat_messages
        .insert(chat.clone(), vec!["message-1".into()]);
    assert_eq!(app.unread_boundary(&chat), None);

    let backend = TestBackend::new(60, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            render_chats_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                Rect::new(0, 0, 60, 30),
            )
        })
        .expect("opened chat with no unread messages should render");

    let buffer = terminal.backend().buffer();
    let output: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
    assert!(
        output.contains("hello"),
        "opened chat message must render: {output:?}"
    );
}
