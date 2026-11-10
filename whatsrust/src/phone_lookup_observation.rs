use std::fmt;

/// Temporary local display text. Never use this value as a chat recipient.
#[derive(Clone, Eq, PartialEq)]
pub struct LocalPhoneLookupObservation(String);

impl LocalPhoneLookupObservation {
    pub fn from_display_text(text: String) -> Option<Self> {
        if text.is_empty()
            || text.len() > 8192
            || !text.bytes().all(|byte| byte == b'\n' || (32..=126).contains(&byte))
        {
            return None;
        }
        Some(Self(text))
    }

    /// Explicit local rendering access; Debug intentionally cannot reveal it.
    pub fn display_text(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for LocalPhoneLookupObservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LocalPhoneLookupObservation([redacted])")
    }
}
