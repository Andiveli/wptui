use whatsrust::{JID, PhoneLookupError};

/// Application port: a network lookup never runs on the UI thread.
pub trait PhoneLookupPort: Send {
    fn lookup(&mut self, phone: &str) -> Result<Option<JID>, PhoneLookupError>;
}
