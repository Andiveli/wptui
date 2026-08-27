use crate::input_key::KeyCode;

use crate::app::actions::AppAction;
use crate::input_key::Key;

pub(crate) fn attachment_viewer_action(key: &Key) -> Option<AppAction> {
    Some(match key.code {
        KeyCode::Char('h') | KeyCode::Left => AppAction::ViewerPrevious,
        KeyCode::Char('l') | KeyCode::Right => AppAction::ViewerNext,
        KeyCode::Char('-') => AppAction::ViewerZoomOut,
        KeyCode::Char('=') => AppAction::ViewerZoomIn,
        KeyCode::Char('x') => AppAction::ViewerOpenExternal,
        KeyCode::Esc | KeyCode::Char('q') => AppAction::CloseAttachmentViewer,
        _ => return None,
    })
}

pub(crate) fn url_picker_action(key: &Key) -> Option<AppAction> {
    Some(match key.code {
        KeyCode::Char('j') | KeyCode::Down => AppAction::UrlPickerNext,
        KeyCode::Char('k') | KeyCode::Up => AppAction::UrlPickerPrevious,
        KeyCode::Enter => AppAction::ConfirmUrlPicker,
        KeyCode::Esc => AppAction::CancelUrlPicker,
        _ => return None,
    })
}

pub(crate) fn file_picker_search_action(key: &Key) -> Option<AppAction> {
    Some(match key.code {
        KeyCode::Backspace => AppAction::FilePickerBackspace,
        KeyCode::Esc => AppAction::FilePickerEndSearch,
        KeyCode::Char(character) => AppAction::FilePickerCharacter(character),
        KeyCode::Enter => AppAction::FilePickerConfirm,
        _ => return None,
    })
}

pub(crate) fn file_picker_navigation_action(key: &Key) -> Option<AppAction> {
    Some(match key.code {
        KeyCode::Char('j') | KeyCode::Down => AppAction::FilePickerNext,
        KeyCode::Char('k') | KeyCode::Up => AppAction::FilePickerPrevious,
        KeyCode::Char('h') | KeyCode::Left => AppAction::FilePickerParent,
        KeyCode::Char('l') | KeyCode::Right => AppAction::FilePickerDescend,
        KeyCode::Char(' ') => AppAction::FilePickerToggle,
        KeyCode::Char('/') => AppAction::FilePickerEnterSearch,
        KeyCode::Enter => AppAction::FilePickerConfirm,
        KeyCode::Backspace => AppAction::FilePickerParent,
        KeyCode::Esc => AppAction::CancelFilePicker,
        _ => return None,
    })
}

pub(crate) fn share_picker_action(key: &Key) -> Option<AppAction> {
    Some(match key.code {
        KeyCode::Char('j') | KeyCode::Down => AppAction::SharePickerNext,
        KeyCode::Char('k') | KeyCode::Up => AppAction::SharePickerPrevious,
        KeyCode::Char(' ') => AppAction::ToggleShareRecipient,
        KeyCode::Enter => AppAction::ConfirmShare,
        KeyCode::Esc => AppAction::CancelShare,
        KeyCode::Backspace => AppAction::ShareSearchBackspace,
        KeyCode::Char(character) => AppAction::ShareSearchCharacter(character),
        _ => return None,
    })
}

pub(crate) fn reaction_picker_action(key: &Key) -> Option<AppAction> {
    Some(match key.code {
        KeyCode::Char('h') | KeyCode::Left => AppAction::ReactionPrev,
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Down => AppAction::ReactionNext,
        KeyCode::Enter => AppAction::ConfirmReaction,
        KeyCode::Esc => AppAction::CancelReaction,
        _ => return None,
    })
}

pub(crate) fn message_menu_action(key: &Key) -> Option<AppAction> {
    Some(match key.code {
        KeyCode::Char('j') | KeyCode::Down => AppAction::MenuNext,
        KeyCode::Char('k') | KeyCode::Up => AppAction::MenuPrevious,
        KeyCode::Enter => AppAction::ConfirmMessageMenu,
        KeyCode::Esc => AppAction::CancelMessageMenu,
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
