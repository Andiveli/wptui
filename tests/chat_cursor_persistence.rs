use tempfile::tempdir;
use wp_tui::{
    app::{ChatReadCursorPort, StoreChatReadCursor},
    db::{DatabaseHandler, SqliteChatReadCursor},
};

#[test]
fn sqlite_chat_read_cursor_overwrites_and_survives_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("chat-cursors.db");
    let chat = whatsrust::JID("alice:7@example.test".into());
    let id1 = whatsrust::MessageId::from("first");
    let id2 = whatsrust::MessageId::from("latest");
    let mut handler = DatabaseHandler::new(&path);
    handler.init();

    let cursor = SqliteChatReadCursor::new(&path);
    let port: &dyn ChatReadCursorPort = &cursor;
    port.store(StoreChatReadCursor {
        chat: chat.clone(),
        message_id: Some(id1),
        timestamp: 42,
    });
    port.store(StoreChatReadCursor {
        chat: chat.clone(),
        message_id: Some(id2.clone()),
        timestamp: 84,
    });
    assert_eq!(port.load(), vec![(chat.clone(), id2.clone(), 84)]);
    handler.stop();

    let mut reopened_handler = DatabaseHandler::new(&path);
    let reopened = SqliteChatReadCursor::new(&path);
    assert_eq!(reopened.load(), vec![(chat, id2, 84)]);
    reopened_handler.stop();
}
