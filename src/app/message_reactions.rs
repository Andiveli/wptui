use std::sync::Arc;

use whatsrust as wr;

pub struct RecordMessageReaction {
    pub message_id: wr::MessageId,
    pub participant: wr::JID,
    pub emoji: Arc<str>,
}

pub trait MessageReactionWritePort {
    fn record(&self, command: RecordMessageReaction);
}
