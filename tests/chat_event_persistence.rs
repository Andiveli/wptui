use tempfile::tempdir;
use wp_tui::{
    app::{
        Chat,
        chat_store::{
            hydration_port::ChatStoreHydrationPort,
            write_port::{ChatStoreWritePort, PersistChat, PersistChatMessage},
        },
    },
    db::{DatabaseHandler, SqliteChatStoreHydration},
};

mod common;
use common::TestApp;

#[test]
fn chat_event_commands_survive_queue_ordering_and_restart() {
    for (name, chat_first) in [("chat-first", true), ("message-first", false)] {
        let directory = tempdir().unwrap();
        let path = directory.path().join(format!("{name}.db"));
        let chat = Chat {
            jid: "chat@example.test".to_owned().into(),
            last_message_time: None,
        };
        let message = whatsrust::Message {
            info: whatsrust::MessageInfo {
                id: "message".into(),
                chat: chat.jid.clone(),
                sender: chat.jid.clone(),
                mentions_self: false,
                timestamp: 42,
                is_from_me: false,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: whatsrust::MessageContent::Text("body".into()),
        };
        let mut handler = DatabaseHandler::new(&path);
        handler.init();
        let writer = handler.chat_store_writer();
        let port: &dyn ChatStoreWritePort = &writer;
        if chat_first {
            port.persist_chat(PersistChat { chat: chat.clone() });
            port.persist(PersistChatMessage {
                chat: chat.clone(),
                message: message.clone(),
            });
        } else {
            port.persist(PersistChatMessage {
                chat: chat.clone(),
                message: message.clone(),
            });
            port.persist_chat(PersistChat { chat: chat.clone() });
        }
        handler.stop();

        let hydration = SqliteChatStoreHydration::new(&path);
        let stored = hydration.load();
        assert_eq!(
            stored
                .chats
                .iter()
                .filter(|saved| saved.jid == chat.jid)
                .count(),
            1
        );
        assert_eq!(
            stored
                .messages
                .iter()
                .filter(|saved| saved.info.id == message.info.id)
                .count(),
            1
        );
        let mut app = TestApp::with_database(&path);
        app.load_data_from_db();
        assert_eq!(
            app.chats[&chat.jid].last_message_time,
            Some(message.info.timestamp)
        );
    }
}
