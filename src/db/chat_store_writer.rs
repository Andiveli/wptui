use crate::app::chat_store::write_port::{
    ChatStoreWritePort, PersistChat, PersistChatMessage, PersistMessage,
};

use super::worker::QueueHandle;

pub struct SqliteChatStoreWriter {
    queue: QueueHandle,
}

impl SqliteChatStoreWriter {
    pub(super) fn new(queue: QueueHandle) -> Self {
        Self { queue }
    }
}

impl ChatStoreWritePort for SqliteChatStoreWriter {
    fn persist(&self, command: PersistChatMessage) {
        self.queue.queue_chat_message(command.chat, command.message);
    }

    fn persist_chat(&self, command: PersistChat) {
        self.queue.queue_chat(command.chat);
    }

    fn persist_message(&self, command: PersistMessage) {
        self.queue.queue_message(command.message);
    }
}
