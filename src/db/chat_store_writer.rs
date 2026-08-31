use crate::app::chat_store::write_port::{ChatStoreWritePort, PersistChatMessage};

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
}
