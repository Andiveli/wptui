#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Backspace,
    Enter,
    Esc,
    Left,
    Right,
    Up,
    Down,
    Tab,
    Char(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyModifiers(u8);

impl KeyModifiers {
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1);
    pub const CONTROL: Self = Self(2);
    pub const ALT: Self = Self(4);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl std::ops::BitOr for KeyModifiers {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl Key {
    pub const fn code_char(&self) -> char {
        match self.code {
            KeyCode::Char(c) => c,
            _ => panic!("leader bindings must use character keys"),
        }
    }

    pub fn c(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: if c.is_ascii_uppercase() || c == '?' {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            },
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
