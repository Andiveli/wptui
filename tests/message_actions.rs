use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::cell::RefCell;
use std::rc::Rc;
use tempfile::tempdir;
use wp_tui::app::App;
use wp_tui::app::MessageAction;
use wp_tui::app::actions::{
    ActionNotice, AppAction, ConversationMode, MessageForwarder, MessageMenuAction,
};
use wp_tui::db::DatabaseHandler;
use wp_tui::ui::message_list::reply_summary;
mod common;
use common::TestApp;

#[derive(Default)]
struct RecordingClipboard(Option<String>);
impl wp_tui::app::actions::ClipboardWriter for RecordingClipboard {
    fn write_text(&mut self, text: &str) -> Result<(), wp_tui::app::actions::ClipboardWriteError> {
        self.0 = Some(text.into());
        Ok(())
    }
    fn written_text(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

struct FailingClipboard;

impl wp_tui::app::actions::ClipboardWriter for FailingClipboard {
    fn write_text(&mut self, _: &str) -> Result<(), wp_tui::app::actions::ClipboardWriteError> {
        Err(wp_tui::app::actions::ClipboardWriteError)
    }
}

struct FakeMessageEditor {
    calls: Rc<RefCell<Vec<(String, String, String)>>>,
    result: Result<(), whatsrust::MessageActionFailed>,
}

struct FakeMessageReactor {
    calls: ReactorCalls,
    result: Result<(), whatsrust::MessageActionFailed>,
}

type ReactorCalls = Rc<RefCell<Vec<(String, String, String, String)>>>;

struct FakeMessageRevoker {
    calls: Rc<RefCell<Vec<(String, String, String)>>>,
    result: Result<(), whatsrust::MessageActionFailed>,
}

impl wp_tui::app::actions::MessageEditor for FakeMessageEditor {
    fn edit_message(
        &self,
        chat: &whatsrust::JID,
        message_id: &whatsrust::MessageId,
        replacement: &str,
    ) -> Result<(), whatsrust::MessageActionFailed> {
        self.calls.borrow_mut().push((
            chat.0.to_string(),
            message_id.to_string(),
            replacement.into(),
        ));
        self.result.clone()
    }
}

impl wp_tui::app::actions::MessageReactor for FakeMessageReactor {
    fn react_to_message(
        &self,
        chat: &whatsrust::JID,
        sender: &whatsrust::JID,
        message_id: &whatsrust::MessageId,
        reaction: &str,
    ) -> Result<(), whatsrust::MessageActionFailed> {
        self.calls.borrow_mut().push((
            chat.0.to_string(),
            sender.0.to_string(),
            message_id.to_string(),
            reaction.into(),
        ));
        self.result.clone()
    }
}

impl wp_tui::app::actions::MessageRevoker for FakeMessageRevoker {
    fn revoke_message(
        &self,
        chat: &whatsrust::JID,
        sender: &whatsrust::JID,
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

#[test]
fn selected_message_resolves_the_selected_id_from_app_messages() {
    let mut app = TestApp::new();
    let target = message("target", None, "original text");
    app.messages.insert(target.info.id.clone(), target);
    app.message_list_state.set_selected_message("target".into());

    assert_eq!(
        app.selected_message()
            .map(|message| message.info.id.as_ref()),
        Some("target")
    );
}

#[test]
fn following_a_reply_reference_selects_the_target_and_requests_scroll() {
    let mut app = TestApp::new();
    let target = message("target", None, "original text");
    let reply = message("reply", Some("target"), "reply text");
    app.messages.insert(target.info.id.clone(), target);
    app.messages.insert(reply.info.id.clone(), reply);
    app.message_list_state.set_selected_message("reply".into());

    assert!(app.follow_selected_reference());
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("target")
    );
    assert!(app.message_list_state.update_selected);
}

#[test]
fn missing_reply_reference_keeps_the_current_selection() {
    let mut app = TestApp::new();
    let reply = message("reply", Some("missing"), "reply text");
    app.messages.insert(reply.info.id.clone(), reply);
    app.message_list_state.set_selected_message("reply".into());

    assert!(!app.follow_selected_reference());
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("reply")
    );
}

#[test]
fn replies_render_the_referenced_author_and_excerpt() {
    let quoted = message("quoted", None, "original text");

    assert_eq!(reply_summary(&quoted, "Alice"), "> Alice: original text");
}

#[test]
fn replies_render_a_bounded_single_line_unicode_excerpt() {
    let quoted = message(
        "quoted",
        None,
        "😀😀😀😀😀\nalpha beta gamma delta epsilon zeta eta",
    );

    assert_eq!(
        reply_summary(&quoted, "Alice"),
        "> Alice: 😀😀😀😀😀 alpha beta gamma delta epsilon zet…"
    );
}

#[test]
fn owned_text_edit_prefills_and_submits_trimmed_replacement() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = owned_text_app("original");
    let directory = tempdir().unwrap();
    replace_database(&mut app, directory.path());
    app.message_editor = Box::new(FakeMessageEditor {
        calls: calls.clone(),
        result: Ok(()),
    });

    app.dispatch_action(AppAction::EditMessage);
    assert_eq!(app.conversation_mode, ConversationMode::EditingMessage);
    assert_eq!(app.composer.text(), "original");
    app.composer.replace_text(" replacement ");
    app.dispatch_action(AppAction::Composer(
        wp_tui::app::actions::ComposerAction::Submit,
    ));

    assert_eq!(
        calls.borrow().as_slice(),
        &[(
            "chat@example.test".into(),
            "owned".into(),
            "replacement".into()
        )]
    );
    assert_eq!(app.action_notice, Some(ActionNotice::EditedMessage));
    assert_eq!(app.conversation_mode, ConversationMode::MessageNavigation);
    assert!(matches!(
        &app.messages["owned"].message,
        whatsrust::MessageContent::Text(text) if text.as_ref() == "replacement"
    ));
    assert!(app.message_status(&"owned".into()).edited);
    assert_eq!(app.message_actions["owned"].len(), 1);
    // Join the writer thread while `directory`/app.db still exist (the outer
    // tempdir is dropped before the app).
    app.db_handler.stop();
}

#[test]
fn missing_or_failed_edit_does_not_call_bridge_or_mutate_message() {
    let missing_calls = Rc::new(RefCell::new(Vec::new()));
    let mut missing = TestApp::new();
    missing.message_editor = Box::new(FakeMessageEditor {
        calls: missing_calls.clone(),
        result: Ok(()),
    });
    missing.dispatch_action(AppAction::EditMessage);
    assert!(missing_calls.borrow().is_empty());
    assert_eq!(
        missing.action_notice,
        Some(ActionNotice::Unavailable("Edit is not available".into()))
    );

    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = owned_text_app("original");
    app.message_editor = Box::new(FakeMessageEditor {
        calls: calls.clone(),
        result: Err(whatsrust::MessageActionFailed),
    });
    app.dispatch_action(AppAction::EditMessage);
    app.composer.replace_text("replacement");
    app.dispatch_action(AppAction::Composer(
        wp_tui::app::actions::ComposerAction::Submit,
    ));
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(app.composer.text(), "replacement");
    assert!(
        matches!(&app.messages["owned"].message, whatsrust::MessageContent::Text(text) if text.as_ref() == "original")
    );
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable("Could not edit message".into()))
    );
    assert!(app.message_actions.get("owned").is_none());
}

#[test]
fn reaction_picker_handles_modal_navigation_without_changing_focus() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = owned_text_app("react");
    app.messages.get_mut("owned").unwrap().info.sender =
        whatsrust::JID("author@example.test".into());
    app.message_reactor = Box::new(FakeMessageReactor {
        calls: calls.clone(),
        result: Ok(()),
    });

    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('r'),
        KeyModifiers::NONE,
    )));
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Right,
        KeyModifiers::NONE,
    )));
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('l'),
        KeyModifiers::NONE,
    )));
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)));
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('h'),
        KeyModifiers::NONE,
    )));
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('l'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.focus_pane, wp_tui::app::actions::FocusPane::ChatList);
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));

    assert_eq!(
        calls.borrow().as_slice(),
        &[(
            "chat@example.test".into(),
            "author@example.test".into(),
            "owned".into(),
            "❤️".into()
        )]
    );
    assert_eq!(app.action_notice, Some(ActionNotice::Reacted));
}

#[test]
fn reaction_missing_selection_cancel_and_failure_are_non_mutating() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut missing = TestApp::new();
    missing.message_reactor = Box::new(FakeMessageReactor {
        calls: calls.clone(),
        result: Ok(()),
    });
    missing.dispatch_action(AppAction::ReactMessage);
    assert!(calls.borrow().is_empty());
    assert_eq!(
        missing.action_notice,
        Some(ActionNotice::Unavailable(
            "Reaction is not available".into()
        ))
    );

    let mut app = owned_text_app("react");
    app.message_reactor = Box::new(FakeMessageReactor {
        calls: calls.clone(),
        result: Err(whatsrust::MessageActionFailed),
    });
    app.dispatch_action(AppAction::ReactMessage);
    app.dispatch_action(AppAction::CancelReaction);
    assert!(calls.borrow().is_empty());
    assert_eq!(app.action_notice, Some(ActionNotice::Cancelled));
    app.dispatch_action(AppAction::ReactMessage);
    app.dispatch_action(AppAction::ConfirmReaction);
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable(
            "Could not react to message".into()
        ))
    );
}

#[test]
fn owned_ordinary_delete_calls_revoker_and_guards_invalid_selection() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = owned_text_app("delete");
    let directory = tempdir().unwrap();
    let path = directory.path().join("app.db");
    replace_database(&mut app, directory.path());
    app.db_handler
        .add_message(app.messages.get("owned").unwrap());
    app.message_revoker = Box::new(FakeMessageRevoker {
        calls: calls.clone(),
        result: Ok(()),
    });
    app.dispatch_action(AppAction::DeleteMessage);
    assert_eq!(
        calls.borrow().as_slice(),
        &[(
            "chat@example.test".into(),
            "chat@example.test".into(),
            "owned".into()
        )]
    );
    assert_eq!(app.action_notice, Some(ActionNotice::DeletedMessage));
    assert!(app.messages.contains_key("owned"));
    assert!(app.message_status(&"owned".into()).deleted);
    assert_eq!(app.message_actions["owned"].len(), 1);

    assert!(app.handle_whatsapp_event(whatsrust::Event::MessageAction {
        action_id: "server-delete".into(),
        target_message_id: "owned".into(),
        chat: whatsrust::JID("chat@example.test".into()),
        sender: whatsrust::JID("chat@example.test".into()),
        kind: whatsrust::MessageActionKind::Delete,
        occurred_at: 2,
        arrival_order: 2,
    }));
    assert_eq!(app.message_actions["owned"].len(), 1);
    app.db_handler.stop();

    let mut reloaded = TestApp::with_database(&path);
    reloaded.load_data_from_db();
    assert!(reloaded.message_status(&"owned".into()).deleted);
    assert!(matches!(
        &reloaded.messages["owned"].message,
        whatsrust::MessageContent::Text(text) if text.as_ref() == "This message was deleted."
    ));
    reloaded.db_handler.stop();

    let mut foreign = message("foreign", None, "not mine");
    foreign.info.sender = whatsrust::JID("other@example.test".into());
    app.messages.insert(foreign.info.id.clone(), foreign);
    app.message_list_state
        .set_selected_message("foreign".into());
    app.dispatch_action(AppAction::DeleteMessage);
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unauthorized(
            "Only your messages can be changed".into()
        ))
    );
    // Join the writer thread while `directory`/app.db still exist (the outer
    // tempdir is dropped before the app).
    app.db_handler.stop();
}

#[test]
fn deleted_message_blocks_copy_open_view_download_forward_reply_edit_delete_and_menu() {
    let directory = tempdir().unwrap();
    let mut app = owned_text_app("secret body");
    replace_database(&mut app, directory.path());
    let selected = app.messages["owned"].clone();
    app.apply_message_action(MessageAction {
        action_id: "delete".into(),
        target_message_id: "owned".into(),
        chat: selected.info.chat.clone(),
        sender: selected.info.sender.clone(),
        kind: wp_tui::app::MessageActionKind::Delete,
        occurred_at: 2,
        arrival_order: 1,
    });
    assert!(app.message_status(&"owned".into()).deleted);
    assert!(matches!(
        &app.messages["owned"].message,
        whatsrust::MessageContent::Text(body)
            if body.as_ref() == "This message was deleted."
    ));

    let revoker_calls = Rc::new(RefCell::new(Vec::new()));
    let editor_calls = Rc::new(RefCell::new(Vec::new()));
    let forwarder_calls = Rc::new(RefCell::new(Vec::new()));
    app.message_revoker = Box::new(FakeMessageRevoker {
        calls: revoker_calls.clone(),
        result: Ok(()),
    });
    app.message_editor = Box::new(FakeMessageEditor {
        calls: editor_calls.clone(),
        result: Ok(()),
    });
    app.message_forwarder = Box::new(RecordingForwarder {
        calls: forwarder_calls.clone(),
        report: Default::default(),
    });
    app.clipboard_writer = Box::new(RecordingClipboard::default());

    for action in [
        AppAction::CopyMessage,
        AppAction::OpenMessage,
        AppAction::DownloadMessage,
        AppAction::ViewMessage,
        AppAction::ShareMessage,
        AppAction::ReplyMessage,
        AppAction::EditMessage,
        AppAction::DeleteMessage,
        AppAction::OpenMessageMenu,
    ] {
        app.dispatch_action(action.clone());
        assert_eq!(
            app.action_notice,
            Some(ActionNotice::Unavailable(
                "This message was deleted.".into()
            )),
            "action {action:?} must be blocked on a deleted message"
        );
    }

    assert!(
        revoker_calls.borrow().is_empty(),
        "revoke must not be called"
    );
    assert!(editor_calls.borrow().is_empty(), "edit must not be called");
    assert!(
        forwarder_calls.borrow().is_empty(),
        "forward must not be called"
    );
    assert!(
        app.clipboard_writer.written_text().is_none(),
        "deleted body must never reach the clipboard"
    );
    assert!(
        app.composer.quote.is_none(),
        "deleted body must not be quotable"
    );
    assert!(app.message_menu.is_none(), "menu must not open");
    assert!(app.attachment_viewer.is_none(), "viewer must not open");
    assert!(app.share_picker.is_none(), "share picker must not open");
    assert!(
        !app.message_status(&"owned".into()).edited,
        "delete must win over any earlier edit status"
    );
    app.db_handler.stop();
}

#[derive(Default)]
struct RecordingForwarder {
    calls: Rc<RefCell<Vec<(String, Vec<String>, bool)>>>,
    report: whatsrust::ForwardReport,
}
impl MessageForwarder for RecordingForwarder {
    fn forward_message(
        &self,
        source: &whatsrust::Message,
        destinations: &[whatsrust::JID],
    ) -> whatsrust::ForwardReport {
        self.calls.borrow_mut().push((
            source.info.id.to_string(),
            destinations.iter().map(|jid| jid.0.to_string()).collect(),
            matches!(source.message, whatsrust::MessageContent::File(_)),
        ));
        self.report
    }
}

#[test]
fn failed_owned_delete_does_not_record_a_local_action() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut app = owned_text_app("delete");
    let directory = tempdir().unwrap();
    replace_database(&mut app, directory.path());
    app.message_revoker = Box::new(FakeMessageRevoker {
        calls: calls.clone(),
        result: Err(whatsrust::MessageActionFailed),
    });

    app.dispatch_action(AppAction::DeleteMessage);

    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable("Could not delete message".into()))
    );
    assert!(app.message_actions.get("owned").is_none());
    assert!(!app.message_status(&"owned".into()).deleted);
    // Join the writer thread while `directory`/app.db still exist (the outer
    // tempdir is dropped before the app).
    app.db_handler.stop();
}

#[test]
fn reply_action_enters_composer_mode_with_the_selected_message() {
    let mut app = TestApp::new();
    let selected = message("selected", None, "reply to this");
    app.messages.insert(selected.info.id.clone(), selected);
    app.message_list_state
        .set_selected_message("selected".into());

    app.dispatch_action(AppAction::ReplyMessage);

    assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
    assert_eq!(
        app.composer
            .quote
            .as_ref()
            .map(|quote| quote.info.id.as_ref()),
        Some("selected")
    );
}

#[test]
fn copy_and_unavailable_actions_report_outcomes_without_mutation() {
    let mut app = TestApp::new();
    let selected = message("selected", None, "copy this");
    app.messages.insert(selected.info.id.clone(), selected);
    app.message_list_state
        .set_selected_message("selected".into());
    app.clipboard_writer = Box::new(RecordingClipboard::default());

    app.dispatch_action(AppAction::CopyMessage);
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::CopiedText("copy this".into()))
    );

    app.dispatch_action(AppAction::DeleteMessage);
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unauthorized(
            "Only your messages can be changed".into()
        ))
    );
    assert!(app.messages.contains_key("selected"));
    assert!(app.composer.quote.is_none());
}

#[test]
fn copy_writes_text_through_the_injected_clipboard_and_reports_failures() {
    let mut app = owned_text_app("copy this");
    app.clipboard_writer = Box::new(RecordingClipboard::default());

    app.dispatch_action(AppAction::CopyMessage);

    assert_eq!(
        app.action_notice,
        Some(ActionNotice::CopiedText("copy this".into()))
    );
    assert_eq!(app.clipboard_writer.written_text(), Some("copy this"));
}

#[test]
fn copy_writer_failure_reports_a_visible_notice_without_reporting_success() {
    let mut app = owned_text_app("copy this");
    app.clipboard_writer = Box::new(FailingClipboard);

    app.dispatch_action(AppAction::CopyMessage);

    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable("Could not copy message".into()))
    );
}

#[test]
fn portable_menu_filters_discord_actions_and_cancellation_preserves_state() {
    let mut app = TestApp::new();
    let selected = message("selected", Some("missing"), "menu text");
    app.messages.insert(selected.info.id.clone(), selected);
    app.message_list_state
        .set_selected_message("selected".into());

    app.dispatch_action(AppAction::OpenMessageMenu);
    assert_eq!(
        app.message_menu_actions(),
        Some(vec![
            MessageMenuAction::CopyText,
            MessageMenuAction::Reply,
            MessageMenuAction::GoToReference,
        ])
    );
    app.dispatch_action(AppAction::MenuNext);
    app.dispatch_action(AppAction::ConfirmMessageMenu);
    assert_eq!(
        app.composer
            .quote
            .as_ref()
            .map(|quote| quote.info.id.as_ref()),
        Some("selected")
    );
    app.dispatch_action(AppAction::OpenMessageMenu);
    app.dispatch_action(AppAction::CancelMessageMenu);

    assert_eq!(app.action_notice, Some(ActionNotice::Cancelled));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("selected")
    );
    assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
}

#[test]
fn portable_menu_exposes_local_sender_and_reacted_user_details_only_when_available() {
    let mut app = owned_text_app("menu text");
    let sender = app.messages["owned"].info.sender.clone();
    app.contacts.insert(sender.clone(), "Alice".into());
    app.reactions.insert(
        "owned".into(),
        [("bob@example.test".to_owned().into(), "👍".into())].into(),
    );

    app.dispatch_action(AppAction::OpenMessageMenu);

    assert_eq!(
        app.message_menu_actions(),
        Some(vec![
            MessageMenuAction::CopyText,
            MessageMenuAction::Reply,
            MessageMenuAction::SenderDetails,
            MessageMenuAction::ReactedUsers,
        ])
    );
}

#[test]
fn portable_menu_confirms_sender_and_reacted_user_details() {
    let mut app = owned_text_app("menu text");
    let sender = app.messages["owned"].info.sender.clone();
    app.contacts.insert(sender.clone(), "Alice".into());
    app.contacts
        .insert("bob@example.test".to_owned().into(), "Bob".into());
    app.reactions.insert(
        "owned".into(),
        [("bob@example.test".to_owned().into(), "👍".into())].into(),
    );

    app.dispatch_action(AppAction::OpenMessageMenu);
    app.dispatch_action(AppAction::MenuNext);
    app.dispatch_action(AppAction::MenuNext);
    app.dispatch_action(AppAction::ConfirmMessageMenu);
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::SenderDetails("Alice".into()))
    );

    app.dispatch_action(AppAction::OpenMessageMenu);
    app.dispatch_action(AppAction::MenuNext);
    app.dispatch_action(AppAction::MenuNext);
    app.dispatch_action(AppAction::MenuNext);
    app.dispatch_action(AppAction::ConfirmMessageMenu);
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::ReactedUsers(vec!["Bob".into()]))
    );
}

fn message(id: &str, quote_id: Option<&str>, text: &str) -> whatsrust::Message {
    let jid = whatsrust::JID("chat@example.test".into());
    whatsrust::Message {
        info: whatsrust::MessageInfo {
            id: id.into(),
            chat: jid.clone(),
            sender: jid,
            mentions_self: false,
            timestamp: 0,
            is_from_me: false,
            quote_id: quote_id.map(Into::into),
            read_by: 0,
            forwarding: Default::default(),
        },
        message: whatsrust::MessageContent::Text(text.into()),
    }
}

fn owned_text_app(text: &str) -> TestApp {
    let mut app = TestApp::new();
    let selected = message("owned", None, text);
    let mut selected = selected;
    selected.info.is_from_me = true;
    app.messages.insert(selected.info.id.clone(), selected);
    app.message_list_state.set_selected_message("owned".into());
    app
}

fn replace_database(app: &mut App<'_>, path: &std::path::Path) {
    std::mem::replace(
        &mut app.db_handler,
        DatabaseHandler::new(&path.join("app.db")),
    )
    .stop();
    app.db_handler.init();
}
