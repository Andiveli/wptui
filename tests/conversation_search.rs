use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use wp_tui::app::actions::{ConversationMode, FocusPane, Section};
use wp_tui::app::events::MediaRenderPlan;
use wp_tui::app::read_receipts::VisibilityPlan;
use wp_tui::ui::render_chats_with_plan;

mod common;
use common::TestApp;

fn press(app: &mut TestApp, key: KeyCode) {
    app.on_terminal_event(Event::Key(KeyEvent::new(key, KeyModifiers::NONE)));
}

fn screen(app: &mut TestApp) -> String {
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
    terminal
        .draw(|frame| {
            render_chats_with_plan(
                frame,
                app,
                &mut MediaRenderPlan::default(),
                &mut VisibilityPlan::default(),
                Rect::new(0, 0, 80, 20),
            );
        })
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn slash_opens_message_search_only_from_open_chat_history() {
    let mut app = TestApp::new();
    app.open_chat_by_jid(whatsrust::JID("alice@s.whatsapp.net".into()));
    app.focus_pane = FocusPane::Conversation;
    app.conversation_mode = ConversationMode::MessageNavigation;

    press(&mut app, KeyCode::Char('/'));
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('k'));
    assert!(screen(&mut app).contains("/jk"));
    assert_eq!(app.focus_pane, FocusPane::Conversation);
    assert_eq!(app.conversation_mode, ConversationMode::MessageNavigation);
    assert!(!app.contact_search_active);
    assert_eq!(app.composer.text(), "");

    press(&mut app, KeyCode::Esc);
    assert!(!screen(&mut app).contains("/jk"));
}

#[test]
fn slash_still_searches_contacts_in_the_chats_list() {
    let mut app = TestApp::new();
    app.focus_pane = FocusPane::ChatList;
    app.selected_section = Section::Chats;

    press(&mut app, KeyCode::Char('/'));
    press(&mut app, KeyCode::Char('a'));
    assert!(app.contact_search_active);
    assert_eq!(app.contact_search.input.to_string(), "a");
}

#[test]
fn slash_remains_text_in_the_composer() {
    let mut app = TestApp::new();
    app.open_chat_by_jid(whatsrust::JID("alice@s.whatsapp.net".into()));
    app.focus_pane = FocusPane::Conversation;
    app.conversation_mode = ConversationMode::ComposerEditing;

    press(&mut app, KeyCode::Char('/'));
    assert_eq!(app.composer.text(), "/");
    assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
    assert!(!app.contact_search_active);
}
