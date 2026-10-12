use std::sync::Arc;

use crate::app::ContactSourcePort;
use whatsrust as wr;

pub struct WhatsRustContactSource;

impl ContactSourcePort for WhatsRustContactSource {
    fn get_contacts(&self) -> Vec<(wr::JID, Arc<str>)> {
        wr::get_contacts()
    }

    fn get_self_contacts(&self) -> Vec<(wr::JID, Arc<str>)> {
        wr::get_self_contacts()
    }
}
