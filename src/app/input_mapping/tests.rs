use super::*;
use crate::input_key::KeyCode;

fn key(code: KeyCode) -> Key {
    Key::k(code)
}

#[test]
fn maps_picker_navigation_and_controls() {
    assert_eq!(
        url_picker_action(&key(KeyCode::Down)),
        Some(AppAction::UrlPickerNext)
    );
    assert_eq!(
        file_picker_navigation_action(&key(KeyCode::Char('h'))),
        Some(AppAction::FilePickerParent)
    );
    assert_eq!(
        reaction_picker_action(&key(KeyCode::Enter)),
        Some(AppAction::ConfirmReaction)
    );
    assert_eq!(
        message_menu_action(&key(KeyCode::Esc)),
        Some(AppAction::CancelMessageMenu)
    );
}

#[test]
fn reaction_picker_uses_vertical_navigation() {
    assert_eq!(
        reaction_picker_action(&key(KeyCode::Char('j'))),
        Some(AppAction::ReactionNext)
    );
    assert_eq!(
        reaction_picker_action(&key(KeyCode::Down)),
        Some(AppAction::ReactionNext)
    );
    assert_eq!(
        reaction_picker_action(&key(KeyCode::Char('k'))),
        Some(AppAction::ReactionPrev)
    );
    assert_eq!(
        reaction_picker_action(&key(KeyCode::Up)),
        Some(AppAction::ReactionPrev)
    );

    for horizontal_key in [
        KeyCode::Char('h'),
        KeyCode::Left,
        KeyCode::Char('l'),
        KeyCode::Right,
    ] {
        assert_eq!(reaction_picker_action(&key(horizontal_key)), None);
    }
    assert_eq!(
        reaction_picker_action(&key(KeyCode::Enter)),
        Some(AppAction::ConfirmReaction)
    );
    assert_eq!(
        reaction_picker_action(&key(KeyCode::Esc)),
        Some(AppAction::CancelReaction)
    );
}

#[test]
fn maps_picker_search_and_viewer_keys() {
    assert_eq!(
        file_picker_search_action(&key(KeyCode::Char('x'))),
        Some(AppAction::FilePickerCharacter('x'))
    );
    assert_eq!(
        share_picker_action(&key(KeyCode::Backspace)),
        Some(AppAction::ShareSearchBackspace)
    );
    assert_eq!(
        attachment_viewer_action(&key(KeyCode::Char('l'))),
        Some(AppAction::ViewerNext)
    );
    assert_eq!(
        attachment_viewer_action(&key(KeyCode::Char('s'))),
        Some(AppAction::ViewerSaveAs)
    );
    assert_eq!(
        attachment_viewer_action(&key(KeyCode::Char('x'))),
        Some(AppAction::ViewerOpenExternal)
    );
    assert_eq!(
        attachment_viewer_action(&key(KeyCode::Esc)),
        Some(AppAction::CloseAttachmentViewer)
    );
}

#[test]
fn save_directory_and_filename_have_distinct_keys() {
    assert_eq!(viewer_save_action(&key(KeyCode::Down), false), Some(AppAction::ViewerSaveNext));
    assert_eq!(viewer_save_action(&key(KeyCode::Left), false), Some(AppAction::ViewerSaveParent));
    assert_eq!(viewer_save_action(&key(KeyCode::Right), false), Some(AppAction::ViewerSaveDescend));
    assert_eq!(viewer_save_action(&key(KeyCode::Enter), false), Some(AppAction::ViewerSaveEditName));
    assert_eq!(viewer_save_action(&key(KeyCode::Char('x')), false), None);
    assert_eq!(viewer_save_action(&key(KeyCode::Char('x')), true), Some(AppAction::ViewerSaveCharacter('x')));
    assert_eq!(viewer_save_action(&key(KeyCode::Enter), true), Some(AppAction::ViewerSaveConfirm));
    assert_eq!(viewer_save_action(&key(KeyCode::Esc), true), Some(AppAction::ViewerSaveCancel));
}

#[test]
fn ignores_keys_outside_each_picker_context() {
    let unknown = key(KeyCode::Tab);
    assert_eq!(attachment_viewer_action(&unknown), None);
    assert_eq!(url_picker_action(&unknown), None);
    assert_eq!(file_picker_search_action(&unknown), None);
    assert_eq!(file_picker_navigation_action(&unknown), None);
    assert_eq!(share_picker_action(&unknown), None);
    assert_eq!(reaction_picker_action(&unknown), None);
    assert_eq!(message_menu_action(&unknown), None);
}
