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
