use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{Terminal, backend::TestBackend};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::rc::Rc;
use whatsrust::{FileContent, FileKind, JID, Message, MessageContent, MessageInfo};
use wp_tui::app::SharePicker;
use wp_tui::app::actions::{ActionNotice, AppAction, FocusPane, Section, UrlOpener};
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

#[derive(Clone, Default)]
struct RecordingUrlOpener(Rc<RefCell<Vec<wp_tui::url::UrlLaunchPlan>>>);

impl UrlOpener for RecordingUrlOpener {
    fn open(&mut self, plan: &wp_tui::url::UrlLaunchPlan) -> io::Result<()> {
        self.0.borrow_mut().push(plan.clone());
        Ok(())
    }
}

#[test]
fn clicking_only_a_painted_url_opens_the_clicked_message_not_the_previous_selection() {
    let mut app = TestApp::with_settings("mouse=enable\n");
    let chat: JID = "click-link@example.test".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    app.focus_pane = FocusPane::Conversation;
    app.add_message(message(
        &chat,
        "older",
        1,
        "https://wrong.example.test/path",
    ));
    app.add_message(message(
        &chat,
        "clicked",
        2,
        "plain https://clicked.example.test/path",
    ));
    app.message_list_state.set_selected_message("older".into());
    let opener = RecordingUrlOpener::default();
    let plans = opener.0.clone();
    app.url_opener = Box::new(opener);
    let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
    draw(&mut app, &mut terminal);

    let conversation =
        ui::navigation_areas(terminal.backend().buffer().area, app.pane_visibility).conversation;
    let painted = |needle: &str, terminal: &Terminal<TestBackend>| {
        let buffer = terminal.backend().buffer();
        (conversation.top()..conversation.bottom())
            .flat_map(|y| {
                (conversation.left()..conversation.right().saturating_sub(needle.len() as u16))
                    .map(move |x| (x, y))
            })
            .find(|&(x, y)| {
                (0..needle.len())
                    .map(|dx| buffer[(x + dx as u16, y)].symbol())
                    .collect::<String>()
                    == needle
            })
            .expect("action target must be painted in the conversation pane")
    };
    let plain = painted("plain", &terminal);
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        plain.0,
        plain.1,
    ));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("clicked")
    );
    assert!(
        plans.borrow().is_empty(),
        "non-actionable text must not open"
    );

    app.message_list_state.set_selected_message("older".into());
    draw(&mut app, &mut terminal);
    let link = painted("https://clicked.example.test/path", &terminal);
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        link.0 + 8,
        link.1,
    ));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("clicked")
    );
    assert_eq!(
        plans.borrow().as_slice(),
        &[wp_tui::url::url_launch_plan(
            "https://clicked.example.test/path"
        )],
        "clicking the painted URL must open that URL, never the older selected URL"
    );
}

#[test]
fn clicking_painted_attachment_and_caption_uses_the_clicked_file_region() {
    let mut app = TestApp::with_settings("mouse=enable\n");
    let chat: JID = "click-file@example.test".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    app.focus_pane = FocusPane::Conversation;
    let mut image = message(&chat, "image", 1, "");
    image.message = MessageContent::File(FileContent {
        kind: FileKind::Image,
        path: "photo-click-target.png".into(),
        ..Default::default()
    });
    let mut document = message(&chat, "document", 2, "");
    document.message = MessageContent::File(FileContent {
        kind: FileKind::Document,
        path: "report-click-target.pdf".into(),
        caption: Some("https://caption.example.test/open".into()),
        ..Default::default()
    });
    let mut audio = message(&chat, "audio", 3, "");
    audio.message = MessageContent::File(FileContent {
        kind: FileKind::Audio,
        path: "audio-click-target.ogg".into(),
        ..Default::default()
    });
    app.add_message(image);
    app.add_message(document);
    app.add_message(audio);
    app.message_list_state
        .set_selected_message("document".into());
    let opener = RecordingUrlOpener::default();
    let plans = opener.0.clone();
    app.url_opener = Box::new(opener);
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    let conversation =
        ui::navigation_areas(terminal.backend().buffer().area, app.pane_visibility).conversation;
    let painted = |needle: &str, terminal: &Terminal<TestBackend>| {
        let buffer = terminal.backend().buffer();
        (conversation.top()..conversation.bottom())
            .flat_map(|y| {
                (conversation.left()..conversation.right().saturating_sub(needle.len() as u16))
                    .map(move |x| (x, y))
            })
            .find(|&(x, y)| {
                (0..needle.len())
                    .map(|dx| buffer[(x + dx as u16, y)].symbol())
                    .collect::<String>()
                    == needle
            })
            .expect("file or caption must be painted in the conversation pane")
    };
    draw(&mut app, &mut terminal);
    let image_path = painted("photo-click-target.png", &terminal);
    let image_gap = image_path.0 - 1;
    assert_eq!(
        terminal.backend().buffer()[(image_gap, image_path.1)].symbol(),
        " "
    );
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        image_gap,
        image_path.1,
    ));
    assert!(
        app.attachment_viewer.is_none(),
        "media-path gap must not open"
    );
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("image")
    );
    app.message_list_state
        .set_selected_message("document".into());
    draw(&mut app, &mut terminal);
    let image_path = painted("photo-click-target.png", &terminal);
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        conversation.right() - 2,
        image_path.1,
    ));
    assert!(
        app.attachment_viewer.is_none(),
        "blank row space must only select"
    );
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("image")
    );
    assert!(plans.borrow().is_empty());
    app.message_list_state
        .set_selected_message("document".into());
    draw(&mut app, &mut terminal);
    let image_path = painted("photo-click-target.png", &terminal);
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        image_path.0 + 6,
        image_path.1,
    ));
    assert_eq!(
        app.attachment_viewer
            .as_ref()
            .map(|viewer| viewer.message_id.as_ref()),
        Some("image"),
        "clicking an image path should open the clicked image, not the selected document"
    );
    assert!(plans.borrow().is_empty());

    app.dispatch_action(AppAction::CloseAttachmentViewer);
    draw(&mut app, &mut terminal);
    let document_path = painted("report-click-target.pdf", &terminal);
    let document_gap = document_path.0 - 1;
    assert_eq!(
        terminal.backend().buffer()[(document_gap, document_path.1)].symbol(),
        " "
    );
    assert!(
        app.action_notice.is_none(),
        "document gap starts without a notice"
    );
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        document_gap,
        document_path.1,
    ));
    assert!(
        app.action_notice.is_none(),
        "document-path gap must not open"
    );
    assert!(plans.borrow().is_empty());
    draw(&mut app, &mut terminal);
    let document_path = painted("report-click-target.pdf", &terminal);
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        document_path.0 + 6,
        document_path.1,
    ));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("document")
    );
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable("Media is not downloaded".into())),
        "document path should follow document open, not its caption URL"
    );
    assert!(plans.borrow().is_empty());

    draw(&mut app, &mut terminal);
    let audio_path = painted("audio-click-target.ogg", &terminal);
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        audio_path.0 + 6,
        audio_path.1,
    ));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("audio")
    );
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable("Media is not downloaded".into())),
        "audio path must use the existing media launch guard"
    );
    assert!(plans.borrow().is_empty());

    draw(&mut app, &mut terminal);
    let caption = painted("https://caption.example.test/open", &terminal);
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        caption.0 + 8,
        caption.1,
    ));
    assert_eq!(
        plans.borrow().as_slice(),
        &[wp_tui::url::url_launch_plan(
            "https://caption.example.test/open"
        )],
        "the caption URL should open only when its painted cells are clicked"
    );
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
fn default_mouse_capture_routes_wheel_and_explicit_disable_ignores_it() {
    for (settings, expected) in [(None, Some(1)), (Some("mouse=disable\n"), Some(0))] {
        let mut app = match settings {
            Some(settings) => TestApp::with_settings(settings),
            None => TestApp::new(),
        };
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
        assert_eq!(app.status_selection.selected(), expected);
    }
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
fn wrapped_share_and_file_entries_keep_the_wheel_cursor_painted() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory
            .path()
            .join(format!("a-file-{}", "longpart".repeat(29))),
        "fixture",
    )
    .unwrap();
    fs::write(directory.path().join("b-file.txt"), "fixture").unwrap();

    for kind in ["share", "file"] {
        let mut app = TestApp::with_settings("mouse=enable\n");
        if kind == "share" {
            let contacts: Vec<JID> = ["a@example.test", "b@example.test"]
                .into_iter()
                .map(|jid| jid.to_owned().into())
                .collect();
            let labels = HashMap::from([
                (contacts[0].clone(), "long label ".repeat(40)),
                (contacts[1].clone(), "target recipient".into()),
            ]);
            for (jid, label) in &labels {
                app.contacts.insert(jid.clone(), label.clone().into());
            }
            let mut picker = SharePicker::new(contacts, labels, HashMap::new());
            picker.query = "example.test".into();
            app.share_picker = Some(picker);
        } else {
            let mut picker = FilePickerState::open(directory.path()).unwrap();
            picker.enter_search();
            for character in "file".chars() {
                picker.push_query(character);
            }
            app.file_picker = Some(picker);
        }
        let mut terminal = Terminal::new(TestBackend::new(48, 9)).unwrap();
        draw(&mut app, &mut terminal);
        let initially_painted = (0..9).any(|row| {
            (0..48)
                .map(|column| terminal.backend().buffer()[(column, row)].symbol())
                .collect::<String>()
                .contains(if kind == "share" {
                    "long label"
                } else {
                    "a-file-"
                })
        });
        assert!(
            initially_painted,
            "{kind} long entry must really be painted"
        );
        app.on_terminal_event(mouse(MouseEventKind::ScrollDown, 10, 2));
        let (selected, query) = if kind == "share" {
            let picker = app.share_picker.as_ref().unwrap();
            (picker.selected, picker.query.as_str())
        } else {
            let picker = app.file_picker.as_ref().unwrap();
            (picker.selected, picker.query.as_str())
        };
        assert_eq!(selected, 1, "{kind} wheel must move to the next entry");
        assert_eq!(
            query,
            if kind == "share" {
                "example.test"
            } else {
                "file"
            }
        );
        draw(&mut app, &mut terminal);
        let buffer = terminal.backend().buffer();
        let cursor_visible = (2..6).any(|row| buffer[(1, row)].symbol() == ">");
        assert!(
            cursor_visible,
            "{kind} selected entry must be painted after wheel"
        );

        if kind == "share" {
            app.share_picker.as_mut().unwrap().move_selection(-1);
        } else {
            let picker = app.file_picker.as_mut().unwrap();
            picker.move_selection(-1);
            picker.end_search(); // Navigation keys are intentionally text while searching.
        }
        draw(&mut app, &mut terminal);
        app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        draw(&mut app, &mut terminal);
        let selected = if kind == "share" {
            app.share_picker.as_ref().unwrap().selected
        } else {
            app.file_picker.as_ref().unwrap().selected
        };
        assert_eq!(selected, 1, "{kind} keyboard cursor must still move");
        assert!(
            (2..6).any(|row| terminal.backend().buffer()[(1, row)].symbol() == ">"),
            "{kind} keyboard-selected entry must also be painted"
        );
    }
}

#[test]
fn a_clipped_url_opens_only_from_its_painted_cells_and_not_beneath_a_modal() {
    let mut app = TestApp::with_settings("mouse=enable\n");
    let chat: JID = "clipped-link@example.test".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    app.focus_pane = FocusPane::Conversation;
    app.add_message(message(
        &chat,
        "clipped-url",
        1,
        &format!("HIDDEN-BEGIN {} https://clip.test/x", "FILLER ".repeat(40)),
    ));
    app.add_message(message(&chat, "newest", 2, "NEWEST-VISIBLE"));
    app.message_list_state.set_selected_message("newest".into());
    let opener = RecordingUrlOpener::default();
    let plans = opener.0.clone();
    app.url_opener = Box::new(opener);
    let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    let conversation =
        ui::navigation_areas(terminal.backend().buffer().area, app.pane_visibility).conversation;
    draw(&mut app, &mut terminal);
    let buffer = terminal.backend().buffer();
    let rows = (conversation.top()..conversation.bottom())
        .map(|y| {
            (conversation.left()..conversation.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert!(rows.iter().all(|row| !row.contains("HIDDEN-BEGIN")));
    let (row, column) = rows
        .iter()
        .enumerate()
        .find_map(|(row, line)| {
            line.find("https://clip.test/x")
                .map(|byte| (row, line[..byte].chars().count()))
        })
        .expect("URL must actually be painted in the clipped message");
    assert!(column > 0, "URL must have preceding non-actionable space");
    let point = (
        conversation.left() + column as u16 + 8,
        conversation.top() + row as u16,
    );

    let blank = conversation.left() + column as u16 - 1;
    assert_eq!(
        buffer[(blank, point.1)].symbol(),
        " ",
        "blank-space fixture must not click painted text: {:?}",
        rows[row]
    );
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        blank,
        point.1,
    ));
    assert!(plans.borrow().is_empty(), "blank space must not open a URL");
    app.message_list_state.set_selected_message("newest".into());
    draw(&mut app, &mut terminal);
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        point.0,
        point.1,
    ));
    assert_eq!(
        plans.borrow().as_slice(),
        &[wp_tui::url::url_launch_plan("https://clip.test/x")]
    );
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("clipped-url")
    );

    app.message_list_state.set_selected_message("newest".into());
    app.url_picker = Some((vec!["https://modal.test".into()], 0));
    draw(&mut app, &mut terminal);
    app.on_terminal_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        point.0,
        point.1,
    ));
    assert_eq!(
        plans.borrow().len(),
        1,
        "modal must shield the underlying URL"
    );
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("newest")
    );
}

#[test]
fn clicking_a_clipped_message_selects_only_its_painted_rows() {
    let mut app = TestApp::with_settings("mouse=enable\n");
    let chat: JID = "clipped@example.test".to_owned().into();
    app.open_chat_by_jid(chat.clone());
    app.focus_pane = FocusPane::Conversation;
    app.add_message(message(
        &chat,
        "clipped",
        1,
        &format!("TARGET-BEGIN {}", "TARGET-CLIPPED ".repeat(40)),
    ));
    app.add_message(message(&chat, "newest", 2, "NEWEST-VISIBLE"));
    app.message_list_state.set_selected_message("newest".into());
    let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    draw(&mut app, &mut terminal);
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("newest")
    );
    let rendered_rows = |terminal: &Terminal<TestBackend>| {
        let buffer = terminal.backend().buffer();
        (0..12)
            .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
    };
    let rows = rendered_rows(&terminal);
    assert!(
        rows.iter().all(|row| !row.contains("TARGET-BEGIN")),
        "the first target row must be clipped"
    );
    let (column, row) = rows
        .iter()
        .enumerate()
        .find_map(|(row, text)| {
            text.find("TARGET-CLIPPED")
                .map(|column| (column as u16, row as u16))
        })
        .expect("a lower, clipped target row must still be painted");

    app.on_terminal_event(mouse(MouseEventKind::Down(MouseButton::Left), column, row));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("clipped")
    );

    app.message_list_state.set_selected_message("newest".into());
    app.url_picker = Some((vec!["https://example.test".into()], 0));
    draw(&mut app, &mut terminal);
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("newest")
    );
    app.on_terminal_event(mouse(MouseEventKind::Down(MouseButton::Left), column, row));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("newest")
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
    // The URL itself may wrap before its prefix is contiguous on one row.
    // Only the selected item paints a cursor at the modal list's left edge.
    let list_x = ui::centered_modal_layout(ratatui::layout::Rect::new(0, 0, 100, 20)).x + 1;
    let selected_is_painted = (3..15).any(|y| buffer[(list_x, y)].symbol() == ">");
    let painted_rows = (0..20)
        .map(|y| {
            (0..100)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert!(
        selected_is_painted,
        "wrapped URLs above the selection must not hide its marker; painted:\n{}",
        painted_rows.join("\n")
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
