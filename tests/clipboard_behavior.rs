use std::fs;

use ratatui::crossterm::event::{
    Event, KeyCode as TerminalKeyCode, KeyEvent, KeyModifiers as TerminalKeyModifiers,
};
use tempfile::tempdir;
use wp_tui::app::actions::{ActionNotice, ConversationMode};
use wp_tui::app::composer::{Composer, ComposerOutcome};
use wp_tui::app::{
    actions::ComposerAction,
    inputs::{apply_clipboard_paste, composer_action_for_editing_key},
};
use wp_tui::clipboard::{ClipboardError, ClipboardPaste, classify_text, encode_rgba_png};
use wp_tui::input_key::{Key, KeyCode, KeyModifiers};
mod common;
use common::TestApp;

struct FailingClipboardReader;

fn terminal_event(key: Key) -> Event {
    let code = match key.code {
        KeyCode::Backspace => TerminalKeyCode::Backspace,
        KeyCode::Enter => TerminalKeyCode::Enter,
        KeyCode::Esc => TerminalKeyCode::Esc,
        KeyCode::Left => TerminalKeyCode::Left,
        KeyCode::Right => TerminalKeyCode::Right,
        KeyCode::Up => TerminalKeyCode::Up,
        KeyCode::Down => TerminalKeyCode::Down,
        KeyCode::Tab => TerminalKeyCode::Tab,
        KeyCode::Char(character) => TerminalKeyCode::Char(character),
    };
    let mut modifiers = TerminalKeyModifiers::NONE;
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        modifiers |= TerminalKeyModifiers::SHIFT;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        modifiers |= TerminalKeyModifiers::CONTROL;
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        modifiers |= TerminalKeyModifiers::ALT;
    }
    Event::Key(KeyEvent::new(code, modifiers))
}

impl wp_tui::app::actions::ClipboardReader for FailingClipboardReader {
    fn read_paste(&mut self) -> Result<ClipboardPaste, ClipboardError> {
        Err(ClipboardError::ClipboardUnavailable)
    }
}

struct TextClipboardReader(&'static str);

impl wp_tui::app::actions::ClipboardReader for TextClipboardReader {
    fn read_paste(&mut self) -> Result<ClipboardPaste, ClipboardError> {
        classify_text(self.0)
    }

    fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
        panic!("Ctrl+V must use the classified clipboard paste")
    }
}

struct NoClipboardAccess;

impl wp_tui::app::actions::ClipboardReader for NoClipboardAccess {
    fn read_paste(&mut self) -> Result<ClipboardPaste, ClipboardError> {
        panic!("Terminal paste must use its event payload, not the system clipboard")
    }
}

struct AttachmentClipboardReader;

impl wp_tui::app::actions::ClipboardReader for AttachmentClipboardReader {
    fn read_paste(&mut self) -> Result<ClipboardPaste, ClipboardError> {
        Ok(ClipboardPaste::Paths(vec!["photo.png".into()]))
    }

    fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
        panic!("Ctrl+V must use the existing classified clipboard paste")
    }
}

#[test]
fn existing_paths_take_precedence_over_plain_text() {
    let directory = tempdir().unwrap();
    let first = directory.path().join("first.png");
    let second = directory.path().join("second.pdf");
    fs::write(&first, []).unwrap();
    fs::write(&second, []).unwrap();

    let paste = classify_text(&format!("{}\n{}", first.display(), second.display())).unwrap();

    assert_eq!(paste, ClipboardPaste::Paths(vec![first, second]));
}

#[test]
fn missing_path_is_recoverable_instead_of_becoming_composer_text() {
    let missing = tempdir().unwrap().path().join("missing.png");

    assert_eq!(
        classify_text(&missing.display().to_string()),
        Err(ClipboardError::MissingPath(missing))
    );
}

#[test]
fn empty_text_is_a_recoverable_clipboard_error() {
    assert_eq!(classify_text("  \n\t"), Err(ClipboardError::EmptyText));
}

#[test]
fn rgba_clipboard_data_encodes_to_a_nonempty_png() {
    let png = encode_rgba_png(1, 1, &[0x10, 0x20, 0x30, 0xff]).unwrap();

    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert!(png.len() > 8);
}

#[test]
fn clipboard_failure_preserves_the_composer_text_and_pending_attachments() {
    let mut composer = Composer::default();
    composer.insert_text("draft message");
    composer.queue_attachment("existing.pdf".into(), whatsrust::FileKind::Document);

    let result = apply_clipboard_paste(
        &mut composer,
        tempdir().unwrap().path(),
        Err(ClipboardError::EmptyText),
    );

    assert_eq!(result, Err(ClipboardError::EmptyText));
    assert_eq!(composer.text(), "draft message");
    assert_eq!(composer.pending.len(), 1);
}

#[test]
fn pasted_existing_paths_are_queued_as_attachments() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("image.png");
    fs::write(&path, []).unwrap();
    let mut composer = Composer::default();

    apply_clipboard_paste(
        &mut composer,
        directory.path(),
        Ok(ClipboardPaste::Paths(vec![path.clone()])),
    )
    .unwrap();

    assert_eq!(composer.pending.len(), 1);
    assert_eq!(composer.pending[0].path.as_ref(), path.to_string_lossy());
    assert!(matches!(
        composer.pending[0].kind,
        whatsrust::FileKind::Image
    ));
}

#[test]
fn control_v_dispatches_the_semantic_paste_action_while_editing() {
    let action = composer_action_for_editing_key(&Key {
        code: KeyCode::Char('v'),
        modifiers: KeyModifiers::CONTROL,
    });

    assert_eq!(action, ComposerAction::Paste);
}

#[test]
fn control_v_preserves_complete_text_through_composer_submission() {
    for (name, pasted) in [
        ("single line", "message"),
        ("multiple lines", "uno\ndos\ntres"),
        (
            "markdown with blank lines and a combining mark",
            "# Heading\n\n*uno*\n_dos_\n~tres~\n\n```rust\nlet n = 1;\n```\n*a*\u{301} tail",
        ),
    ] {
        let mut app = TestApp::new();
        app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;
        app.conversation_mode = ConversationMode::ComposerEditing;
        app.composer.insert_text("draft ");
        app.clipboard_reader = Box::new(TextClipboardReader(pasted));
        let expected = format!("draft {pasted}");

        app.on_terminal_event(terminal_event(Key::ctrl('v')));

        assert_eq!(app.composer.text(), expected, "{name}: pasted source");
        assert_eq!(
            app.conversation_mode,
            ConversationMode::ComposerEditing,
            "{name}: paste must not submit the draft"
        );
        assert_eq!(app.action_notice, None, "{name}: no clipboard error");
        assert!(app.composer.pending.is_empty(), "{name}: no attachments");

        // Inspect the public submit result without dispatching a real send.
        let outcome = app.composer.apply(ComposerAction::Submit);

        assert_eq!(outcome.text_messages(), [expected.as_str()], "{name}");
        assert!(outcome.file_messages().is_empty(), "{name}: text only");
        let ComposerOutcome::Submit { display_text, .. } = outcome else {
            panic!("{name}: expected a submitted text payload");
        };
        assert_eq!(display_text.as_ref(), expected, "{name}: display source");
        assert!(app.composer.text().is_empty(), "{name}: submitted draft");
    }
}

#[test]
fn terminal_paste_preserves_all_lines_until_explicit_submission() {
    let mut app = TestApp::new();
    app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;
    app.conversation_mode = ConversationMode::ComposerEditing;
    app.clipboard_reader = Box::new(NoClipboardAccess);
    let pasted = "uno\ndos\ntres";

    app.on_terminal_event(Event::Paste(pasted.into()));

    assert_eq!(
        app.composer.text(),
        pasted,
        "complete terminal paste source"
    );
    assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
    assert!(app.composer.pending.is_empty());
    assert_eq!(app.action_notice, None);

    let outcome = app.composer.apply(ComposerAction::Submit);

    assert_eq!(outcome.text_messages(), [pasted]);
    assert!(outcome.file_messages().is_empty());
    let ComposerOutcome::Submit { display_text, .. } = outcome else {
        panic!("Expected one explicitly submitted multiline message");
    };
    assert_eq!(display_text.as_ref(), pasted);
    assert!(app.composer.text().is_empty());
}

#[test]
fn terminal_paste_combined_screenshot_fixture_submits_unchanged_markdown_source() {
    let source = concat!(
        "# Encabezado 1\n## Encabezado 2\n### Encabezado 3\n\n",
        "> Una cita de prueba\n\n",
        "[Ejemplo](https://example.com)\n\n",
        "[enlace sin destino]\n",
        "![imagen no soportada](https://example.com/image.png)"
    );
    let mut app = TestApp::new();
    app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;
    app.conversation_mode = ConversationMode::ComposerEditing;
    app.clipboard_reader = Box::new(NoClipboardAccess);

    app.on_terminal_event(Event::Paste(source.into()));

    assert_eq!(app.composer.text(), source);
    assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
    assert!(app.composer.pending.is_empty());
    assert_eq!(app.action_notice, None);

    // Only obtain the payload: never dispatch an outgoing network message.
    let outcome = app.composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.text_messages(), [source]);
    assert!(outcome.file_messages().is_empty());
    let ComposerOutcome::Submit { display_text, .. } = outcome else {
        panic!("Expected the exact combined screenshot text payload");
    };
    assert_eq!(display_text.as_ref(), source);
    assert!(app.composer.text().is_empty());
}

#[test]
fn terminal_paste_rejection_preserves_draft_attachments_and_mode() {
    use wp_tui::app::actions::{FocusPane, Section, VimComposerMode};

    for guard in [
        "focus",
        "section",
        "navigation",
        "vim normal",
        "logout",
        "logging out",
        "blocked",
    ] {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.conversation_mode = ConversationMode::ComposerEditing;
        app.clipboard_reader = Box::new(NoClipboardAccess);
        app.composer.insert_text("draft");
        app.composer
            .queue_attachment("existing.pdf".into(), whatsrust::FileKind::Document);
        match guard {
            "focus" => app.focus_pane = FocusPane::ChatList,
            "section" => app.selected_section = Section::Status,
            "navigation" => app.conversation_mode = ConversationMode::MessageNavigation,
            "vim normal" => app.vim_composer_mode = Some(VimComposerMode::Normal),
            "logout" => app.pending_logout = true,
            "logging out" => app.logout_in_progress = true,
            "blocked" => {
                let group = whatsrust::JID("123@g.us".into());
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
            }
            _ => unreachable!(),
        }
        let text = app.composer.text();
        let pending = app.composer.pending.len();
        let mode = app.conversation_mode;

        app.on_terminal_event(Event::Paste("uno\ndos\ntres".into()));

        assert_eq!(app.composer.text(), text, "{guard}");
        assert_eq!(app.composer.pending.len(), pending, "{guard}");
        if pending != 0 {
            assert_eq!(app.composer.pending[0].path.as_ref(), "existing.pdf");
        }
        assert_eq!(app.conversation_mode, mode, "{guard}");
        assert_eq!(app.action_notice, None, "{guard}");
        assert!(!app.should_quit, "{guard}");
    }
}

#[test]
fn terminal_paste_inserts_at_cursor_in_message_editing_and_vim_insert() {
    use wp_tui::app::actions::{FocusPane, VimComposerMode};

    for mode in [
        ConversationMode::EditingMessage,
        ConversationMode::ComposerEditing,
    ] {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.conversation_mode = mode;
        app.vim_composer_mode = Some(VimComposerMode::Insert);
        app.clipboard_reader = Box::new(NoClipboardAccess);
        app.composer.insert_text("ab");
        app.on_terminal_event(terminal_event(Key::k(KeyCode::Left)));

        app.on_terminal_event(Event::Paste("uno\ndos\ntres".into()));

        assert_eq!(app.composer.text(), "auno\ndos\ntresb");
        assert_eq!(app.conversation_mode, mode);
        assert!(app.composer.pending.is_empty());
        assert_eq!(app.action_notice, None);
    }
}

#[test]
fn control_v_still_queues_file_paths_as_attachments_in_composer() {
    let mut app = TestApp::new();
    app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;
    app.conversation_mode = ConversationMode::ComposerEditing;
    app.composer.insert_text("caption ");
    app.clipboard_reader = Box::new(AttachmentClipboardReader);

    app.on_terminal_event(terminal_event(Key::ctrl('v')));

    assert_eq!(app.composer.text(), "caption ");
    assert_eq!(app.composer.pending.len(), 1);
    assert_eq!(app.composer.pending[0].path.as_ref(), "photo.png");
    assert!(matches!(
        app.composer.pending[0].kind,
        whatsrust::FileKind::Image
    ));
    assert_eq!(app.action_notice, None);
}

#[test]
fn control_v_clipboard_read_failure_shows_notice_and_preserves_composer_state() {
    let mut app = TestApp::new();
    app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;
    app.conversation_mode = ConversationMode::ComposerEditing;
    app.composer.insert_text("draft message");
    app.composer
        .queue_attachment("existing.pdf".into(), whatsrust::FileKind::Document);
    app.clipboard_reader = Box::new(FailingClipboardReader);

    app.on_terminal_event(terminal_event(Key::ctrl('v')));

    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable(
            "Could not paste clipboard content: ClipboardUnavailable".into()
        ))
    );
    assert_eq!(app.composer.text(), "draft message");
    assert_eq!(app.composer.pending.len(), 1);
}
