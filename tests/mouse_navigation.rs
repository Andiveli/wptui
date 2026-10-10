use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{Terminal, backend::TestBackend};
use std::collections::HashMap;
use std::fs;
use whatsrust::{JID, Message, MessageContent, MessageInfo};
use wp_tui::app::SharePicker;
use wp_tui::app::actions::{FocusPane, Section};
use wp_tui::app::events::MediaRenderPlan;
use wp_tui::app::read_receipts::VisibilityPlan;
use wp_tui::file_picker::FilePickerState;
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
    let mut app = TestApp::with_settings("mouse=enable\n");
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::ChatList;
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
}

#[test]
fn mouse_disabled_by_default_ignores_injected_wheel_events() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    let broadcast: JID = "status@broadcast".to_owned().into();
    for (index, name) in ["alice", "bob"].into_iter().enumerate() {
        let sender: JID = format!("{name}@s.whatsapp.net").into();
        let mut status = message(&broadcast, name, (index + 1) as i64, name);
        status.info.sender = sender;
        app.add_message(status);
    }
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    draw(&mut app, &mut terminal);
    assert_eq!(app.status_selection.selected(), Some(0));

    app.on_terminal_event(mouse(MouseEventKind::ScrollDown, 20, 2));
    assert_eq!(app.status_selection.selected(), Some(0));
}

#[test]
fn wheel_over_messages_scrolls_rendered_rows_without_changing_selection() {
    let mut app = TestApp::with_settings("mouse=enable\n");
    let chat: JID = "wheel@example.test".to_owned().into();
    app.open_chat_by_jid(chat.clone());
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
    let mut app = TestApp::with_settings("mouse=enable\n");
    let chat: JID = "click@example.test".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    app.focus_pane = FocusPane::Conversation;
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

#[test]
fn wrapped_urls_do_not_hide_the_wheel_selected_link() {
    let mut app = TestApp::with_settings("mouse=enable\n");
    app.url_picker = Some((
        (0..18)
            .map(|index| format!("https://example.test/{index}/{}", "segment/".repeat(12)))
            .collect(),
        0,
    ));
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    draw(&mut app, &mut terminal);
    for _ in 0..15 {
        app.on_terminal_event(mouse(MouseEventKind::ScrollDown, 25, 4));
    }
    assert_eq!(app.url_picker.as_ref().unwrap().1, 15);
    draw(&mut app, &mut terminal);
    let buffer = terminal.backend().buffer();
    let selected_is_painted = (0..20).any(|y| {
        (0..100)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
            .contains("> https://example.test/15/")
    });
    assert!(
        selected_is_painted,
        "wrapped URLs above the selection must not hide its marker"
    );
}

#[test]
fn long_url_picker_keeps_wheel_selected_link_painted() {
    let mut app = TestApp::with_settings("mouse=enable\n");
    app.url_picker = Some((
        (0..18)
            .map(|index| format!("https://example.test/{index}"))
            .collect(),
        0,
    ));
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    draw(&mut app, &mut terminal);
    let rendered_rows = |terminal: &Terminal<TestBackend>| {
        let buffer = terminal.backend().buffer();
        (0..20)
            .map(|y| {
                (0..100)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
    };
    assert!(
        rendered_rows(&terminal)
            .iter()
            .any(|row| row.contains("> https://example.test/0"))
    );
    for _ in 0..15 {
        app.on_terminal_event(mouse(MouseEventKind::ScrollDown, 25, 4));
    }
    assert_eq!(app.url_picker.as_ref().unwrap().1, 15);
    draw(&mut app, &mut terminal);
    assert!(
        rendered_rows(&terminal)
            .iter()
            .any(|row| row.contains("> https://example.test/15")),
        "wheel-selected URL must remain visible, not scroll off the modal"
    );

    // Existing arrow navigation still moves by one and paints its selection.
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)));
    assert_eq!(app.url_picker.as_ref().unwrap().1, 14);
    draw(&mut app, &mut terminal);
    assert!(
        rendered_rows(&terminal)
            .iter()
            .any(|row| row.contains("> https://example.test/14"))
    );
}

#[test]
fn wheel_in_rendered_picker_lists_moves_only_the_active_picker() {
    let directory = tempfile::tempdir().unwrap();
    for index in 0..18 {
        fs::write(
            directory.path().join(format!("file-{index:02}.txt")),
            "fixture",
        )
        .unwrap();
    }

    for picker in ["share", "url", "file"] {
        let mut app = TestApp::with_settings("mouse=enable\n");
        app.selected_section = Section::Status;
        let broadcast: JID = "status@broadcast".to_owned().into();
        for (index, name) in ["alice", "bob"].into_iter().enumerate() {
            let sender: JID = format!("{name}@s.whatsapp.net").into();
            app.contacts.insert(sender.clone(), name.into());
            let mut status = message(&broadcast, name, (index + 1) as i64, name);
            status.info.sender = sender;
            app.add_message(status);
        }
        let needle = match picker {
            "share" => {
                let contacts = (0..18)
                    .map(|index| format!("recipient-{index:02}@example.test").into())
                    .collect();
                app.share_picker = Some(SharePicker::new(contacts, HashMap::new(), HashMap::new()));
                "recipient-00"
            }
            "url" => {
                app.url_picker = Some((
                    (0..18)
                        .map(|index| format!("https://example.test/{index}"))
                        .collect(),
                    0,
                ));
                "https://example.test/0"
            }
            "file" => {
                app.file_picker = Some(FilePickerState::open(directory.path()).unwrap());
                "file-00.txt"
            }
            _ => unreachable!(),
        };
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        draw(&mut app, &mut terminal);
        let buffer = terminal.backend().buffer();
        let (column, row) = (0..20)
            .flat_map(|y| (0..100).map(move |x| (x, y)))
            .find(|&(x, y)| {
                x + needle.len() <= 100
                    && (0..needle.len())
                        .map(|dx| buffer[(x as u16 + dx as u16, y as u16)].symbol())
                        .collect::<String>()
                        == needle
            })
            .unwrap_or_else(|| panic!("{picker} list must be painted"));
        assert_eq!(app.status_selection.selected(), Some(0));

        app.on_terminal_event(mouse(MouseEventKind::ScrollDown, column as u16, row as u16));
        let selected = match picker {
            "share" => app.share_picker.as_ref().unwrap().selected,
            "url" => app.url_picker.as_ref().unwrap().1,
            "file" => app.file_picker.as_ref().unwrap().selected,
            _ => unreachable!(),
        };
        assert_eq!(selected, 1, "wheel must move the {picker} cursor");
        assert_eq!(
            app.status_selection.selected(),
            Some(0),
            "base pane must not move"
        );

        draw(&mut app, &mut terminal);
        app.on_terminal_event(mouse(MouseEventKind::ScrollDown, 14, row as u16));
        app.on_terminal_event(mouse(MouseEventKind::ScrollDown, 10, row as u16));
        let selected = match picker {
            "share" => app.share_picker.as_ref().unwrap().selected,
            "url" => app.url_picker.as_ref().unwrap().1,
            "file" => app.file_picker.as_ref().unwrap().selected,
            _ => unreachable!(),
        };
        assert_eq!(selected, 1, "border/outside must not scroll the {picker}");
        assert_eq!(
            app.status_selection.selected(),
            Some(0),
            "modal must shield the base pane"
        );
    }
}
