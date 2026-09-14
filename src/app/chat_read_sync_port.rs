use whatsrust as wr;

pub trait ChatReadSyncPort: Send + 'static {
    fn schedule(
        &mut self,
        chat: &wr::JID,
        message_id: &wr::MessageId,
        timestamp: i64,
        from_me: bool,
        participant: Option<&wr::JID>,
    ) -> bool;

    fn shutdown(&mut self);

    fn restart(&mut self);
}
