use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, style::Color};
use wp_tui::app::actions::{ConversationMode, FocusPane, Section};
use wp_tui::app::events::MediaRenderPlan;
use wp_tui::app::read_receipts::VisibilityPlan;
use wp_tui::ui::render_chats_with_plan;

mod common;
use common::TestApp;

fn press(app: &mut TestApp, key: KeyCode) {
    let modifiers = match key {
        KeyCode::Char(character) if character.is_ascii_uppercase() => KeyModifiers::SHIFT,
        _ => KeyModifiers::NONE,
    };
    app.on_terminal_event(Event::Key(KeyEvent::new(key, modifiers)));
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

fn buffer(app: &mut TestApp) -> Buffer {
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
    terminal.backend().buffer().clone()
}

fn screen(app: &mut TestApp) -> String {
    buffer(app)
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

fn painted_column(buffer: &Buffer, row: u16, needle: &str) -> u16 {
    let symbols = needle
        .chars()
        .map(|character| character.to_string())
        .collect::<Vec<_>>();
    assert!(symbols.len() <= buffer.area.width as usize);
    (0..=buffer.area.width - symbols.len() as u16)
        .find(|&x| {
            symbols
                .iter()
                .enumerate()
                .all(|(offset, symbol)| buffer[(x + offset as u16, row)].symbol() == symbol)
        })
        .expect("text must be painted in the requested row")
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
    let painted = buffer(&mut app);
    let row = (0..20)
        .find(|&y| {
            (0..80)
                .map(|x| painted[(x, y)].symbol())
                .collect::<String>()
                .contains("needle in Bob")
        })
        .expect("other chat message must be painted");
    let x = painted_column(&painted, row, "needle in Bob");
    assert_eq!(painted[(x, row)].bg, Color::Reset);
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
    assert_eq!(screen(&mut app).matches("DISTANT-NEEDLE").count(), 2);
    assert_eq!(selected_id(&app).as_deref(), Some("msg-2"));
}

#[test]
fn enter_selects_the_latest_offscreen_match_before_navigating() {
    let mut app = TestApp::new();
    let chat: whatsrust::JID = "recent@s.whatsapp.net".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    for index in 0..30 {
        let body = match index {
            8 => "MARKER older",
            22 => "MARKER newest",
            _ => "ordinary filler message",
        };
        app.add_message(message(&chat, &format!("msg-{index}"), index, body));
    }
    app.focus_pane = FocusPane::Conversation;
    assert!(!screen(&mut app).contains("MARKER newest"));

    press(&mut app, KeyCode::Char('/'));
    for character in "MARKER".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press(&mut app, KeyCode::Enter);
    assert_eq!(selected_id(&app).as_deref(), Some("msg-22"));
    assert!(screen(&mut app).contains("MARKER newest"));

    press(&mut app, KeyCode::Char('n'));
    assert_eq!(selected_id(&app).as_deref(), Some("msg-8"));
    press(&mut app, KeyCode::Char('N'));
    assert_eq!(selected_id(&app).as_deref(), Some("msg-22"));
}

#[test]
fn search_prompt_reports_matches_and_selected_position() {
    let mut app = TestApp::new();
    let chat: whatsrust::JID = "count@s.whatsapp.net".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    app.add_message(message(&chat, "old", 1, "find old"));
    app.add_message(message(&chat, "new", 2, "find new"));
    app.focus_pane = FocusPane::Conversation;

    press(&mut app, KeyCode::Char('/'));
    for character in "find".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app).contains("1/2 coincidencias"));
    press(&mut app, KeyCode::Char('n'));
    assert!(screen(&mut app).contains("2/2 coincidencias"));
}

#[test]
fn search_prompt_reports_no_matches_without_moving_the_selection() {
    let mut app = TestApp::new();
    let chat: whatsrust::JID = "empty@s.whatsapp.net".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    app.add_message(message(&chat, "only", 1, "ordinary text"));
    app.focus_pane = FocusPane::Conversation;
    screen(&mut app);
    let before = selected_id(&app);

    press(&mut app, KeyCode::Char('/'));
    for character in "absent".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press(&mut app, KeyCode::Enter);
    assert!(screen(&mut app).contains("Sin coincidencias"));
    assert_eq!(selected_id(&app), before);
    press(&mut app, KeyCode::Char('n'));
    press(&mut app, KeyCode::Char('N'));
    assert_eq!(selected_id(&app), before);
    press(&mut app, KeyCode::Esc);
    assert!(!screen(&mut app).contains("Sin coincidencias"));
}

#[test]
fn wrapped_rtl_fragment_is_highlighted_in_visual_order() {
    let mut app = TestApp::new();
    let chat: whatsrust::JID = "rtl@s.whatsapp.net".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    app.add_message(message(
        &chat,
        "rtl-text",
        1,
        &format!("{}שלום tail", "prefix ".repeat(13)),
    ));
    app.focus_pane = FocusPane::Conversation;
    press(&mut app, KeyCode::Char('/'));
    for character in "שלום".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press(&mut app, KeyCode::Enter);

    let painted = buffer(&mut app);
    let (y, line) = (0..20)
        .map(|y| {
            (
                y,
                (0..80)
                    .map(|x| painted[(x, y)].symbol())
                    .collect::<String>(),
            )
        })
        .find(|(_, line)| line.contains("םולש"))
        .expect("wrapped RTL match must be painted in the message viewport");
    let x = painted_column(&painted, y, "םולש");
    for offset in 0..4 {
        assert_eq!(painted[(x + offset, y)].bg, Color::Yellow);
    }
    if x > 0 {
        assert_eq!(painted[(x - 1, y)].bg, Color::Reset);
    }
}

#[test]
fn search_highlights_only_matching_text_in_the_open_chat_and_clears_on_escape() {
    let mut app = TestApp::new();
    let chat: whatsrust::JID = "highlight@s.whatsapp.net".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    app.add_message(message(&chat, "text", 1, "before needle after"));
    app.focus_pane = FocusPane::Conversation;

    let plain = buffer(&mut app);
    let row = (0..20)
        .find(|&y| {
            (0..80)
                .map(|x| plain[(x, y)].symbol())
                .collect::<String>()
                .contains("before needle after")
        })
        .expect("message text must be painted");
    let match_x = painted_column(&plain, row, "before needle after") + 7;
    assert_eq!(plain[(match_x, row)].bg, Color::Reset);

    press(&mut app, KeyCode::Char('/'));
    for character in "needle".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.conversation_mode, ConversationMode::MessageNavigation);
    assert!(screen(&mut app).contains("/needle"));
    let highlighted = buffer(&mut app);
    let highlighted_row = (0..20)
        .find(|&y| {
            (0..80)
                .map(|x| highlighted[(x, y)].symbol())
                .collect::<String>()
                .contains("before needle after")
        })
        .expect("matching message must be painted");
    let highlighted_x = painted_column(&highlighted, highlighted_row, "before needle after") + 7;
    for offset in 0.."needle".len() as u16 {
        assert_eq!(
            highlighted[(highlighted_x + offset, highlighted_row)].bg,
            Color::Yellow
        );
    }
    assert_eq!(
        highlighted[(highlighted_x - 1, highlighted_row)].bg,
        Color::Reset
    );
    assert_eq!(
        highlighted[(highlighted_x + "needle".len() as u16, highlighted_row)].bg,
        Color::Reset
    );

    press(&mut app, KeyCode::Esc);
    let cleared = buffer(&mut app);
    assert_eq!(cleared[(match_x, row)].bg, Color::Reset);
}
