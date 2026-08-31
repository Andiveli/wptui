//! Media cleanup must remove only application-owned files (plus thumbnail
//! sidecars) and must never touch external or traversing paths, even through
//! the status purge helper.

use std::path::{Path, PathBuf};

use tempfile::tempdir;
use whatsrust::{FileContent, FileKind, JID, Message, MessageContent, MessageInfo};
use wp_tui::app::{FileMeta, MessageAction, MessageActionKind, Metadata};
use wp_tui::db::SqliteChatStoreHydration;
mod common;
use common::TestApp;

fn file_message(id: &str, path: &str) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: JID::from("chat@example.test".to_owned()),
            sender: JID::from("chat@example.test".to_owned()),
            mentions_self: false,
            timestamp: 1,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::File(FileContent {
            kind: FileKind::Image,
            path: path.into(),
            file_id: "file-1".into(),
            caption: Some("secret caption".into()),
        }),
    }
}

fn app_with_media(media: &Path) -> TestApp {
    let mut app = TestApp::new();
    app.media_path = media.to_owned();
    let db_path = media.join("app.db");
    let db_handler = wp_tui::db::DatabaseHandler::new(&db_path);
    app.set_chat_store_write(Box::new(db_handler.chat_store_writer()));
    std::mem::replace(&mut app.db_handler, db_handler).stop();
    app.set_chat_store_hydration(Box::new(SqliteChatStoreHydration::new(&db_path)));
    app.db_handler.init();
    app
}

#[test]
fn deleted_file_message_removes_owned_media_and_thumbnail_sidecar() {
    let directory = tempdir().unwrap();
    let media = directory.path().join("media");
    std::fs::create_dir_all(media.join("images")).unwrap();
    let main = media.join("images/photo.png");
    let sidecar = media.join("images/photo.jpg");
    std::fs::write(&main, b"image").unwrap();
    std::fs::write(&sidecar, b"thumb").unwrap();

    let mut app = app_with_media(&media);
    let msg = file_message("target", "images/photo.png");
    app.messages.insert(msg.info.id.clone(), msg);
    app.metadata
        .insert("target".into(), Metadata::File(FileMeta::Downloaded));

    app.apply_message_action(MessageAction {
        action_id: "delete".into(),
        target_message_id: "target".into(),
        chat: JID::from("chat@example.test".to_owned()),
        sender: JID::from("chat@example.test".to_owned()),
        kind: MessageActionKind::Delete,
        occurred_at: 2,
        arrival_order: 1,
    });

    assert!(
        !main.exists(),
        "owned media file must be removed when the delete is processed"
    );
    assert!(
        !sidecar.exists(),
        "video thumbnail sidecar must be removed with the media"
    );
    assert!(matches!(
        &app.messages["target"].message,
        MessageContent::Text(body) if body.as_ref() == "This message was deleted."
    ));
    assert!(
        app.metadata
            .get(&whatsrust::MessageId::from("target"))
            .is_none(),
        "cached media metadata must not survive deletion"
    );
    app.db_handler.stop();
}

#[test]
fn status_media_purge_never_removes_paths_outside_the_media_root() {
    let directory = tempdir().unwrap();
    let media = directory.path().join("media");
    std::fs::create_dir_all(&media).unwrap();
    let victim = directory.path().join("victim.txt");
    std::fs::write(&victim, b"keep me").unwrap();

    wp_tui::app::remove_status_media_files(&media, &[PathBuf::from("../victim.txt")]);

    assert!(
        victim.exists(),
        "a traversing status path must never escape the media root"
    );
}

#[test]
fn media_cleanup_ignores_absolute_paths_outside_the_media_root() {
    let directory = tempdir().unwrap();
    let media = directory.path().join("media");
    std::fs::create_dir_all(&media).unwrap();
    let external = directory.path().join("external.png");
    std::fs::write(&external, b"keep me").unwrap();

    wp_tui::app::remove_owned_media_files(&media, &[external.clone()]);

    assert!(external.exists(), "absolute external paths must be ignored");
}
