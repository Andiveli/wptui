use std::sync::Arc;

use crate::app::ContactSourcePort;
use whatsrust as wr;

pub struct WhatsRustContactSource;

impl ContactSourcePort for WhatsRustContactSource {
    fn get_contacts(&self) -> Vec<(wr::JID, Arc<str>)> {
        wr::get_contacts()
    }

    fn verified_phone_for_lid(&self, lid: &wr::JID) -> Option<wr::JID> {
        if !lid.0.ends_with("@lid") {
            return None;
        }
        wr::resolve_dm_chat(lid).filter(|phone| {
            phone.0.strip_suffix("@s.whatsapp.net").is_some_and(|user| {
                !user.is_empty() && user.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
    }
}
