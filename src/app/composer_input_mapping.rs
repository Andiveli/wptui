use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use crate::app::actions::ComposerAction;
use crate::key_handler::Key;

/// Maps a key pressed while editing the composer to its semantic action.
pub fn composer_action_for_editing_key(key: &Key) -> ComposerAction {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, KeyModifiers::NONE) => ComposerAction::Submit,
        (KeyCode::Char('v'), KeyModifiers::CONTROL) => ComposerAction::Paste,
        _ => ComposerAction::Edit(key.clone()),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    use super::*;

    #[test]
    fn maps_submit_and_paste_keys() {
        assert_eq!(
            composer_action_for_editing_key(&Key::k(KeyCode::Enter)),
            ComposerAction::Submit
        );
        assert_eq!(
            composer_action_for_editing_key(&Key::ctrl('v')),
            ComposerAction::Paste
        );
    }

    #[test]
    fn preserves_other_editing_keys() {
        let key = Key {
            code: KeyCode::Char('V'),
            modifiers: KeyModifiers::SHIFT,
        };

        assert_eq!(
            composer_action_for_editing_key(&key),
            ComposerAction::Edit(key)
        );
    }
}
