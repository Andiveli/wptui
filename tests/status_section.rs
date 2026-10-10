use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use std::cell::RefCell;
use std::rc::Rc;
use whatsrust::{FileContent, FileKind, JID, Message, MessageContent, MessageInfo};

use wp_tui::app::actions::{
    ActionNotice, AppAction, ConversationMode, FocusPane, MessageReactor, MessageRevoker,
    PaneVisibility, Section, StatusCompositionState,
};
use wp_tui::app::contextual_actions::{ContextualAction, RowStyle};
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

struct FakeStatusRevoker {
    calls: Rc<RefCell<Vec<(String, String, String)>>>,
    result: Result<(), whatsrust::MessageActionFailed>,
}

impl MessageRevoker for FakeStatusRevoker {
    fn revoke_message(
        &self,
        chat: &JID,
        sender: &JID,
        message_id: &whatsrust::MessageId,
    ) -> Result<(), whatsrust::MessageActionFailed> {
        self.calls.borrow_mut().push((
            chat.0.to_string(),
            sender.0.to_string(),
            message_id.to_string(),
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
    let mut media_render_plan = MediaRenderPlan::default();
    let mut visibility_plan = wp_tui::app::read_receipts::VisibilityPlan::default();
    terminal
        .draw(|frame| ui::draw_with_plan(frame, app, &mut media_render_plan, &mut visibility_plan))
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
fn status_chat_list_menu_exposes_and_routes_create_status() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::ChatList;

    app.open_contextual_actions();

    let create_status = app
        .contextual_menu
        .as_ref()
        .expect("status chat-list menu")
        .0
        .iter()
        .position(|row| row.action_token == ContextualAction::CreateStatus)
        .expect("Create status action");
    let row = app.contextual_menu.as_ref().unwrap().0[create_status];
    assert_eq!(row.display_label, "Create status");
    assert_eq!(row.row_style, RowStyle::Enabled);

    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::NONE,
    )));

    assert_eq!(app.status_composition, StatusCompositionState::Authoring);
    assert!(app.contextual_menu.is_none());
}

#[test]
fn status_composition_enters_only_from_the_status_chat_list() {
    let mut app = TestApp::new();

    app.selected_section = Section::Chats;
    app.focus_pane = FocusPane::ChatList;
    app.dispatch_action(AppAction::StartStatusComposition);
    assert_eq!(app.status_composition, StatusCompositionState::Inactive);

    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::Conversation;
    app.dispatch_action(AppAction::StartStatusComposition);
    assert_eq!(app.status_composition, StatusCompositionState::Inactive);

    app.focus_pane = FocusPane::ChatList;
    app.dispatch_action(AppAction::StartStatusComposition);
    assert_eq!(app.status_composition, StatusCompositionState::Authoring);
}

#[test]
fn escape_navigates_then_cancels_and_resets_status_composition() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::ChatList;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.composer.insert_text("discard me");
    app.composer
        .queue_attachment("image.png".into(), FileKind::Image);
    app.composer.quote = Some(status_message(&broadcast(), "quoted", unix_now(), "quoted"));

    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.status_composition, StatusCompositionState::Navigating);
    assert_eq!(app.composer.text(), "discard me");
    assert_eq!(app.composer.pending.len(), 1);
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));

    assert_eq!(app.status_composition, StatusCompositionState::Inactive);
    assert!(app.composer.text().is_empty());
    assert!(app.composer.pending.is_empty());
    assert!(app.composer.quote.is_none());
    assert!(matches!(
        app.action_notice,
        Some(wp_tui::app::actions::ActionNotice::Cancelled)
    ));
}

#[test]
fn own_status_navigation_preserves_draft_and_excludes_other_contacts() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    app.add_message(status_message(
        &alice,
        "incoming",
        unix_now(),
        "OTHER CONTACT",
    ));
    for (id, timestamp) in [("mine-old", unix_now() - 2), ("mine-new", unix_now() - 1)] {
        let mut message = status_message(&broadcast(), id, timestamp, id);
        message.info.is_from_me = true;
        app.add_message(message);
    }
    app.selected_section = Section::Status;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.composer.insert_text("saved draft");
    app.composer
        .queue_attachment("image.png".into(), FileKind::Image);
    app.composer
        .queue_attachment("video.mp4".into(), FileKind::Video);
    let key = |app: &mut App, code| {
        app.on_terminal_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
    };
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.status_composition, StatusCompositionState::Navigating);
    let output = render(&mut app, 100, 20);
    assert!(output.contains("navigation") && output.contains("saved draft"));
    assert!(!output.contains("OTHER CONTACT"));
    key(&mut app, KeyCode::Char('k'));
    render(&mut app, 100, 20);
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("mine-old")
    );
    key(&mut app, KeyCode::Char('j'));
    render(&mut app, 100, 20);
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("mine-new")
    );
    key(&mut app, KeyCode::Char('i'));
    assert_eq!(app.status_composition, StatusCompositionState::Authoring);
    assert_eq!(app.composer.text(), "saved draft");
    assert_eq!(app.composer.pending.len(), 2);
}

#[test]
fn deleting_one_selected_own_status_revokes_its_canonical_id_and_hides_only_that_item() {
    let me = JID::from("me@s.whatsapp.net".to_owned());
    let other = JID::from("other@s.whatsapp.net".to_owned());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = TestApp::new();
    for mut item in [
        status_message(&me, "own-text", unix_now() - 2, "keep"),
        status_media_message(&me, "own-media", unix_now() - 1, "photo.png"),
    ] {
        item.info.is_from_me = true;
        app.add_message(item);
    }
    app.add_message(status_message(&other, "foreign", unix_now(), "other"));
    app.message_revoker = Box::new(FakeStatusRevoker {
        calls: calls.clone(),
        result: Ok(()),
    });
    app.selected_section = Section::Status;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    render(&mut app, 100, 20);
    app.message_list_state
        .set_selected_message("own-media".into());
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('d'),
        KeyModifiers::NONE,
    )));

    assert_eq!(
        calls.borrow().as_slice(),
        &[(
            "status@broadcast".into(),
            me.0.to_string(),
            "own-media".into()
        )]
    );
    assert_eq!(app.action_notice, Some(ActionNotice::DeletedMessage));
    assert!(app.message_status(&"own-media".into()).deleted);
    assert_eq!(app.own_status_messages(), vec!["own-text".into()]);
    assert_eq!(app.status_messages(&me), vec!["own-text".into()]);
    assert_eq!(app.status_messages(&other), vec!["foreign".into()]);
    assert_eq!(app.status_composition, StatusCompositionState::Navigating);

    let before = calls.borrow().len();
    app.message_list_state
        .set_selected_message("own-media".into());
    app.dispatch_action(AppAction::DeleteMessage);
    assert_eq!(
        calls.borrow().len(),
        before,
        "already deleted status must not be revoked twice"
    );

    app.message_list_state
        .set_selected_message("foreign".into());
    app.dispatch_action(AppAction::DeleteMessage);
    assert_eq!(
        calls.borrow().len(),
        before,
        "foreign status must not be revoked"
    );
    assert!(!app.message_status(&"foreign".into()).deleted);

    let mut pending = status_message(&me, "pending", unix_now(), "not published");
    pending.info.is_from_me = true;
    app.messages.insert(pending.info.id.clone(), pending);
    app.message_list_state
        .set_selected_message("pending".into());
    app.dispatch_action(AppAction::DeleteMessage);
    assert_eq!(
        calls.borrow().len(),
        before,
        "unpublished status must not be revoked"
    );
    assert!(!app.message_status(&"pending".into()).deleted);
}

#[test]
fn failed_own_status_delete_keeps_the_published_item_and_reports_failure() {
    let me = JID::from("me@s.whatsapp.net".to_owned());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = TestApp::new();
    let mut item = status_message(&me, "own-text", unix_now(), "keep on failure");
    item.info.is_from_me = true;
    app.add_message(item);
    app.selected_section = Section::Status;
    app.status_composition = StatusCompositionState::Navigating;
    app.focus_pane = FocusPane::Conversation;
    app.message_list_state
        .set_selected_message("own-text".into());
    app.message_revoker = Box::new(FakeStatusRevoker {
        calls: calls.clone(),
        result: Err(whatsrust::MessageActionFailed),
    });

    app.dispatch_action(AppAction::DeleteMessage);

    assert_eq!(
        calls.borrow().as_slice(),
        &[(
            "status@broadcast".into(),
            me.0.to_string(),
            "own-text".into()
        )]
    );
    assert!(!app.message_status(&"own-text".into()).deleted);
    assert_eq!(app.own_status_messages(), vec!["own-text".into()]);
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable("Could not delete message".into()))
    );
}

#[test]
fn contact_status_view_still_rejects_delete_without_mutating_the_item() {
    let contact = JID::from("other@s.whatsapp.net".to_owned());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = TestApp::new();
    app.add_message(status_message(
        &contact,
        "contact-status",
        unix_now(),
        "untouched",
    ));
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::Conversation;
    app.message_list_state
        .set_selected_message("contact-status".into());
    app.message_revoker = Box::new(FakeStatusRevoker {
        calls: calls.clone(),
        result: Ok(()),
    });

    app.dispatch_action(AppAction::DeleteMessage);

    assert!(calls.borrow().is_empty());
    assert!(!app.message_status(&"contact-status".into()).deleted);
    assert_eq!(app.status_messages(&contact), vec!["contact-status".into()]);
}

#[test]
fn ordinary_chat_text_delete_still_revokes_the_selected_message() {
    let chat = JID::from("friend@s.whatsapp.net".to_owned());
    let me = JID::from("me@s.whatsapp.net".to_owned());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = TestApp::new();
    let mut item = status_message(&me, "chat-text", unix_now(), "original");
    item.info.chat = chat.clone();
    item.info.is_from_me = true;
    app.add_message(item);
    app.open_chat_by_jid(chat.clone());
    app.selected_section = Section::Chats;
    app.focus_pane = FocusPane::Conversation;
    app.message_list_state
        .set_selected_message("chat-text".into());
    app.message_revoker = Box::new(FakeStatusRevoker {
        calls: calls.clone(),
        result: Ok(()),
    });

    app.dispatch_action(AppAction::DeleteMessage);

    assert_eq!(
        calls.borrow().as_slice(),
        &[(chat.0.to_string(), me.0.to_string(), "chat-text".into())]
    );
    assert_eq!(app.action_notice, Some(ActionNotice::DeletedMessage));
    assert!(app.message_status(&"chat-text".into()).deleted);
}

#[test]
fn authoring_picker_ignores_unrelated_group_permissions_and_cancel_preserves_draft() {
    let mut app = TestApp::new();
    let group = JID::from("123@g.us".to_owned());
    app.open_chat_by_jid(group.clone());
    app.group_permissions.insert(
        group.clone(),
        whatsrust::GroupInfo {
            jid: group,
            name: "Admins only".into(),
            is_announce: true,
            is_admin: false,
        },
    );
    let blocked = app.composer_blocked();
    app.composer.set_blocked(blocked);
    assert!(app.composer_blocked());
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::ChatList;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.composer.text(), "x");
    app.composer.insert_text("saved draft");
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('o'),
        KeyModifiers::CONTROL,
    )));
    assert!(app.file_picker.is_some());
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(app.file_picker.is_none());
    assert_eq!(app.status_composition, StatusCompositionState::Authoring);
    assert_eq!(app.composer.text(), "xsaved draft");
    assert!(app.composer.pending.is_empty());
}

#[test]
fn submitting_rejects_chat_actions_but_preserves_global_controls() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    let mut own = status_message(&broadcast(), "mine-submitting", unix_now(), "mine");
    own.info.is_from_me = true;
    app.add_message(own);
    app.message_list_state
        .set_selected_message("mine-submitting".into());
    app.status_composition = StatusCompositionState::Submitting;
    app.focus_pane = FocusPane::Conversation;
    app.dispatch_action(AppAction::OpenMessageMenu);
    app.dispatch_action(AppAction::ReplyMessage);
    assert!(app.message_menu.is_none());
    assert_eq!(app.selected_section, Section::Status);
    app.dispatch_action(AppAction::ToggleLogs);
    assert!(app.show_logs);
    app.dispatch_action(AppAction::Quit);
    assert!(app.should_quit);
}

#[test]
fn navigating_own_status_does_not_mark_stale_chat_read() {
    let mut app = TestApp::new();
    let chat = JID::from("stale@s.whatsapp.net".to_owned());
    app.open_chat_by_jid(chat.clone());
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::ChatList;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.add_message(Message {
        info: MessageInfo {
            id: "unread".into(),
            chat: chat.clone(),
            sender: chat.clone(),
            mentions_self: false,
            timestamp: unix_now(),
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::Text("unread".into()),
    });
    let mut own = status_message(&broadcast(), "mine", unix_now(), "mine");
    own.info.is_from_me = true;
    app.add_message(own);
    app.timeline
        .entry(chat.clone())
        .or_default()
        .pending_new_messages = 2;
    let pending_before = app.pending_new_messages(&chat);
    assert_eq!(pending_before, 2);
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    app.dispatch_action(AppAction::JumpBottom);
    assert_eq!(app.pending_new_messages(&chat), pending_before);
}

#[test]
fn navigating_community_group_to_latest_marks_it_read() {
    let mut app = TestApp::new();
    let group = JID::from("group@g.us".to_owned());
    app.add_message(Message {
        info: MessageInfo {
            id: "group-unread".into(),
            chat: group.clone(),
            sender: JID::from("member@s.whatsapp.net".to_owned()),
            mentions_self: false,
            timestamp: unix_now(),
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::Text("group message".into()),
    });
    app.timeline
        .entry(group.clone())
        .or_default()
        .pending_new_messages = 1;
    app.open_chat = Some(group.clone());
    app.selected_section = Section::Communities;
    app.focus_pane = FocusPane::Conversation;
    app.message_list_state.select(Some(1));
    assert_eq!(app.pending_new_messages(&group), 1);

    app.dispatch_action(AppAction::JumpBottom);

    assert_eq!(app.message_list_state.selected, Some(0));
    assert_eq!(app.pending_new_messages(&group), 0);
}

#[test]
fn stale_admin_only_chat_does_not_hide_status_draft() {
    let mut app = TestApp::new();
    let group = JID::from("123@g.us".to_owned());
    app.open_chat_by_jid(group.clone());
    app.group_permissions.insert(
        group.clone(),
        whatsrust::GroupInfo {
            jid: group,
            name: "Admins only".into(),
            is_announce: true,
            is_admin: false,
        },
    );
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::ChatList;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.composer.insert_text("VISIBLE STATUS DRAFT");
    let output = render(&mut app, 100, 20);
    assert!(output.contains("VISIBLE STATUS DRAFT"), "{output:?}");
    assert!(!output.contains("Admin-only group"), "{output:?}");
}

#[test]
fn navigating_status_keeps_global_shortcuts() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('l'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )));
    assert!(app.show_logs);
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL,
    )));
    assert!(app.should_quit);
}

#[test]
fn navigating_status_rejects_chat_actions() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    app.dispatch_action(AppAction::OpenMessageMenu);
    assert!(app.message_menu.is_none());
    assert_eq!(app.status_composition, StatusCompositionState::Navigating);
}

#[test]
fn status_picker_owns_keys_and_rejects_non_media_without_losing_draft() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.composer.insert_text("saved draft");
    let temp = std::env::temp_dir().join(format!(
        "wptui-status-picker-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos()
    ));
    std::fs::create_dir(&temp).expect("create isolated picker directory");
    for name in ["image.png", "video.mp4", "document.pdf"] {
        std::fs::write(temp.join(name), b"test media").expect("create picker entry");
    }
    app.file_picker = Some(wp_tui::file_picker::FilePickerState::open(&temp).unwrap());
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::NONE,
    )));
    assert!(app.file_picker.as_ref().unwrap().searching);
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.file_picker.as_ref().unwrap().query, "j");
    assert_eq!(app.composer.text(), "saved draft");
    app.file_picker.as_mut().unwrap().end_search();
    app.file_picker.as_mut().unwrap().backspace_query();
    for name in ["image.png", "video.mp4", "document.pdf"] {
        let picker = app.file_picker.as_mut().unwrap();
        let index = picker
            .visible_entries()
            .iter()
            .position(|entry| entry.name == name)
            .unwrap();
        picker.move_selection(index as isize - picker.selected as isize);
        assert!(picker.toggle_selected());
    }
    assert_eq!(app.file_picker.as_ref().unwrap().selected_count(), 3);
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));
    assert!(app.file_picker.is_none());
    assert!(matches!(
        app.action_notice,
        Some(ActionNotice::Unsupported(_))
    ));
    assert_eq!(app.status_composition, StatusCompositionState::Authoring);
    assert_eq!(app.composer.text(), "saved draft");
    assert_eq!(app.composer.pending.len(), 2);
    assert!(
        app.composer
            .pending
            .iter()
            .any(|file| file.path.to_string().ends_with("image.png")
                && matches!(file.kind, FileKind::Image))
    );
    assert!(
        app.composer
            .pending
            .iter()
            .any(|file| file.path.to_string().ends_with("video.mp4")
                && matches!(file.kind, FileKind::Video))
    );
    assert!(
        !app.composer
            .pending
            .iter()
            .any(|file| file.path.to_string().ends_with("document.pdf"))
    );
    assert_eq!(app.pending_status_sends, 0);
    for name in ["image.png", "video.mp4", "document.pdf"] {
        std::fs::remove_file(temp.join(name)).expect("remove picker entry");
    }
    std::fs::remove_dir(&temp).expect("remove isolated picker directory");
}

#[test]
fn status_composition_renders_status_specific_authoring_and_submitting_feedback() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    app.status_composition = StatusCompositionState::Authoring;
    app.composer.insert_text("A new status");

    let authoring = render(&mut app, 100, 20);
    assert!(authoring.contains("Status update"), "{authoring:?}");
    assert!(authoring.contains("Enter publish"), "{authoring:?}");
    assert!(authoring.contains("A new status"), "{authoring:?}");

    app.status_composition = StatusCompositionState::Submitting;
    app.pending_status_sends = 2;
    let submitting = render(&mut app, 100, 20);
    assert!(submitting.contains("Publishing status"), "{submitting:?}");
    assert!(submitting.contains("2 updates pending"), "{submitting:?}");
}

#[test]
fn creating_status_shows_only_loaded_own_statuses_above_composer() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    let now = unix_now();
    app.add_message(status_message(
        &alice,
        "alice",
        now - 1,
        "PRIVATE ALICE STATUS",
    ));
    let mut own = status_message(&alice, "mine", now, "MY LOADED STATUS");
    own.info.is_from_me = true;
    app.add_message(own);
    app.selected_section = Section::Status;
    app.focus_pane = FocusPane::ChatList;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.composer.insert_text("MY DRAFT");

    let output = render(&mut app, 100, 20);
    assert!(output.contains("Create status"), "{output:?}");
    assert!(!output.contains("My Statuses"), "{output:?}");
    assert!(output.contains("MY LOADED STATUS"), "{output:?}");
    assert!(output.contains("MY DRAFT"), "{output:?}");
    assert!(!output.contains("PRIVATE ALICE STATUS"), "{output:?}");
    assert!(
        output.find("MY LOADED STATUS") < output.find("MY DRAFT"),
        "own status should render above composer: {output:?}"
    );
}

#[test]
fn creating_status_after_scrolling_incoming_resets_own_status_viewport() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    let now = unix_now();
    for index in 0..16 {
        app.add_message(status_message(
            &alice,
            &format!("incoming-{index}"),
            now - 10 + index,
            &format!("INCOMING STATUS {index}"),
        ));
    }
    let mut own = status_message(&broadcast(), "mine", now, "MY LOADED STATUS");
    own.info.is_from_me = true;
    app.add_message(own);
    app.selected_section = Section::Status;
    let alice_index = app
        .status_contacts
        .iter()
        .position(|contact| contact == &alice)
        .expect("Alice should appear in the status contact list");
    app.status_selection.select(Some(alice_index));
    app.dispatch_action(AppAction::OpenChat);
    assert_eq!(app.open_status_contact.as_ref(), Some(&alice));
    assert!(render(&mut app, 100, 12).contains("INCOMING STATUS"));
    app.dispatch_action(AppAction::JumpTop);
    render(&mut app, 100, 12);
    for _ in 0..12 {
        app.dispatch_action(AppAction::SelectNext);
        render(&mut app, 100, 12);
    }
    assert!(app.message_list_state.offset > 0);
    let selected = app.message_list_state.selected.expect("status selected");
    let selected_message = app.message_list_state.get_selected_message();
    assert_eq!(
        selected_message,
        Some(format!("incoming-{}", 15 - selected).into())
    );
    app.dispatch_action(AppAction::CloseStatusPane);
    app.dispatch_action(AppAction::StartStatusComposition);

    assert_eq!(app.status_composition, StatusCompositionState::Authoring);
    assert_eq!(app.message_list_state.selected, None);
    assert_eq!(app.message_list_state.offset, 0);
    assert_eq!(app.message_list_state.get_selected_message(), None);
    let output = render(&mut app, 100, 20);
    assert!(output.contains("MY LOADED STATUS"), "{output:?}");
    assert!(!output.contains("INCOMING STATUS"), "{output:?}");
}

#[test]
fn creating_status_without_loaded_own_statuses_shows_empty_state_and_composer() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    app.add_message(status_message(
        &alice,
        "alice",
        unix_now(),
        "PRIVATE ALICE STATUS",
    ));
    app.selected_section = Section::Status;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.composer.insert_text("MY DRAFT");

    let output = render(&mut app, 100, 20);
    assert!(output.contains("No statuses published yet"), "{output:?}");
    assert!(output.contains("MY DRAFT"), "{output:?}");
    assert!(!output.contains("PRIVATE ALICE STATUS"), "{output:?}");
}

#[test]
fn creating_status_in_short_frame_keeps_composer_and_own_status_visible() {
    let mut app = TestApp::new();
    let mut own = status_message(&broadcast(), "mine", unix_now(), "MY STATUS");
    own.info.is_from_me = true;
    app.add_message(own);
    app.selected_section = Section::Status;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.composer.insert_text("DRAFT");

    let output = render(&mut app, 60, 8);
    assert!(output.contains("MY STATUS"), "{output:?}");
    assert!(output.contains("DRAFT"), "{output:?}");
    assert!(output.contains("Create status"), "{output:?}");
    assert!(!output.contains("My Statuses"), "{output:?}");
}

#[test]
fn narrow_status_composer_keeps_mode_actions_discoverable() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    app.status_composition = StatusCompositionState::Authoring;
    let editing = render(&mut app, 60, 12);
    assert!(editing.contains("Enter"), "{editing:?}");
    assert!(editing.contains("Esc"), "{editing:?}");
    app.composer.insert_text("draft");
    let with_draft = render(&mut app, 60, 12);
    assert!(with_draft.contains("Ctrl+O"), "{with_draft:?}");

    app.status_composition = StatusCompositionState::Navigating;
    let navigating = render(&mut app, 60, 12);
    assert!(navigating.contains("i Esc"), "{navigating:?}");
    assert!(navigating.contains("Esc"), "{navigating:?}");
}

#[test]
fn narrow_status_draft_wraps_inside_composer_below_own_status() {
    let mut app = TestApp::new();
    let mut own = status_message(&broadcast(), "mine", unix_now(), "MY STATUS");
    own.info.is_from_me = true;
    app.add_message(own);
    app.selected_section = Section::Status;
    app.dispatch_action(AppAction::StartStatusComposition);
    app.composer.insert_text("ABCDEFGHIJ1234567890abcdefghij");
    let width = 60usize;
    let output = render(&mut app, width as u16, 14);
    let cells: Vec<char> = output.chars().collect();
    let rows: Vec<String> = cells
        .chunks(width)
        .map(|row| row.iter().collect())
        .collect();
    let status_row = rows
        .iter()
        .position(|row| row.contains("MY STATUS"))
        .expect("own status visible");
    let first = rows
        .iter()
        .position(|row| row.contains("ABCDEFGHIJ"))
        .expect("draft starts");
    let continuation = rows
        .iter()
        .position(|row| row.contains("34567890abcd"))
        .unwrap_or_else(|| panic!("draft continues: {rows:?}"));
    assert!(status_row < first && first < continuation, "{rows:?}");
    assert!(
        rows[first].contains("│") && rows[continuation].contains("│"),
        "{rows:?}"
    );
}

#[test]
fn submitting_status_composition_keeps_progress_and_notices_visible() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    app.status_composition = StatusCompositionState::Submitting;
    app.pending_status_sends = 2;

    for (notice, expected) in [
        (ActionNotice::StatusPublished, "StatusPublished"),
        (ActionNotice::Cancelled, "Cancelled"),
        (
            ActionNotice::Unavailable("send failed".into()),
            "Unavailable",
        ),
    ] {
        app.action_notice = Some(notice);
        let output = render(&mut app, 100, 20);
        assert!(output.contains("2 updates pending"), "{output:?}");
        assert!(output.contains(expected), "{output:?}");
    }
}

#[test]
fn status_notices_remain_visible_across_composition_states() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    app.status_composition = StatusCompositionState::Authoring;
    app.action_notice = Some(ActionNotice::Unsupported("text only".into()));
    let authoring = render(&mut app, 100, 20);
    assert!(authoring.contains("Unsupported"), "{authoring:?}");

    for (notice, expected) in [
        (ActionNotice::StatusPublished, "StatusPublished"),
        (ActionNotice::Cancelled, "Cancelled"),
        (
            ActionNotice::Unavailable("send failed".into()),
            "Unavailable",
        ),
    ] {
        app.status_composition = StatusCompositionState::Inactive;
        app.action_notice = Some(notice);
        let output = render(&mut app, 100, 20);
        assert!(output.contains(expected), "{output:?}");
    }
}

#[test]
fn uncertain_send_warning_remains_visible_at_narrow_widths() {
    let mut app = TestApp::new();
    app.selected_section = Section::Status;
    app.status_composition = StatusCompositionState::Authoring;
    app.pane_visibility = PaneVisibility {
        section_rail: false,
        chat_list: false,
    };
    app.status_retry_warning = true;
    app.action_notice = Some(ActionNotice::Unavailable(
        "Could not publish status: SendFailed. The failed item may have been published; check your statuses before retrying".into(),
    ));
    for (width, expected) in [(18, "CHECK RETRY"), (32, "CHECK BEFORE RETRY")] {
        let output = render(&mut app, width, 20);
        assert!(output.contains(expected), "{output:?}");
    }
}

#[test]
fn inactive_status_composition_preserves_incoming_status_view() {
    let mut app = TestApp::new();
    let alice = JID::from("alice@s.whatsapp.net".to_owned());
    app.add_message(status_message(
        &alice,
        "incoming-status",
        unix_now(),
        "Incoming status remains visible",
    ));
    app.selected_section = Section::Status;
    app.open_selected_status();

    let output = render(&mut app, 100, 20);
    assert!(
        output.contains("Incoming status remains visible"),
        "{output:?}"
    );
    assert!(!output.contains("Status update"), "{output:?}");
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
    assert!(source.contains("render_statuses_with_plan("));
    assert!(source.contains("media_render_plan,"));
    assert!(source.contains("visibility_plan,"));
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
