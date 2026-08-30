use ratatui::{Terminal, backend::TestBackend};
use std::cell::RefCell;
use std::rc::Rc;
use whatsrust::{FileContent, FileKind, JID, Message, MessageContent, MessageInfo};

use wp_tui::app::actions::{AppAction, ConversationMode, FocusPane, MessageReactor, Section};
use wp_tui::app::unix_now;
use wp_tui::app::{App, events::MediaRenderPlan};
use wp_tui::ui;
mod common;
use common::TestApp;

struct FakeMessageReactor {
    calls: Rc<RefCell<Vec<(String, String, String, String, String)>>>,
    result: Result<(), whatsrust::MessageActionFailed>,
}

impl MessageReactor for FakeMessageReactor {
    fn react_to_message(
        &self,
        chat: &JID,
        sender: &JID,
        message_id: &whatsrust::MessageId,
        reaction: &str,
    ) -> Result<(), whatsrust::MessageActionFailed> {
        self.calls.borrow_mut().push((
            chat.0.to_string(),
            chat.0.to_string(),
            sender.0.to_string(),
            message_id.to_string(),
            reaction.to_owned(),
        ));
        self.result.clone()
    }

    fn react_to_message_in_chat(
        &self,
        target: &JID,
        destination: &JID,
        sender: &JID,
        message_id: &whatsrust::MessageId,
        reaction: &str,
    ) -> Result<(), whatsrust::MessageActionFailed> {
        self.calls.borrow_mut().push((
            target.0.to_string(),
            destination.0.to_string(),
            sender.0.to_string(),
            message_id.to_string(),
            reaction.to_owned(),
        ));
        self.result.clone()
    }
}

fn broadcast() -> JID {
    JID::from("status@broadcast".to_owned())
}

fn status_message(sender: &JID, id: &str, timestamp: i64, text: &str) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: broadcast(),
            sender: sender.clone(),
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

fn status_media_message(sender: &JID, id: &str, timestamp: i64, path: &str) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: broadcast(),
            sender: sender.clone(),
            mentions_self: false,
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::File(FileContent {
            kind: FileKind::Image,
            path: path.into(),
            ..Default::default()
        }),
    }
}

fn render(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            ui::draw_with_plan(frame, app, &mut media_render_plan)
        })
        .expect("status section should render");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn status_section_renders_contacts_sorted_by_recency_with_unseen_markers() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    let bob = JID::from("bob@s.whatsapp.net".to_owned());
    app.contacts.insert(alice.clone(), "Alice".into());
    app.contacts.insert(bob.clone(), "Bob".into());

    let now = unix_now();
    app.add_message(status_message(&alice, "a-old", now - 100, "Alice status"));
    app.add_message(status_message(&bob, "b-status", now - 50, "Bob status"));
    app.add_message(status_message(&alice, "a-new", now - 10, "Alice newest"));
    app.selected_section = Section::Status;

    assert_eq!(app.status_contacts, vec![alice.clone(), bob.clone()]);

    let output = render(&mut app, 100, 20);
    assert!(output.contains("Alice"), "Alice row missing: {output:?}");
    assert!(output.contains("Bob"), "Bob row missing: {output:?}");
    assert!(output.contains("now"), "relative time missing: {output:?}");
    assert!(output.contains("●"), "unseen marker missing: {output:?}");
    assert!(
        !output.contains("Status is not available yet."),
        "status placeholder must not render"
    );
    // The right pane stays empty until a contact is opened with Enter
    // (same contract as Chats: only the opened chat renders).
    assert!(
        !output.contains("Alice newest"),
        "statuses must not load before Enter: {output:?}"
    );

    app.dispatch_action(AppAction::OpenChat);
    let output = render(&mut app, 100, 20);
    // The right pane shows only the opened contact's statuses.
    assert!(output.contains("Alice newest"));
    assert!(output.contains("Alice status"));
    assert!(
        !output.contains("Bob status"),
        "right pane leaked another contact's statuses: {output:?}"
    );
}

#[test]
fn status_section_shows_empty_state_without_statuses() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;

    let output = render(&mut app, 100, 20);
    assert!(output.contains("No statuses yet"), "{output:?}");
    assert!(
        output.contains("Select a contact to view their statuses"),
        "open hint missing: {output:?}"
    );
    assert!(!output.contains("Status is not available yet."));
}

#[test]
fn enter_on_a_status_contact_marks_it_seen_and_focuses_its_pane() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    app.add_message(status_message(
        &alice,
        "a-new",
        unix_now() - 10,
        "Alice newest",
    ));
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::ChatList;

    assert!(app.has_unseen_statuses(&alice));
    app.dispatch_action(AppAction::OpenChat);
    assert_eq!(app.focus_pane, FocusPane::Conversation);
    assert!(!app.has_unseen_statuses(&alice));
}

#[test]
fn esc_from_a_status_pane_returns_focus_to_the_status_list() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    app.add_message(status_message(&alice, "a-new", unix_now(), "Alice newest"));
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::Conversation;

    app.dispatch_action(AppAction::CloseStatusPane);
    assert_eq!(app.focus_pane, FocusPane::ChatList);
}

#[test]
fn a_newer_status_after_viewing_is_unseen_again() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    let now = unix_now();
    app.add_message(status_message(&alice, "a-old", now - 100, "old"));
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::ChatList;

    app.dispatch_action(AppAction::OpenChat);
    assert!(!app.has_unseen_statuses(&alice));

    app.add_message(status_message(&alice, "a-new", now - 10, "new"));
    assert!(app.has_unseen_statuses(&alice));
}

#[test]
fn media_viewer_for_statuses_is_scoped_to_the_selected_contact() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    let bob = JID::from("bob@s.whatsapp.net".to_owned());
    let now = unix_now();
    // Bob's status is the oldest so Alice (newest) is the auto-selected
    // contact and bob.jpg must stay out of the viewer.
    app.add_message(status_media_message(&bob, "bob-pic", now - 300, "bob.jpg"));
    app.add_message(status_media_message(
        &alice,
        "alice-pic",
        now - 200,
        "alice.jpg",
    ));
    app.add_message(status_message(&alice, "alice-text", now - 100, "hello"));
    app.selected_section = Section::Status;

    // The contact must be opened (Enter) before its media is viewable,
    // mirroring the Chats contract where the viewer targets the open chat.
    app.dispatch_action(AppAction::OpenChat);
    app.message_list_state
        .set_selected_message("alice-pic".into());
    app.dispatch_action(AppAction::ViewMessage);

    let viewer = app.attachment_viewer.as_ref().expect("viewer should open");
    assert_eq!(viewer.attachment_count, 1);
    assert_eq!(viewer.attachments[0].message_id.as_ref(), "alice-pic");
}

#[test]
fn status_pane_still_blocks_edit_and_menu_actions() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    app.add_message(status_message(
        &alice,
        "a-new",
        unix_now() - 10,
        "Alice newest",
    ));
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::Conversation;
    app.message_list_state.set_selected_message("a-new".into());

    app.dispatch_action(AppAction::EditMessage);
    app.dispatch_action(AppAction::OpenMessageMenu);

    assert!(app.edit_message.is_none());
    assert!(app.message_menu.is_none());
}

#[test]
fn reply_from_a_status_opens_the_contact_inbox_with_quote() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    app.add_message(status_message(
        &alice,
        "a-new",
        unix_now() - 10,
        "Alice newest",
    ));
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::Conversation;
    app.message_list_state.set_selected_message("a-new".into());

    app.dispatch_action(AppAction::ReplyMessage);

    assert_eq!(
        app.selected_section,
        Section::Chats,
        "reply must jump to the inbox"
    );
    assert_eq!(
        app.open_chat(),
        Some(alice),
        "reply must open the sender's chat"
    );
    assert_eq!(
        app.composer
            .quote
            .as_ref()
            .map(|message| message.info.id.to_string()),
        Some("a-new".to_owned()),
        "the status must be quoted in the composer"
    );
    assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
    assert_eq!(app.focus_pane, FocusPane::Conversation);
}

#[test]
fn heart_reacts_to_the_selected_status_directly() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    app.add_message(status_message(
        &alice,
        "a-new",
        unix_now() - 10,
        "Alice newest",
    ));
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::Conversation;
    app.message_list_state.set_selected_message("a-new".into());
    let calls = Rc::new(RefCell::new(Vec::new()));
    app.message_reactor = Box::new(FakeMessageReactor {
        calls: calls.clone(),
        result: Ok(()),
    });

    app.dispatch_action(AppAction::ReactMessage);

    assert!(
        app.reaction_picker.is_none(),
        "status reactions skip the picker"
    );
    let recorded = calls.borrow();
    assert_eq!(recorded.len(), 1, "exactly one reaction must be sent");
    let (target, destination, sender, id, reaction) = &recorded[0];
    assert_eq!(target, "status@broadcast");
    assert_eq!(destination, "status@broadcast");
    assert_eq!(sender, "alice@s.whatsapp.net");
    assert_eq!(id, "a-new");
    assert_eq!(reaction, "💚");
}

#[test]
fn chats_section_still_lists_only_real_conversations() {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    app.add_message(Message {
        info: MessageInfo {
            id: "c1".into(),
            chat: chat.clone(),
            sender: chat.clone(),
            mentions_self: false,
            timestamp: unix_now() - 10,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::Text("hi".into()),
    });
    app.add_message(status_message(&alice, "a-new", unix_now(), "Alice newest"));
    app.sort_chats();

    assert!(app.sorted_chats.contains(&chat));
    assert!(
        !app.sorted_chats
            .iter()
            .any(|jid| jid.0.as_ref() == "status@broadcast")
    );
}

#[test]
fn status_renderers_have_a_dedicated_owner_and_ui_keeps_composition() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ui.rs"))
        .expect("ui source should be readable");
    let status_source =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ui/status.rs"))
            .expect("status renderer module should exist");

    assert!(status_source.contains("pub(super) fn render_status_contacts"));
    assert!(status_source.contains("pub(super) fn render_statuses_with_plan"));
    assert!(!source.contains("fn render_status_contacts"));
    assert!(!source.contains("fn render_statuses"));
    assert!(source.contains("render_status_contacts(frame, app, area)"));
    assert!(
        source.contains(
            "render_statuses_with_plan(frame, app, media_render_plan, areas.conversation)"
        )
    );
}

#[test]
fn status_empty_and_opened_states_are_safe_in_tiny_supported_frames() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    let empty_output = render(&mut app, 20, 6);
    assert!(
        empty_output.contains("No s"),
        "empty state missing: {empty_output:?}"
    );

    let mut opened_app = TestApp::new();
    opened_app.selected_section = Section::Status;
    opened_app.focus_pane = FocusPane::ChatList;
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    opened_app.contacts.insert(alice.clone(), "Alice".into());
    opened_app.add_message(status_message(
        &alice,
        "tiny-status",
        unix_now(),
        "tiny message",
    ));
    opened_app.status_contacts = vec![alice.clone()];
    opened_app.status_selection.select(Some(0));
    opened_app.open_selected_status();
    let opened_output = render(&mut opened_app, 60, 8);
    assert!(
        opened_output.contains("tiny") || opened_output.contains("Alice"),
        "opened state missing: {opened_output:?}"
    );
}
