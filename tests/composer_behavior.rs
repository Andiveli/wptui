use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use wp_tui::app::actions::ComposerAction;
use wp_tui::app::actions::{AppAction, ConversationMode};
use wp_tui::app::composer::Composer;
use wp_tui::app::inputs::composer_action_for_editing_key;
use wp_tui::key_handler::Key;
use wp_tui::ui::render_chats;
mod common;
use common::TestApp;

#[test]
fn enter_maps_to_submit_while_ctrl_enter_falls_through_to_editing() {
    assert_eq!(
        composer_action_for_editing_key(&Key::k(KeyCode::Enter)),
        ComposerAction::Submit
    );
    assert_eq!(
        composer_action_for_editing_key(&Key {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::CONTROL,
        }),
        ComposerAction::Edit(Key {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::CONTROL,
        })
    );
}

#[test]
fn enter_submits_the_composer_text() {
    let mut composer = Composer::default();
    composer.insert_text("hello");

    assert_eq!(
        composer.apply(ComposerAction::Submit).text_messages(),
        vec!["hello"]
    );
    assert!(composer.is_empty());
}

#[test]
fn a_normal_space_remains_composer_input() {
    let mut composer = Composer::default();

    composer.apply(composer_action_for_editing_key(&Key::c(' ')));

    assert_eq!(composer.text(), " ");
}

#[test]
fn ctrl_enter_inserts_a_newline_without_submitting() {
    let mut composer = Composer::default();
    composer.insert_text("first");

    let outcome = composer.apply(ComposerAction::InsertNewline);

    assert!(outcome.is_idle());
    composer.insert_text("second");
    assert_eq!(
        composer.apply(ComposerAction::Submit).text_messages(),
        vec!["first\nsecond"]
    );
}

#[test]
fn queued_files_use_the_composer_caption_only_for_the_first_file() {
    let mut composer = Composer::default();
    composer.insert_text("caption");
    composer.queue_attachment("first.png".into(), whatsrust::FileKind::Image);
    composer.queue_attachment("second.pdf".into(), whatsrust::FileKind::Document);

    let outcome = composer.apply(ComposerAction::Submit);
    let messages = outcome.file_messages();

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].caption.as_deref(), Some("caption"));
    assert_eq!(messages[1].caption, None);
}

#[test]
fn blank_drafts_are_rejected_without_clearing_the_draft_or_quote() {
    let mut composer = Composer::default();
    composer.insert_text(" \n\t ");
    composer.quote = Some(message("quoted"));

    assert!(composer.apply(ComposerAction::Submit).is_idle());
    assert_eq!(composer.text(), " \n\t ");
    assert_eq!(
        composer.quote.as_ref().map(|quote| quote.info.id.as_ref()),
        Some("quoted")
    );
}

#[test]
fn empty_drafts_are_rejected_without_clearing_the_draft() {
    let mut composer = Composer::default();

    assert!(composer.apply(ComposerAction::Submit).is_idle());
    assert!(composer.text().is_empty());
}

#[test]
fn empty_and_blank_submits_are_silent_no_ops() {
    for draft in ["", " \n\t"] {
        let mut app = TestApp::new();
        app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;
        app.conversation_mode = ConversationMode::ComposerEditing;
        app.composer.insert_text(draft);

        app.dispatch_action(AppAction::Composer(ComposerAction::Submit));

        assert_eq!(app.action_notice, None, "draft: {draft:?}");
        assert_eq!(app.composer.text(), draft, "draft: {draft:?}");
    }
}

#[test]
fn escape_cancels_reply_and_attachment_modes_without_leaking_to_the_next_draft() {
    let mut app = TestApp::new();
    app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;
    app.conversation_mode = ConversationMode::ComposerEditing;
    app.composer.quote = Some(message("quoted"));
    app.composer
        .queue_attachment("image.png".into(), whatsrust::FileKind::Image);

    app.on_terminal_event(ratatui::crossterm::event::Event::Key(
        ratatui::crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
    ));

    assert!(app.composer.quote.is_none());
    assert!(app.composer.pending.is_empty());
    assert_eq!(app.conversation_mode, ConversationMode::MessageNavigation);
}

#[test]
fn ctrl_shift_l_toggles_logs_without_mutating_the_active_composer() {
    for character in ['L', 'l'] {
        let mut app = TestApp::new();
        app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;
        app.conversation_mode = ConversationMode::ComposerEditing;
        app.composer.insert_text("draft");
        app.kh.resolve(Key::c('g'));

        app.on_terminal_event(Event::Key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        )));

        assert!(app.show_logs, "character: {character:?}");
        assert_eq!(app.composer.text(), "draft", "character: {character:?}");
        assert_eq!(app.kh.buffered_keys(), &[Key::c('g')]);
    }
}

#[test]
fn attachment_only_drafts_are_sendable() {
    let mut composer = Composer::default();
    composer.queue_attachment("image.png".into(), whatsrust::FileKind::Image);

    let outcome = composer.apply(ComposerAction::Submit);

    assert!(outcome.text_messages().is_empty());
    assert_eq!(outcome.file_messages().len(), 1);
}

#[test]
fn blocked_composer_rejects_text_attachments_and_submission() {
    let mut composer = Composer::default();
    composer.set_blocked(true);

    composer.insert_text("blocked");
    composer.queue_attachment("image.png".into(), whatsrust::FileKind::Image);

    assert!(composer.text().is_empty());
    assert!(composer.pending.is_empty());
    assert!(composer.apply(ComposerAction::Submit).is_idle());
}

#[test]
fn admin_only_group_blocks_input_actions_and_renders_message() {
    let mut app = TestApp::new();
    let group = whatsrust::JID("123@g.us".into());
    app.open_chat_by_jid(group.clone());
    app.group_permissions.insert(
        group,
        whatsrust::GroupInfo {
            jid: whatsrust::JID("123@g.us".into()),
            name: "Admins only".into(),
            is_announce: true,
            is_admin: false,
        },
    );
    let blocked = app.composer_blocked();
    app.composer.set_blocked(blocked);
    app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;

    app.dispatch_action(AppAction::InsertMode);
    app.dispatch_action(AppAction::Composer(ComposerAction::Edit(Key::c('x'))));
    app.dispatch_action(AppAction::AttachFile);

    assert_eq!(app.conversation_mode, ConversationMode::MessageNavigation);
    assert!(app.composer.text().is_empty());
    assert!(app.composer.pending.is_empty());

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
    assert!(rendered.contains("Only group admins can send messages in this group."));
    assert!(!rendered.contains("Message input"));
}

fn message(id: &str) -> whatsrust::Message {
    let jid = whatsrust::JID("chat@example.test".into());
    whatsrust::Message {
        info: whatsrust::MessageInfo {
            id: id.into(),
            chat: jid.clone(),
            sender: jid,
            timestamp: 0,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: whatsrust::MessageContent::Text("quoted message".into()),
    }
}
