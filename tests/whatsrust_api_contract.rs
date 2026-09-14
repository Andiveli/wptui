use whatsrust::{Event, JID, MessageId, ReadSyncWorker, ReceiptKind};

#[test]
fn wp_tui_uses_the_local_whatsrust_read_sync_api() {
    let _sync_chat_read: fn(&JID, &MessageId, i64, bool, Option<&JID>) = whatsrust::sync_chat_read;
    let _receipt_kind = ReceiptKind::ReadSelf;
    let mut read_sync_worker = ReadSyncWorker::new();
    read_sync_worker.shutdown();
    let _event = Event::MarkChatAsRead {
        chat: JID("chat@example.test".into()),
        message_id: MessageId::from("message"),
        read: true,
        timestamp: 1,
        from_me: false,
        participant: None,
    };
}
