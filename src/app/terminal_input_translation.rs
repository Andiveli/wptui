use ratatui::crossterm::event::{Event, KeyCode as TerminalKeyCode, KeyEventKind, KeyModifiers};

use crate::input_key::{Key, KeyCode, KeyModifiers as InputKeyModifiers};

pub(crate) fn translate_terminal_event(event: &Event) -> Option<Key> {
    let Event::Key(key_event) = event else {
        return None;
    };
    if key_event.kind != KeyEventKind::Press {
        return None;
    }

    let code = match key_event.code {
        TerminalKeyCode::Backspace => KeyCode::Backspace,
        TerminalKeyCode::Enter => KeyCode::Enter,
        TerminalKeyCode::Esc => KeyCode::Esc,
        TerminalKeyCode::Left => KeyCode::Left,
        TerminalKeyCode::Right => KeyCode::Right,
        TerminalKeyCode::Up => KeyCode::Up,
        TerminalKeyCode::Down => KeyCode::Down,
        TerminalKeyCode::Tab => KeyCode::Tab,
        TerminalKeyCode::Char(character) => KeyCode::Char(character),
        _ => return None,
    };

    let mut modifiers = InputKeyModifiers::NONE;
    if key_event.modifiers.contains(KeyModifiers::SHIFT) {
        modifiers = modifiers | InputKeyModifiers::SHIFT;
    }
    if key_event.modifiers.contains(KeyModifiers::CONTROL) {
        modifiers = modifiers | InputKeyModifiers::CONTROL;
    }
    if key_event.modifiers.contains(KeyModifiers::ALT) {
        modifiers = modifiers | InputKeyModifiers::ALT;
    }

    Some(Key { code, modifiers })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_supported_terminal_key_and_modifiers() {
        let event = Event::Key(ratatui::crossterm::event::KeyEvent::new(
            TerminalKeyCode::Char('x'),
            KeyModifiers::SHIFT | KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));

        assert_eq!(
            translate_terminal_event(&event),
            Some(Key {
                code: KeyCode::Char('x'),
                modifiers: InputKeyModifiers::SHIFT
                    | InputKeyModifiers::CONTROL
                    | InputKeyModifiers::ALT,
            })
        );
    }

    #[test]
    fn ignores_non_key_events_and_non_press_keys() {
        assert_eq!(translate_terminal_event(&Event::Resize(80, 24)), None);

        let release = Event::Key(ratatui::crossterm::event::KeyEvent {
            code: TerminalKeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: ratatui::crossterm::event::KeyEventState::NONE,
        });

        assert_eq!(translate_terminal_event(&release), None);
    }

    #[test]
    fn ignores_terminal_keys_without_an_input_key_equivalent() {
        let event = Event::Key(ratatui::crossterm::event::KeyEvent::new(
            TerminalKeyCode::F(1),
            KeyModifiers::NONE,
        ));

        assert_eq!(translate_terminal_event(&event), None);
    }
}
