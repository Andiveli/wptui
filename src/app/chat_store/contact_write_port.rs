use std::sync::Arc;

use whatsrust as wr;

pub struct PersistContact {
    pub jid: wr::JID,
    pub name: Arc<str>,
}

pub trait ContactWritePort {
    fn persist(&self, command: PersistContact);
}
