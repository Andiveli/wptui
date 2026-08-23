pub use crate::input_key::{Key, KeyCode, KeyModifiers};
use crate::keybindings::{SequenceResolution, resolve_sequence};
use ratatui::crossterm::event::KeyEvent;

#[derive(Debug, Clone, Default)]
pub struct KeybindHandler {
    pub key_buffer: Vec<Key>,
    pub key_sequence_active: bool,
}

impl KeybindHandler {
    pub fn resolve(&mut self, key: Key) -> SequenceResolution {
        if key == Key::k(KeyCode::Esc) {
            self.key_buffer.clear();
            self.key_sequence_active = false;
            return SequenceResolution::Cancelled;
        }

        self.key_buffer.push(key);
        let resolution = resolve_sequence(&self.key_buffer);
        self.key_sequence_active = matches!(resolution, SequenceResolution::Partial);
        if !self.key_sequence_active {
            self.key_buffer.clear();
        }
        resolution
    }

    pub fn buffered_keys(&self) -> &[Key] {
        &self.key_buffer
    }

    /// Call this when a key is pressed down. It will return the `Key` that was pressed and update the internal state of the handler.
    pub fn pressed_start(&mut self, event: &KeyEvent) -> Key {
        self.key_sequence_active = false;
        let key = Key {
            code: match event.code {
                ratatui::crossterm::event::KeyCode::Backspace => KeyCode::Backspace,
                ratatui::crossterm::event::KeyCode::Enter => KeyCode::Enter,
                ratatui::crossterm::event::KeyCode::Esc => KeyCode::Esc,
                ratatui::crossterm::event::KeyCode::Left => KeyCode::Left,
                ratatui::crossterm::event::KeyCode::Right => KeyCode::Right,
                ratatui::crossterm::event::KeyCode::Up => KeyCode::Up,
                ratatui::crossterm::event::KeyCode::Down => KeyCode::Down,
                ratatui::crossterm::event::KeyCode::Tab => KeyCode::Tab,
                ratatui::crossterm::event::KeyCode::Char(c) => KeyCode::Char(c),
                _ => return Key::k(KeyCode::Esc),
            },
            modifiers: KeyModifiers::NONE,
        };
        if key == Key::k(KeyCode::Esc) && !self.key_buffer.is_empty() {
            self.key_buffer.clear();
        } else {
            self.key_buffer.push(key.clone());
        }
        key
    }

    pub fn pressed_end(&mut self) {
        if !self.key_sequence_active {
            self.key_buffer.clear();
        }
    }

    pub fn kp(&mut self, expected: &[Key]) -> bool {
        if self
            .key_buffer
            .iter()
            .zip(expected.iter())
            .all(|(a, b)| a == b)
        {
            if self.key_buffer.len() == expected.len() {
                self.key_buffer.clear();
                return true;
            } else {
                self.key_sequence_active = true;
            }
        }
        false
    }

    pub fn kp_partial(&mut self, expected: &[Key]) -> Option<Vec<Key>> {
        // pub fn kp_partial(&mut self, expected: &[Key]) -> bool {
        if self.key_buffer.len() >= expected.len()
            && self
                .key_buffer
                .iter()
                .zip(expected.iter())
                .all(|(a, b)| a == b)
        {
            // return true;
            // self.key_sequence_active = true;
            return Some(self.key_buffer[expected.len()..].to_vec());
            // return Some(&expected[self.key_buffer.len()..]);
        }
        None
        // false
    }
}
