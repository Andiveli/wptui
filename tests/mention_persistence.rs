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

#[test]
fn message_ownership_is_monotonic_for_text_and_file_rows() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ownership.db");
    let mut handler = DatabaseHandler::new(&path);
    handler.init();

    let mut text = message("text", "verified");
    text.info.is_from_me = true;
    handler.add_message(&text);
    handler.add_message(&message("text", "echo"));

    let mut file = message("file", "verified-file");
    file.info.is_from_me = true;
    file.message = wr::MessageContent::File(wr::FileContent {
        kind: wr::FileKind::Document,
        path: "file.bin".into(),
        file_id: "file-id".into(),
        caption: None,
    });
    handler.add_message(&file);
    let mut file_echo = file.clone();
    file_echo.info.is_from_me = false;
    file_echo.info.timestamp += 1;
    handler.add_message(&file_echo);
    thread::sleep(Duration::from_millis(1_200));
    handler.stop();

    let mut reloaded = DatabaseHandler::new(&path);
    reloaded.init();
    let loaded = reloaded.get_messages();
    assert!(
        loaded
            .iter()
            .any(|item| { item.info.id.as_ref() == "text" && item.info.is_from_me })
    );
    assert!(
        loaded
            .iter()
            .any(|item| { item.info.id.as_ref() == "file" && item.info.is_from_me })
    );
    reloaded.stop();
}
