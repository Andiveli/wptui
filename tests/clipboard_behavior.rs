use std::fs;

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tempfile::tempdir;
use wp_tui::app::actions::{ActionNotice, ConversationMode};
use wp_tui::app::composer::Composer;
use wp_tui::app::{
    actions::ComposerAction,
    inputs::{apply_clipboard_paste, composer_action_for_editing_key},
};
use wp_tui::clipboard::{ClipboardError, ClipboardPaste, classify_text, encode_rgba_png};
use wp_tui::key_handler::Key;
mod common;
use common::TestApp;

struct FailingClipboardReader;

impl wp_tui::app::actions::ClipboardReader for FailingClipboardReader {
    fn read_paste(&mut self) -> Result<ClipboardPaste, ClipboardError> {
        Err(ClipboardError::ClipboardUnavailable)
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
fn control_v_clipboard_read_failure_shows_notice_and_preserves_composer_state() {
    let mut app = TestApp::new();
    app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;
    app.conversation_mode = ConversationMode::ComposerEditing;
    app.composer.insert_text("draft message");
    app.composer
        .queue_attachment("existing.pdf".into(), whatsrust::FileKind::Document);
    app.clipboard_reader = Box::new(FailingClipboardReader);

    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('v'),
        KeyModifiers::CONTROL,
    )));

    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable(
            "Could not paste clipboard content: ClipboardUnavailable".into()
        ))
    );
    assert_eq!(app.composer.text(), "draft message");
    assert_eq!(app.composer.pending.len(), 1);
}
