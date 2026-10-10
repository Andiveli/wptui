use ratatui::crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend};
use whatsrust::{JID, Message, MessageContent, MessageInfo};
use wp_tui::app::actions::{FocusPane, Section};
use wp_tui::app::events::MediaRenderPlan;
use wp_tui::app::read_receipts::VisibilityPlan;
use wp_tui::ui;

mod common;
use common::TestApp;

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn message(chat: &JID, id: &str, timestamp: i64, text: &str) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: "sender@example.test".to_owned().into(),
            mentions_self: false,
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::Text(text.into()),
    }
}

fn draw(app: &mut TestApp, terminal: &mut Terminal<TestBackend>) {
    let mut media = MediaRenderPlan::default();
    let mut visibility = VisibilityPlan::default();
    terminal
        .draw(|frame| ui::draw_with_plan(frame, app, &mut media, &mut visibility))
        .unwrap();
}

#[test]
fn wheel_over_status_contacts_moves_only_that_visible_list() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::ChatList;
    app.mouse_capture_enabled = true;
    let broadcast: JID = "status@broadcast".to_owned().into();
    for (index, name) in ["alice", "bob", "carol"].into_iter().enumerate() {
        let sender: JID = format!("{name}@s.whatsapp.net").into();
        app.contacts.insert(sender.clone(), name.into());
        let mut status = message(&broadcast, name, (index + 1) as i64, name);
        status.info.sender = sender;
        app.add_message(status);
    }
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    draw(&mut app, &mut terminal);
    assert_eq!(app.status_selection.selected(), Some(0));

    app.on_terminal_event(mouse(MouseEventKind::ScrollDown, 20, 2));

    assert_eq!(app.status_selection.selected(), Some(1));
    assert_eq!(app.focus_pane, FocusPane::ChatList);
    assert_eq!(app.selected_section, Section::Status);

    app.mouse_capture_enabled = false;
    app.on_terminal_event(mouse(MouseEventKind::ScrollDown, 20, 2));
    assert_eq!(app.status_selection.selected(), Some(1));
}

#[test]
fn wheel_over_messages_scrolls_rendered_rows_without_changing_selection() {
    let mut app = TestApp::new();
    let chat: JID = "wheel@example.test".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    app.mouse_capture_enabled = true;
    app.focus_pane = FocusPane::Conversation;
    for index in 0..12 {
        app.add_message(message(
            &chat,
            &format!("item-{index}"),
            index,
            &format!("MESSAGE-{index}"),
        ));
    }
    app.message_list_state.select(Some(0));
    let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    draw(&mut app, &mut terminal);
    let selected = app.message_list_state.get_selected_message();

    app.on_terminal_event(mouse(MouseEventKind::ScrollUp, 60, 3));
    assert_eq!(app.message_list_state.offset, 3);
    draw(&mut app, &mut terminal);
    assert_eq!(app.message_list_state.get_selected_message(), selected);

    app.on_terminal_event(mouse(MouseEventKind::ScrollDown, 60, 3));
    assert_eq!(app.message_list_state.offset, 0);
    assert_eq!(app.message_list_state.get_selected_message(), selected);

    for _ in 0..20 {
        app.on_terminal_event(mouse(MouseEventKind::ScrollUp, 60, 3));
        draw(&mut app, &mut terminal);
    }
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(
        rendered.contains("MESSAGE-0"),
        "wheel overscrolled beyond oldest message"
    );
}

#[test]
fn clicking_a_rendered_variable_height_message_selects_that_exact_row() {
    let mut app = TestApp::new();
    let chat: JID = "click@example.test".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    app.focus_pane = FocusPane::Conversation;
    app.mouse_capture_enabled = true;
    for (id, time, text) in [
        ("old", 1, "OLDER-MESSAGE"),
        ("middle", 2, "MIDDLE-CLICK-TARGET"),
        (
            "newest",
            3,
            "NEWEST-LONG-MESSAGE repeated words make this selected message wrap into several lines repeated words",
        ),
    ] {
        app.add_message(message(&chat, id, time, text));
    }
    app.message_list_state.select(Some(0));
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
    draw(&mut app, &mut terminal);
    let buffer = terminal.backend().buffer();
    let find_rendered = |needle: &str| {
        (0..20)
            .flat_map(|y| (0..74).map(move |x| (x, y)))
            .find(|&(x, y)| {
                (0..needle.len())
                    .map(|dx| buffer[(x + dx as u16, y)].symbol())
                    .collect::<String>()
                    == needle
            })
            .expect("message must actually be rendered in the test viewport")
    };
    let target = find_rendered("MIDDLE");
    let other_target = find_rendered("NEWEST");

    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        target.0,
        target.1,
    ));
    draw(&mut app, &mut terminal);

    assert_eq!(app.message_list_state.selected, Some(1));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("middle")
    );

    app.shortcut_popup = true;
    draw(&mut app, &mut terminal);
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        other_target.0,
        other_target.1,
    ));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("middle")
    );
}
