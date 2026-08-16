use std::sync::Arc;

use tempfile::tempdir;
use whatsrust::{FileKind, JID, Message, MessageContent, MessageInfo};
use wp_tui::app::{App, FileMeta, Metadata};
use wp_tui::ui::message_list::{AuthorGroupContext, MESSAGE_HEIGHT_CACHE_CAPACITY, message_height};
mod common;
use common::TestApp;

#[test]
fn applying_reaction_invalidates_cached_message_height() {
    let dir = tempdir().unwrap();
    let mut app = app_with_database(&dir.path().join("reaction-height.db"));
    let message = text_message("reaction", "short");

    assert_eq!(height(&message, 20, false, &mut app), 2);
    assert!(app.message_height_cache.contains(&message.info.id));

    app.apply_reaction(
        &message.info.id,
        String::from("alice@example.test").into(),
        "👍".into(),
    );

    assert!(!app.message_height_cache.contains(&message.info.id));
    assert_eq!(height(&message, 20, false, &mut app), 3);
    app.db_handler.stop();
}

#[test]
fn width_and_content_changes_cannot_reuse_stale_height() {
    let mut app = TestApp::new();
    let mut message = text_message("edited", "1234567890");

    assert_eq!(height(&message, 10, false, &mut app), 2);
    assert_eq!(height(&message, 5, false, &mut app), 3);

    message.message = MessageContent::Text("x".into());
    assert_eq!(height(&message, 5, false, &mut app), 2);
}

#[test]
fn unchanged_text_layout_reuses_cached_height() {
    let mut app = TestApp::new();
    let message = text_message("unchanged", "cached text");

    assert_eq!(height(&message, 20, false, &mut app), 2);
    let first_measurements = app.message_height_cache.measurement_count();

    assert_eq!(height(&message, 20, false, &mut app), 2);
    assert_eq!(
        app.message_height_cache.measurement_count(),
        first_measurements
    );
}

#[test]
fn same_length_replacements_with_different_cell_widths_refresh_height() {
    let mut app = TestApp::new();
    let mut message = text_message("same-length", "aaaa");

    assert_eq!(height(&message, 3, false, &mut app), 3);

    message.message = MessageContent::Text("界a".into());
    assert_eq!(height(&message, 3, false, &mut app), 2);
}

#[test]
fn unchanged_file_caption_layout_reuses_cached_height() {
    let mut app = TestApp::new();
    let message = file_message("caption-cache", FileKind::Document, Some("caption"));

    assert_eq!(height(&message, 20, false, &mut app), 3);
    let first_measurements = app.message_height_cache.measurement_count();

    assert_eq!(height(&message, 20, false, &mut app), 3);
    assert_eq!(
        app.message_height_cache.measurement_count(),
        first_measurements
    );
}

#[test]
fn reply_context_presence_cannot_reuse_stale_height() {
    let mut app = TestApp::new();
    let mut message = text_message("reply", "short");

    assert_eq!(height(&message, 20, false, &mut app), 2);

    message.info.quote_id = Some("quoted-message".into());
    assert_eq!(height(&message, 20, false, &mut app), 3);
}

#[test]
fn delayed_media_preview_keeps_height_and_cache_stable() {
    let mut app = TestApp::new();
    let messages = [
        file_message("image", FileKind::Image, Some("caption")),
        file_message("video", FileKind::Video, Some("caption")),
        file_message("sticker", FileKind::Sticker, None),
    ];

    for message in &messages {
        app.metadata
            .insert(message.info.id.clone(), Metadata::File(FileMeta::Loading));
        let loading_height = height(message, 20, false, &mut app);
        assert_eq!(loading_height, height(message, 20, false, &mut app));

        app.metadata
            .insert(message.info.id.clone(), Metadata::File(FileMeta::Loaded));
        assert_eq!(height(message, 20, false, &mut app), loading_height);
    }
}

#[test]
fn delayed_media_preview_preserves_offsets_visible_range_anchor_and_selection() {
    let mut app = TestApp::new();
    let messages = [
        file_message("older", FileKind::Document, None),
        file_message("media", FileKind::Image, Some("caption")),
        text_message("newer", "hello"),
    ];

    let offsets = |app: &mut App<'_>| {
        messages
            .iter()
            .map(|message| height(message, 20, false, app))
            .scan(0, |offset, height| {
                *offset += height;
                Some(*offset)
            })
            .collect::<Vec<_>>()
    };

    app.metadata
        .insert("media".into(), Metadata::File(FileMeta::Loading));
    let before = offsets(&mut app);
    let before_anchor = before[1];
    let before_visible = before
        .iter()
        .enumerate()
        .filter(|(_, bottom)| **bottom > before_anchor.saturating_sub(2))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();

    app.metadata
        .insert("media".into(), Metadata::File(FileMeta::Loaded));
    let after = offsets(&mut app);
    let after_anchor = after[1];
    let after_visible = after
        .iter()
        .enumerate()
        .filter(|(_, bottom)| **bottom > after_anchor.saturating_sub(2))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();

    assert_eq!(before, after);
    assert_eq!(before_anchor, after_anchor);
    assert_eq!(before_visible, after_visible);
    assert_eq!(messages[1].info.id.as_ref(), "media");
}

#[test]
fn cache_is_bounded_and_evicted_messages_reload() {
    let mut app = TestApp::new();
    for index in 0..=MESSAGE_HEIGHT_CACHE_CAPACITY {
        let message = text_message(&format!("message-{index}"), "cached");
        height(&message, 20, false, &mut app);
    }

    assert_eq!(
        app.message_height_cache.len(),
        MESSAGE_HEIGHT_CACHE_CAPACITY
    );
    assert!(!app.message_height_cache.contains("message-0"));

    let evicted = text_message("message-0", "cached");
    assert_eq!(height(&evicted, 20, false, &mut app), 2);
    assert!(app.message_height_cache.contains("message-0"));
}

#[test]
fn retaining_current_messages_drops_removed_entries_and_replacements_refresh_in_place() {
    let mut app = TestApp::new();
    let removed = text_message("removed", "old");
    let mut replaced = text_message("replaced", "old");
    height(&removed, 20, false, &mut app);
    height(&replaced, 20, false, &mut app);

    app.message_height_cache
        .retain_messages(std::slice::from_ref(&replaced.info.id));
    replaced.message = MessageContent::Text(Arc::from("a message that wraps"));

    assert!(!app.message_height_cache.contains(&removed.info.id));
    assert_eq!(height(&replaced, 8, false, &mut app), 5);
    assert_eq!(app.message_height_cache.len(), 1);
}

#[test]
fn continuation_selection_expansion_and_collapse_cannot_reuse_stale_heights() {
    let mut app = TestApp::new();
    let message = text_message("selected", "1234567890");

    assert_eq!(
        message_height(
            &message,
            10,
            false,
            AuthorGroupContext::CONTINUATION,
            &mut app,
        ),
        1
    );
    assert_eq!(
        message_height(
            &message,
            10,
            true,
            AuthorGroupContext::CONTINUATION,
            &mut app,
        ),
        4
    );
    assert_eq!(
        message_height(
            &message,
            10,
            false,
            AuthorGroupContext::CONTINUATION,
            &mut app,
        ),
        1
    );
}

#[test]
fn neighboring_author_context_cannot_reuse_stale_height() {
    let mut app = TestApp::new();
    let message = text_message("neighbor-sensitive", "short");

    assert_eq!(
        message_height(
            &message,
            20,
            false,
            AuthorGroupContext::STARTS_GROUP,
            &mut app,
        ),
        2
    );
    assert_eq!(
        message_height(
            &message,
            20,
            false,
            AuthorGroupContext::CONTINUATION,
            &mut app,
        ),
        1
    );
}

fn app_with_database(path: &std::path::Path) -> TestApp {
    TestApp::with_database(path)
}

fn height(message: &Message, width: usize, is_selected: bool, app: &mut App<'_>) -> usize {
    message_height(
        message,
        width,
        is_selected,
        AuthorGroupContext::STARTS_GROUP,
        app,
    )
}

fn text_message(id: &str, text: &str) -> Message {
    Message {
        info: message_info(id),
        message: MessageContent::Text(text.into()),
    }
}

fn file_message(id: &str, kind: FileKind, caption: Option<&str>) -> Message {
    Message {
        info: message_info(id),
        message: MessageContent::File(whatsrust::FileContent {
            kind,
            path: format!("{id}.png").into(),
            file_id: "file-id".into(),
            caption: caption.map(Into::into),
        }),
    }
}

fn message_info(id: &str) -> MessageInfo {
    let chat = JID::from("chat@example.test".to_owned());
    MessageInfo {
        id: id.into(),
        chat: chat.clone(),
        sender: chat,
        timestamp: 0,
        is_from_me: false,
        quote_id: None,
        read_by: 0,
        forwarding: Default::default(),
    }
}
