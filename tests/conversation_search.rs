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

fn message(chat: &whatsrust::JID, id: &str, timestamp: i64, body: &str) -> whatsrust::Message {
    whatsrust::Message {
        info: whatsrust::MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: chat.clone(),
            mentions_self: false,
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: whatsrust::MessageContent::Text(body.into()),
    }
}

fn selected_id(app: &TestApp) -> Option<String> {
    app.message_list_state
        .get_selected_message()
        .map(|id| id.to_string())
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

#[test]
fn matches_are_scoped_to_the_open_chat_and_navigated_without_changing_jk() {
    let mut app = TestApp::new();
    let alice: whatsrust::JID = "alice@s.whatsapp.net".to_owned().into();
    let bob: whatsrust::JID = "bob@s.whatsapp.net".to_owned().into();
    app.open_chat_by_jid(alice.clone());
    for (id, timestamp, body) in [
        ("old", 1, "needle old"),
        ("middle", 2, "ordinary message"),
        ("new", 3, "needle new"),
    ] {
        app.add_message(message(&alice, id, timestamp, body));
    }
    app.add_message(message(&bob, "other-chat", 4, "needle in Bob"));
    app.focus_pane = FocusPane::Conversation;
    screen(&mut app);

    press(&mut app, KeyCode::Char('/'));
    for character in "needle".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('n'));
    screen(&mut app);
    assert_eq!(selected_id(&app).as_deref(), Some("old"));
    press(&mut app, KeyCode::Char('N'));
    screen(&mut app);
    assert_eq!(selected_id(&app).as_deref(), Some("new"));

    press(&mut app, KeyCode::Char('k'));
    screen(&mut app);
    assert_eq!(selected_id(&app).as_deref(), Some("middle"));
    press(&mut app, KeyCode::Char('j'));
    screen(&mut app);
    assert_eq!(selected_id(&app).as_deref(), Some("new"));

    app.open_chat_by_jid(bob);
    app.focus_pane = FocusPane::Conversation;
    let before = selected_id(&app);
    press(&mut app, KeyCode::Char('n'));
    screen(&mut app);
    assert_eq!(selected_id(&app), before);
    assert!(!screen(&mut app).contains("/needle"));
}

#[test]
fn jumping_to_a_distant_match_brings_it_into_the_message_viewport() {
    let mut app = TestApp::new();
    let chat: whatsrust::JID = "distant@s.whatsapp.net".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    for index in 0..30 {
        let body = if index == 2 {
            "DISTANT-NEEDLE"
        } else {
            "ordinary filler message"
        };
        app.add_message(message(&chat, &format!("msg-{index}"), index, body));
    }
    app.focus_pane = FocusPane::Conversation;
    assert!(!screen(&mut app).contains("DISTANT-NEEDLE"));
    press(&mut app, KeyCode::Char('/'));
    for character in "DISTANT-NEEDLE".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('n'));
    assert!(screen(&mut app).contains("DISTANT-NEEDLE"));
    assert_eq!(selected_id(&app).as_deref(), Some("msg-2"));
}
