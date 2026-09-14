use std::sync::Arc;

use whatsrust as wr;

pub trait ContactSourcePort {
    fn get_contacts(&self) -> Vec<(wr::JID, Arc<str>)>;
}
