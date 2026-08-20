use std::ops::Range;
use std::thread;
use std::time::Duration;

use tempfile::tempdir;
use whatsrust as wr;
use wp_tui::db::DatabaseHandler;

fn message(id: &str, body: &str) -> wr::Message {
    let chat = wr::JID::from("chat@example.test".to_owned());
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: chat,
            mentions_self: false,
            timestamp: 1,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: wr::MessageContent::Text(body.into()),
    }
}

#[test]
fn mention_ranges_round_trip_and_legacy_rows_reload_empty() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("messages.db");
    let mut handler = DatabaseHandler::new(&path);
    handler.init();

    let mut stored = message("mention", "Hi @阿丽");
    stored.info.mentions_self = true;
    wr::store_message_mention_ranges(
        &stored.info.id,
        "Hi @阿丽",
        vec![Range { start: 3, end: 10 }],
    );
    handler.add_message(&stored);
    thread::sleep(Duration::from_millis(1_200));
    handler.stop();

    let mut reloaded = DatabaseHandler::new(&path);
    reloaded.init();
    let loaded = reloaded.get_messages();
    assert_eq!(loaded.len(), 1);
    assert!(loaded[0].info.mentions_self);
    assert_eq!(
        wr::message_mention_ranges(&loaded[0].info.id, "Hi @阿丽"),
        vec![Range { start: 3, end: 10 }]
    );
    reloaded.stop();

    let legacy_path = dir.path().join("legacy.db");
    let connection = rusqlite::Connection::open(&legacy_path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE text_messages (id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER, quote_id TEXT, is_from_me INTEGER, read INTEGER, message TEXT);
             CREATE TABLE file_messages (id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER, quote_id TEXT, is_from_me INTEGER, read INTEGER, kind INTEGER, path TEXT, file_id TEXT, caption TEXT);
             INSERT INTO text_messages VALUES ('legacy', 'chat@example.test', 'chat@example.test', 0, NULL, 0, 0, 'hello');",
        )
        .unwrap();
    drop(connection);

    let mut legacy = DatabaseHandler::new(&legacy_path);
    legacy.init();
    let loaded = legacy.get_messages();
    assert_eq!(loaded.len(), 1);
    assert!(wr::message_mention_ranges(&loaded[0].info.id, "hello").is_empty());
    legacy.stop();
}
