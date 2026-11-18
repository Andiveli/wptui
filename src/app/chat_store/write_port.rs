use crate::app::Chat;
use whatsrust as wr;

pub struct PersistChatMessage {
    pub chat: Chat,
    pub message: wr::Message,
}

pub struct PersistChat {
    pub chat: Chat,
}

pub struct PersistMessage {
    pub message: wr::Message,
}

pub trait ChatStoreWritePort {
    fn persist(&self, command: PersistChatMessage);
    /// Additive contract: existing adapters retain their ordinary write path.
    fn persist_with_native_preview(
        &self,
        command: PersistChatMessage,
        preview: Option<wr::NativeLinkPreview>,
    ) {
        let _ = preview;
        self.persist(command);
    }
    fn persist_chat(&self, command: PersistChat);
    fn persist_message(&self, command: PersistMessage);
}
