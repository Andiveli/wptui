use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use wp_tui::ui::render_chats;

mod common;
use common::TestApp;

#[test]
fn chat_panel_shows_andiveli_logo_when_no_chat_is_open() {
    let mut app = TestApp::new();
    assert!(app.open_chat().is_none());

    let backend = TestBackend::new(60, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| render_chats(frame, &mut app, Rect::new(0, 0, 60, 30)))
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
        .draw(|frame| render_chats(frame, &mut app, Rect::new(0, 0, 60, 30)))
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
