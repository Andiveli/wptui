use std::path::PathBuf;

use tempfile::tempdir;
use whatsrust::{FileContent, FileKind, JID, Message, MessageContent, MessageInfo};
use wp_tui::{
    app::{PurgeExpiredStatuses, StatusRetentionPort},
    db::{DatabaseHandler, SqliteStatusRetention},
};

const RETENTION_SECS: i64 = 24 * 60 * 60;
const NOW: i64 = 2_000_000_000;

fn message(chat: &JID, id: &str, timestamp: i64, content: MessageContent) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            sender: chat.clone(),
            chat: chat.clone(),
            mentions_self: false,
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: content,
    }
}

#[test]
fn sqlite_status_retention_purges_expired_rows_through_the_port() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("status-retention.db");
    let status = JID::from("status@broadcast".to_owned());
    let regular = JID::from("chat@example.test".to_owned());
    let mut handler = DatabaseHandler::new(&path);
    handler.init();
    for (chat, id, timestamp, content) in [
        (
            &status,
            "expired-file",
            NOW - RETENTION_SECS - 1,
            MessageContent::File(FileContent {
                kind: FileKind::Image,
                path: "images/expired.jpg".into(),
                ..Default::default()
            }),
        ),
        (
            &status,
            "expired-text",
            NOW - RETENTION_SECS - 1,
            MessageContent::Text("expired-text".into()),
        ),
        (
            &status,
            "boundary-status",
            NOW - RETENTION_SECS,
            MessageContent::Text("boundary-status".into()),
        ),
        (
            &regular,
            "regular-message",
            NOW - 2 * RETENTION_SECS,
            MessageContent::Text("regular-message".into()),
        ),
    ] {
        handler.add_message(&message(chat, id, timestamp, content));
    }
    handler.stop();

    let adapter = SqliteStatusRetention::new(&path);
    let port: &dyn StatusRetentionPort = &adapter;
    assert_eq!(
        port.purge_expired_statuses(PurgeExpiredStatuses { now: NOW })
            .unwrap()
            .media_paths,
        vec![PathBuf::from("images/expired.jpg")]
    );
    assert!(
        SqliteStatusRetention::new(directory.path())
            .purge_expired_statuses(PurgeExpiredStatuses { now: NOW })
            .is_err()
    );

    let mut reopened = DatabaseHandler::new(&path);
    let remaining: Vec<_> = reopened
        .get_messages()
        .iter()
        .map(|message| message.info.id.to_string())
        .collect();
    assert!(
        remaining.contains(&"boundary-status".to_owned())
            && remaining.contains(&"regular-message".to_owned())
            && !remaining.contains(&"expired-file".to_owned())
            && !remaining.contains(&"expired-text".to_owned()),
        "only boundary and regular rows must remain: {remaining:?}"
    );
    reopened.stop();
}
