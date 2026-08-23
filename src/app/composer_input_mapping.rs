use crate::input_key::{KeyCode, KeyModifiers};
use ratatui_textarea::Input;

use crate::app::actions::ComposerAction;
use crate::input_key::Key;

/// Maps a key pressed while editing the composer to its semantic action.
pub fn composer_action_for_editing_key(key: &Key) -> ComposerAction {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, KeyModifiers::NONE) => ComposerAction::Submit,
        (KeyCode::Char('v'), KeyModifiers::CONTROL) => ComposerAction::Paste,
        _ => ComposerAction::Edit(key.clone()),
    }
}

/// Converts the neutral terminal key at the composer adapter boundary.
pub fn textarea_input(key: &Key) -> Input {
    Input {
        key: match key.code {
            KeyCode::Backspace => ratatui_textarea::Key::Backspace,
            KeyCode::Enter => ratatui_textarea::Key::Enter,
            KeyCode::Esc => ratatui_textarea::Key::Esc,
            KeyCode::Left => ratatui_textarea::Key::Left,
            KeyCode::Right => ratatui_textarea::Key::Right,
            KeyCode::Up => ratatui_textarea::Key::Up,
            KeyCode::Down => ratatui_textarea::Key::Down,
            KeyCode::Tab => ratatui_textarea::Key::Tab,
            KeyCode::Char(c) => ratatui_textarea::Key::Char(c),
        },
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
    }
}

#[cfg(test)]
mod tests {
    use crate::input_key::{KeyCode, KeyModifiers};

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
