use crate::app::phone_lookup_port::PhoneLookupPort;
use whatsrust::{JID, PhoneLookupError};

pub struct WhatsRustPhoneLookup;

impl PhoneLookupPort for WhatsRustPhoneLookup {
    fn lookup(&mut self, phone: &str) -> Result<Option<JID>, PhoneLookupError> {
        whatsrust::lookup_phone(phone)
    }
}
