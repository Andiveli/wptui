use std::sync::Arc;

use whatsrust as wr;

pub trait ContactSourcePort {
    fn get_contacts(&self) -> Vec<(wr::JID, Arc<str>)>;
    /// A verified PN for a LID, if the device's identity mapping knows one.
    fn verified_phone_for_lid(&self, lid: &wr::JID) -> Option<wr::JID>;
}
