use tempfile::tempdir;
use wp_tui::{
    app::{
        Chat,
        chat_store::{
            hydration_port::ChatStoreHydrationPort,
            write_port::{ChatStoreWritePort, PersistMessage},
        },
    },
    db::{DatabaseHandler, SqliteChatStoreHydration},
};

#[test]
fn receipt_message_writeback_drains_after_seed_and_survives_hydration() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("receipts.db");
    let chat = Chat {
        jid: "chat@example.test".to_owned().into(),
        last_message_time: None,
    };
    let mut message = whatsrust::Message {
        info: whatsrust::MessageInfo {
            id: "outgoing".into(),
            chat: chat.jid.clone(),
            sender: chat.jid.clone(),
            mentions_self: false,
            timestamp: 1,
            is_from_me: true,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: whatsrust::MessageContent::Text("body".into()),
    };
    let mut handler = DatabaseHandler::new(&path);
    handler.init();
    handler.add_chat(&chat);
    handler.add_message(&message);
    message.info.read_by = 1;
    let writer = handler.chat_store_writer();
    let port: &dyn ChatStoreWritePort = &writer;
    port.persist_message(PersistMessage { message });
    handler.stop();

    let hydration = SqliteChatStoreHydration::new(&path);
    let port: &dyn ChatStoreHydrationPort = &hydration;
    assert_eq!(port.load().messages[0].info.read_by, 1);
}
