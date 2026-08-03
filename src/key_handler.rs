use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::{self, Input};

use crate::app::actions::{SequenceResolution, resolve_sequence};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}
impl Key {
    pub fn c(c: char) -> Self {
        if c.is_ascii_uppercase() {
            Self {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::SHIFT,
            }
        } else {
            Self {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
            }
        }
    }

    pub fn k(c: KeyCode) -> Self {
        Self {
            code: c,
            modifiers: KeyModifiers::NONE,
        }
    }

    pub fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
        }
    }

    pub fn ctrl_shift(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        }
    }
}

impl From<Key> for Input {
    fn from(value: Key) -> Self {
        Input {
            key: value.code.into(),
            ctrl: value.modifiers.contains(KeyModifiers::CONTROL),
            alt: value.modifiers.contains(KeyModifiers::ALT),
            shift: value.modifiers.contains(KeyModifiers::SHIFT),
        }
    }
}

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
            code: event.code,
            modifiers: event.modifiers,
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
