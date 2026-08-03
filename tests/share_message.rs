use std::cell::RefCell;
use std::rc::Rc;

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use whatsrust::{ForwardFailure, ForwardReport, JID, Message, MessageContent, MessageInfo};
use wp_tui::app::actions::{ActionNotice, FocusPane, MessageForwarder};
use wp_tui::app::{App, Chat};
use wp_tui::ui::render_chats;
mod common;
use common::TestApp;

#[derive(Default)]
struct RecordingForwarder {
    calls: Rc<RefCell<Vec<(String, Vec<String>, bool)>>>,
    report: ForwardReport,
}
impl MessageForwarder for RecordingForwarder {
    fn forward_message(&self, source: &Message, destinations: &[JID]) -> ForwardReport {
        self.calls.borrow_mut().push((
            source.info.id.to_string(),
            destinations.iter().map(|jid| jid.0.to_string()).collect(),
            matches!(source.message, MessageContent::File(_)),
        ));
        self.report
    }
}
fn message(id: &str, content: MessageContent) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: JID::from("source@s.whatsapp.net".to_owned()),
            sender: JID::from("source@s.whatsapp.net".to_owned()),
            timestamp: 1,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: content,
    }
}
fn app_with_selected_message(content: MessageContent) -> TestApp {
    let mut app = TestApp::new();
    let message = message("source", content);
    app.messages.insert(message.info.id.clone(), message);
    app.message_list_state.set_selected_message("source".into());
    app.contacts
        .insert(JID::from("alice@s.whatsapp.net".to_owned()), "Alice".into());
    app.contacts
        .insert(JID::from("bob@s.whatsapp.net".to_owned()), "Bob".into());
    app.focus_pane = FocusPane::Conversation;
    app
}
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn share_picker_searches_toggles_multiple_contacts_and_forwards_once() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = app_with_selected_message(MessageContent::Text("hello".into()));
    app.message_forwarder = Box::new(RecordingForwarder {
        calls: calls.clone(),
        report: ForwardReport {
            succeeded: 2,
            failed: 0,
            failure: Default::default(),
        },
    });
    app.on_terminal_event(key(KeyCode::Char('s')));
    app.on_terminal_event(key(KeyCode::Char('a')));
    app.on_terminal_event(key(KeyCode::Char('l')));
    assert_eq!(
        app.share_picker.as_ref().unwrap().visible_contacts().len(),
        1
    );
    app.on_terminal_event(key(KeyCode::Char(' ')));
    app.on_terminal_event(key(KeyCode::Backspace));
    app.on_terminal_event(key(KeyCode::Down));
    app.on_terminal_event(key(KeyCode::Char(' ')));
    assert_eq!(app.share_picker.as_ref().unwrap().selected_count(), 2);
    app.on_terminal_event(key(KeyCode::Enter));
    assert_eq!(
        calls.borrow().as_slice(),
        &[(
            "source".into(),
            vec!["alice@s.whatsapp.net".into(), "bob@s.whatsapp.net".into()],
            false
        )]
    );
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Forwarded {
            succeeded: 2,
            failed: 0,
            failure: Default::default()
        })
    );
}

#[test]
fn share_picker_cancel_empty_selection_and_modal_precedence_have_no_side_effects() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = app_with_selected_message(MessageContent::Text("hello".into()));
    app.message_forwarder = Box::new(RecordingForwarder {
        calls: calls.clone(),
        report: ForwardReport::default(),
    });
    app.on_terminal_event(key(KeyCode::Char('s')));
    app.on_terminal_event(key(KeyCode::Char('r')));
    assert!(app.share_picker.is_some());
    app.on_terminal_event(key(KeyCode::Enter));
    assert!(calls.borrow().is_empty());
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable(
            "Select at least one contact".into()
        ))
    );
    app.on_terminal_event(key(KeyCode::Esc));
    assert!(app.share_picker.is_none());
}

#[test]
fn share_reports_all_failed_forwarding() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = app_with_selected_message(MessageContent::Text("hello".into()));
    app.message_forwarder = Box::new(RecordingForwarder {
        calls: calls.clone(),
        report: ForwardReport {
            succeeded: 0,
            failed: 1,
            failure: Default::default(),
        },
    });
    app.on_terminal_event(key(KeyCode::Char('s')));
    app.on_terminal_event(key(KeyCode::Char(' ')));
    app.on_terminal_event(key(KeyCode::Enter));
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Forwarded {
            succeeded: 0,
            failed: 1,
            failure: Default::default()
        })
    );
}

#[test]
fn share_forwards_files_and_rejects_broadcast_sources() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = app_with_selected_message(MessageContent::File(Default::default()));
    app.message_forwarder = Box::new(RecordingForwarder {
        calls: calls.clone(),
        report: ForwardReport {
            succeeded: 1,
            failed: 1,
            failure: Default::default(),
        },
    });
    app.on_terminal_event(key(KeyCode::Char('s')));
    app.on_terminal_event(key(KeyCode::Char(' ')));
    app.on_terminal_event(key(KeyCode::Down));
    app.on_terminal_event(key(KeyCode::Char(' ')));
    app.on_terminal_event(key(KeyCode::Enter));
    assert!(calls.borrow()[0].2);
    let mut status = app_with_selected_message(MessageContent::Text("status".into()));
    status.messages.get_mut("source").unwrap().info.chat = JID::from("status@broadcast".to_owned());
    status.on_terminal_event(key(KeyCode::Char('s')));
    assert_eq!(
        status.action_notice,
        Some(ActionNotice::Unavailable("Forward is not available".into()))
    );
}

fn add_contacts_with_activity(app: &mut App<'_>) {
    for index in 0..10 {
        let jid = JID::from(format!("contact-{index}@s.whatsapp.net"));
        app.contacts
            .insert(jid.clone(), format!("Contact {index}").into());
        app.chats.insert(
            jid.clone(),
            Chat {
                jid,
                last_message_time: Some(index),
            },
        );
    }
}

#[test]
fn share_picker_sorts_by_activity_and_keeps_navigation_visible_in_small_viewport() {
    let mut app = app_with_selected_message(MessageContent::Text("hello".into()));
    add_contacts_with_activity(&mut app);
    app.on_terminal_event(key(KeyCode::Char('s')));
    assert_eq!(
        app.share_picker.as_ref().unwrap().visible_contacts()[0]
            .0
            .as_ref(),
        "contact-9@s.whatsapp.net"
    );
    for _ in 0..9 {
        app.on_terminal_event(key(KeyCode::Char('j')));
    }
    let mut terminal = Terminal::new(TestBackend::new(30, 7)).unwrap();
    terminal
        .draw(|frame| render_chats(frame, &mut app, Rect::new(0, 0, 30, 7)))
        .unwrap();
    let picker = app.share_picker.as_ref().unwrap();
    assert_eq!(picker.viewport().len(), 1);
    assert!(picker.viewport().contains(&picker.selected));
    app.on_terminal_event(key(KeyCode::Char('k')));
    app.on_terminal_event(key(KeyCode::Up));
    let picker = app.share_picker.as_ref().unwrap();
    assert!(picker.viewport().contains(&picker.selected));
}

#[test]
fn share_picker_search_resets_position_and_preserves_selections_across_scroll() {
    let mut app = app_with_selected_message(MessageContent::Text("hello".into()));
    add_contacts_with_activity(&mut app);
    app.on_terminal_event(key(KeyCode::Char('s')));
    app.share_picker.as_mut().unwrap().set_viewport_height(2);
    app.on_terminal_event(key(KeyCode::Char(' ')));
    for _ in 0..5 {
        app.on_terminal_event(key(KeyCode::Down));
    }
    app.on_terminal_event(key(KeyCode::Char(' ')));
    for character in ['c', 'o', 'n'] {
        app.on_terminal_event(key(KeyCode::Char(character)));
    }
    let picker = app.share_picker.as_ref().unwrap();
    assert_eq!(
        (
            picker.selected,
            picker.offset,
            picker.visible_contacts().len(),
            picker.selected_count()
        ),
        (0, 0, 10, 2)
    );
    app.on_terminal_event(key(KeyCode::Backspace));
    assert_eq!(app.share_picker.as_ref().unwrap().selected_count(), 2);
}

#[test]
fn forwarding_failure_notices_are_concise_and_typed() {
    let mut app = app_with_selected_message(MessageContent::Text("hello".into()));
    for (failure, expected) in [
        (ForwardFailure::SourceUnavailable, "Source unavailable"),
        (ForwardFailure::InvalidDestination, "Invalid destination"),
        (ForwardFailure::SendFailed, "Send failed"),
    ] {
        app.action_notice = Some(ActionNotice::Forwarded {
            succeeded: 0,
            failed: 1,
            failure,
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
        terminal
            .draw(|frame| render_chats(frame, &mut app, Rect::new(0, 0, 80, 12)))
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains(expected), "{rendered}");
    }
}
