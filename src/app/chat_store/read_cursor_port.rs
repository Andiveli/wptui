use whatsrust as wr;

#[derive(Clone, Debug)]
pub struct StoreChatReadCursor {
    pub chat: wr::JID,
    pub message_id: Option<wr::MessageId>,
    pub timestamp: i64,
}

pub trait ChatReadCursorPort {
    fn load(&self) -> Vec<(wr::JID, wr::MessageId, i64)>;
    fn store(&self, command: StoreChatReadCursor);
}
