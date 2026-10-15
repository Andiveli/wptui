use crate::app::chat_read_sync_port::ChatReadSyncPort;
use whatsrust as wr;

pub struct WhatsRustChatReadSync {
    worker: wr::ReadSyncWorker,
}

impl Default for WhatsRustChatReadSync {
    fn default() -> Self {
        Self {
            worker: wr::ReadSyncWorker::new(),
        }
    }
}

impl ChatReadSyncPort for WhatsRustChatReadSync {
    fn schedule(
        &mut self,
        chat: &wr::JID,
        message_id: &wr::MessageId,
        timestamp: i64,
        from_me: bool,
        participant: Option<&wr::JID>,
    ) -> bool {
        self.worker
            .schedule(chat, message_id, timestamp, from_me, participant)
    }

    fn shutdown(&mut self) {
        self.worker.shutdown();
    }

    fn restart(&mut self) {
        self.worker = wr::ReadSyncWorker::new();
    }
}
