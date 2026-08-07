//! Integration tests for the embedded file picker wiring in the App:
//! opening, cancel, and confirming an attachment through `dispatch_action`,
//! which must end up in `Composer::pending` via `queue_attachment`.

use std::fs;

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tempfile::tempdir;
use wp_tui::app::actions::{AppAction, ConversationMode};
use wp_tui::file_picker::FilePickerState;

mod common;
use common::TestApp;

fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

fn tree_dir() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(root.join("docs/report.pdf"), b"x").unwrap();
    fs::write(root.join("notes.txt"), b"x").unwrap();
    fs::write(root.join("pic.png"), b"x").unwrap();
    dir
}

fn picker_for(dir: &std::path::Path) -> FilePickerState {
    FilePickerState::open(dir).unwrap()
}

#[test]
fn confirm_forwards_attachment_and_closes_picker_by_path_and_kind() {
    let mut app = TestApp::new();
    let dir = tree_dir();
    // Root listing: `docs` (dir) is the first entry, so it is selected by
    // default. Confirm it to descend.
    app.file_picker = Some(picker_for(dir.path()));

    app.dispatch_action(AppAction::FilePickerDescend); // enter 'docs'
    assert!(app.file_picker.is_some(), "picking a dir keeps picker open");
    assert_ne!(
        app.conversation_mode,
        ConversationMode::ComposerEditing,
        "descending must not steal focus into the composer"
    );

    // Inside docs there is only report.pdf, which is selected by default.
    let before = app.composer.pending.len();
    app.dispatch_action(AppAction::FilePickerConfirm);
    assert!(
        app.file_picker.is_none(),
        "picker closes after a file attach"
    );
    assert_eq!(
        app.conversation_mode,
        ConversationMode::ComposerEditing,
        "confirming auto-focuses the composer"
    );
    assert_eq!(
        app.focus_pane,
        wp_tui::app::actions::FocusPane::Conversation,
        "confirming focuses the conversation pane"
    );
    assert_eq!(
        app.composer.pending.len(),
        before + 1,
        "attachment must be queued"
    );
    let attachment = app.composer.pending.last().unwrap();
    assert!(
        attachment.path.as_ref().ends_with("report.pdf"),
        "queued path should be the confirmed file, got {}",
        attachment.path
    );
    assert_eq!(
        attachment.kind.clone() as u8,
        whatsrust::FileKind::Document as u8,
        "pdf maps to Document"
    );
}

#[test]
fn ctrl_o_opens_the_picker_from_composer_editing() {
    let mut app = TestApp::new();
    // The composer editing branch runs before the pickers in the key handle,
    // so verifying Ctrl+O routes to AttachFile needs the composer active and
    // the conversation pane focused.
    app.conversation_mode = ConversationMode::ComposerEditing;
    app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;

    app.on_terminal_event(key(KeyCode::Char('o'), KeyModifiers::CONTROL));

    // Opening the picker must not depend on a live chat but on env; it opens
    // in a real directory, so it always becomes Some when the platform reports
    // a readable home/current dir.
    let opened = app.file_picker.is_some();
    assert!(
        opened || app.action_notice.is_some(),
        "Ctrl+O must either open the picker or surface an unavailable notice"
    );
}

#[test]
fn multi_selection_commits_every_toggled_file() {
    let mut app = TestApp::new();
    let dir = tree_dir();
    app.file_picker = Some(picker_for(dir.path()));

    // Root listing: docs (dir), notes.txt, pic.png. Toggle notes.txt and
    // pic.png with Space, then confirm and check both are queued.
    let pic = app
        .file_picker
        .as_ref()
        .unwrap()
        .visible_entries()
        .iter()
        .position(|entry| entry.name == "pic.png")
        .unwrap();
    for _ in 0..pic {
        app.dispatch_action(AppAction::FilePickerNext);
    }
    app.dispatch_action(AppAction::FilePickerToggle); // pic.png
    // Move back up to notes.txt.
    app.dispatch_action(AppAction::FilePickerPrevious); // to notes.txt
    app.dispatch_action(AppAction::FilePickerToggle); // notes.txt

    assert_eq!(
        app.file_picker.as_ref().unwrap().selected_count(),
        2,
        "both files must be toggled"
    );
    let queued = app.composer.pending.len();
    app.dispatch_action(AppAction::FilePickerConfirm);
    assert!(app.file_picker.is_none());
    assert_eq!(app.composer.pending.len(), queued + 2, "both files queued");
}

#[test]
fn cancel_closes_picker_without_touching_draft() {
    let mut app = TestApp::new();
    let dir = tree_dir();
    app.composer.insert_text("hello");
    app.file_picker = Some(picker_for(dir.path()));

    app.dispatch_action(AppAction::CancelFilePicker);

    assert!(app.file_picker.is_none());
    assert!(
        app.composer.pending.is_empty(),
        "cancel must not queue files"
    );
    assert_eq!(app.composer.text(), "hello", "draft text is untouched");
}

#[test]
fn opening_picker_then_navigating_up_and_confirming_png_maps_to_image_kind() {
    let mut app = TestApp::new();
    let dir = tree_dir();
    app.file_picker = Some(picker_for(dir.path()));

    // Pick image.png (icon at the root listing, after navigating up from docs).
    // Simpler deterministic path: start directly at root, enter docs, come back.
    app.dispatch_action(AppAction::FilePickerDescend); // enter docs (first entry)
    assert!(app.file_picker.is_some());
    app.dispatch_action(AppAction::FilePickerParent); // back to root
    assert!(app.file_picker.is_some());

    // Root: docs, notes.txt, pic.png → select pic.png (last visible).
    let roots = app.file_picker.as_ref().unwrap().visible_entries();
    let pic_index = roots
        .iter()
        .position(|entry| entry.name == "pic.png")
        .unwrap();
    for _ in 0..pic_index {
        app.dispatch_action(AppAction::FilePickerNext);
    }
    app.dispatch_action(AppAction::FilePickerConfirm);

    let attachment = app.composer.pending.last().unwrap();
    assert_eq!(
        attachment.kind.clone() as u8,
        whatsrust::FileKind::Image as u8,
        "png maps to Image"
    );
}
