use std::path::{Path, PathBuf};

use tempfile::tempdir;
use whatsrust::{FileContent, FileKind, JID, Message, MessageContent, MessageInfo};
use wp_tui::app::remove_status_media_files;
use wp_tui::db::DatabaseHandler;

/// WhatsApp statuses expire 24 hours after posting. The local purge must use
/// the same window so the DB does not accumulate status broadcasts forever.
const RETENTION_SECS: i64 = 24 * 60 * 60;
const NOW: i64 = 2_000_000_000;

fn status_text(id: &str, timestamp: i64, sender: &JID) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: JID::from("status@broadcast".to_owned()),
            sender: sender.clone(),
            mentions_self: false,
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::Text(id.into()),
    }
}

fn status_file(id: &str, timestamp: i64, sender: &JID, path: &str) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: JID::from("status@broadcast".to_owned()),
            sender: sender.clone(),
            mentions_self: false,
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::File(FileContent {
            kind: FileKind::Image,
            path: path.into(),
            ..Default::default()
        }),
    }
}

fn regular_chat_message(id: &str, timestamp: i64) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: JID::from("chat@example.test".to_owned()),
            sender: JID::from("alice@s.whatsapp.net".to_owned()),
            mentions_self: false,
            forwarding: Default::default(),
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
        },
        message: MessageContent::Text(id.into()),
    }
}

fn fresh_handler(path: &Path) -> DatabaseHandler {
    let db = DatabaseHandler::new(path);
    db.init();
    db
}

fn remaining_ids(db: &DatabaseHandler) -> Vec<String> {
    db.get_messages()
        .iter()
        .map(|message| message.info.id.to_string())
        .collect()
}

#[test]
fn purge_removes_only_expired_status_messages() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("retention.db");
    let alice = JID::from("alice@s.whatsapp.net".to_owned());

    let mut db = fresh_handler(&path);
    db.add_message(&status_text("old-status", NOW - 2 * RETENTION_SECS, &alice));
    db.add_message(&status_file(
        "old-file",
        NOW - 2 * RETENTION_SECS,
        &alice,
        "imgs/old.jpg",
    ));
    db.add_message(&status_text("fresh-status", NOW - 3_600, &alice));
    db.add_message(&regular_chat_message("old-chat", NOW - 2 * RETENTION_SECS));
    db.stop(); // flush queued writes and join the writer thread

    let mut db = fresh_handler(&path);
    let purged = db.purge_expired_statuses(NOW);

    assert_eq!(purged, vec![PathBuf::from("imgs/old.jpg")]);
    let remaining = remaining_ids(&db);
    assert!(
        remaining.contains(&"fresh-status".to_owned()),
        "fresh status must survive: {remaining:?}"
    );
    assert!(
        remaining.contains(&"old-chat".to_owned()),
        "regular chats must be untouched: {remaining:?}"
    );
    assert!(
        !remaining.contains(&"old-status".to_owned()),
        "expired status text must be purged: {remaining:?}"
    );
    assert!(
        !remaining.contains(&"old-file".to_owned()),
        "expired status media row must be purged: {remaining:?}"
    );
    db.stop();
}

#[test]
fn purge_keeps_status_at_the_24h_boundary() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("retention.db");
    let alice = JID::from("alice@s.whatsapp.net".to_owned());

    let mut db = fresh_handler(&path);
    db.add_message(&status_text("at-cutoff", NOW - RETENTION_SECS, &alice));
    db.add_message(&status_text(
        "one-second-older",
        NOW - RETENTION_SECS - 1,
        &alice,
    ));
    db.stop();

    let mut db = fresh_handler(&path);
    let purged = db.purge_expired_statuses(NOW);
    assert!(purged.is_empty());
    let remaining = remaining_ids(&db);
    assert!(
        remaining.contains(&"at-cutoff".to_owned()),
        "status at exactly 24h must survive: {remaining:?}"
    );
    assert!(!remaining.contains(&"one-second-older".to_owned()));
    db.stop();
}

#[test]
fn remove_status_media_files_deletes_media_and_video_sidecars_only() {
    let directory = tempdir().unwrap();
    let media = directory.path().join("media");
    std::fs::create_dir_all(media.join("imgs")).unwrap();
    std::fs::create_dir_all(media.join("videos")).unwrap();

    let old_img = media.join("imgs/old.jpg");
    let old_video = media.join("videos/old.mp4");
    let old_sidecar = media.join("videos/old.jpg");
    let fresh_img = media.join("imgs/fresh.jpg");
    let chat_img = media.join("imgs/chat.jpg");
    for file in [&old_img, &old_video, &old_sidecar, &fresh_img, &chat_img] {
        std::fs::write(file, b"x").unwrap();
    }

    remove_status_media_files(
        &media,
        &[
            PathBuf::from("imgs/old.jpg"),
            PathBuf::from("videos/old.mp4"),
        ],
    );

    assert!(!old_img.exists(), "purged image must be deleted");
    assert!(!old_video.exists(), "purged video must be deleted");
    assert!(
        !old_sidecar.exists(),
        "video thumbnail sidecar must be deleted"
    );
    assert!(fresh_img.exists(), "fresh status media must survive");
    assert!(chat_img.exists(), "regular chat media must survive");
}
