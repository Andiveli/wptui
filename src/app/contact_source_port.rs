use std::sync::Arc;

use whatsrust as wr;

pub trait ContactSourcePort {
    fn get_contacts(&self) -> Vec<(wr::JID, Arc<str>)>;
    /// Authenticated self aliases only; safe before joined-group hydration.
    fn get_self_contacts(&self) -> Vec<(wr::JID, Arc<str>)>;
    /// Local contact aliases only; never queries joined groups.
    fn get_contact_aliases(&self) -> Vec<(wr::JID, Arc<str>)>;
}
