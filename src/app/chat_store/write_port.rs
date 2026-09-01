use crate::app::Chat;
use whatsrust as wr;

pub struct PersistChatMessage {
    pub chat: Chat,
    pub message: wr::Message,
}

pub struct PersistMessage {
    pub message: wr::Message,
}

pub trait ChatStoreWritePort {
    fn persist(&self, command: PersistChatMessage);
    fn persist_message(&self, command: PersistMessage);
}
