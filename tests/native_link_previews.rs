//! T1 contract: an owned native adjunct, consumed once at ingestion, then
//! app-owned and persisted. These new public names intentionally give structural
//! RED on the base; do not add a second test-only ingestion implementation.
use std::ops::Range;

use tempfile::tempdir;
use whatsrust as wr;
use wp_tui::url::extract_openable_urls;

mod common;
use common::TestApp;

const URL: &str = "https://example.test/article?q=%2F#part";
const TITLE_LIMIT: usize = 2_048;
const DESCRIPTION_LIMIT: usize = 8_192;
const THUMBNAIL_LIMIT: usize = 256 * 1_024;

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

fn preview() -> wr::NativeLinkPreview {
    wr::NativeLinkPreview {
        matched_text: URL.into(),
        title: "Native title".into(),
        description: "Native description".into(),
        // Deliberately not a decodable image: T1 transports bytes, not pixels.
        jpeg_thumbnail: vec![0xff, 0xd8, 0xff, 0xd9],
    }
}

fn ingest(app: &mut TestApp, message: wr::Message, preview: Option<wr::NativeLinkPreview>) {
    if let Some(preview) = preview {
        wr::store_native_link_preview(&message.info, body(&message), preview);
    }
    // Expose the existing process_message method; sync avoids notifications and
    // still follows the actual chat-store transaction and ingestion path.
    app.process_message(message.clone(), true);
    assert!(wr::take_native_link_preview(&message.info, body(&message)).is_none());
}

fn body(message: &wr::Message) -> &str {
    match &message.message {
        wr::MessageContent::Text(body) => body,
        _ => panic!("fixture must remain MessageContent::Text"),
    }
}

fn assert_source(app: &TestApp, id: &str, expected: &str, destinations: &[&str]) {
    assert_eq!(body(&app.messages[id]), expected);
    assert_eq!(extract_openable_urls(body(&app.messages[id])), destinations);
}

fn assert_preview(app: &TestApp, id: &str, range: Range<usize>, expected: &wr::NativeLinkPreview) {
    let (actual_range, actual) = app.native_link_preview(&id.into()).expect("native preview");
    assert_eq!(actual_range, range);
    assert_eq!(&body(&app.messages[id])[actual_range], URL);
    assert_eq!(actual, expected);
}

#[test]
fn callback_adjunct_is_owned_chat_scoped_source_bound_and_consumed_once() {
    let first = message("adjunct-key", URL);
    let mut second = first.clone();
    second.info.chat = wr::JID::from("other@example.test".to_owned());
    let mut other = preview();
    other.title = "Other chat".into();
    wr::store_native_link_preview(&first.info, URL, preview());
    wr::store_native_link_preview(&second.info, URL, other.clone());
    assert_eq!(
        wr::take_native_link_preview(&first.info, URL),
        Some(preview())
    );
    assert!(wr::take_native_link_preview(&first.info, URL).is_none());
    assert_eq!(wr::take_native_link_preview(&second.info, URL), Some(other));
    wr::store_native_link_preview(&first.info, URL, preview());
    assert!(wr::take_native_link_preview(&first.info, "changed body").is_none());
    assert!(wr::take_native_link_preview(&first.info, URL).is_none());
}

#[test]
fn incoming_native_fields_round_trip_through_app_ingestion_and_hydration() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("native.db");
    let source = format!("原文 ({URL}).");
    let original = message("round-trip", &source);
    let native = preview();
    {
        let mut app = TestApp::with_database(&path);
        ingest(&mut app, original.clone(), Some(native.clone()));
        assert_source(&app, "round-trip", &source, &[URL]);
        // UTF-8 prefix + punctuation: binding must reuse the extractor's actual
        // byte range, not a reconstructed/normalized URL or a display title.
        assert_preview(&app, "round-trip", 8..8 + URL.len(), &native);
    }
    assert!(wr::take_native_link_preview(&original.info, &source).is_none());
    let mut reloaded = TestApp::with_database(&path);
    reloaded.load_data_from_db();
    assert_source(&reloaded, "round-trip", &source, &[URL]);
    assert_preview(&reloaded, "round-trip", 8..8 + URL.len(), &native);
}

#[test]
fn ordinary_outgoing_and_legacy_public_writes_keep_text_without_metadata() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("plain.db");
    {
        let mut app = TestApp::with_database(&path);
        ingest(&mut app, message("ordinary", URL), None);
        let mut outgoing = message("outgoing", URL);
        outgoing.info.is_from_me = true;
        ingest(&mut app, outgoing, Some(preview()));
        // Existing write callers need not supply a new argument or adjunct.
        app.db_handler.add_message(&message("legacy-writer", URL));
        for id in ["ordinary", "outgoing"] {
            assert_source(&app, id, URL, &[URL]);
            assert!(app.native_link_preview(&id.into()).is_none());
        }
    }
    let mut app = TestApp::with_database(&path);
    app.load_data_from_db();
    for id in ["ordinary", "outgoing", "legacy-writer"] {
        assert_source(&app, id, URL, &[URL]);
        assert!(app.native_link_preview(&id.into()).is_none());
    }
    assert!(app.messages["outgoing"].info.is_from_me);
}

#[test]
fn old_database_without_native_metadata_still_hydrates() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("old.db");
    // SQL is fixture setup only: all assertions go through public hydration.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE chats (jid TEXT PRIMARY KEY);
             CREATE TABLE contacts (jid TEXT PRIMARY KEY, name TEXT NOT NULL);
             CREATE TABLE text_messages (id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER, quote_id TEXT, is_from_me INTEGER, read INTEGER, message TEXT);
             INSERT INTO chats VALUES ('chat@example.test');
             INSERT INTO text_messages VALUES ('old', 'chat@example.test', 'chat@example.test', 1, NULL, 0, 0, 'old https://example.test');",
        )
        .unwrap();
    let mut app = TestApp::with_database(&path);
    app.load_data_from_db();
    assert_source(
        &app,
        "old",
        "old https://example.test",
        &["https://example.test"],
    );
    assert!(app.native_link_preview(&"old".into()).is_none());
}

#[test]
fn invalid_metadata_cannot_replace_source_or_destination_or_another_stored_message() {
    let mut cases = Vec::new();
    let mut mismatched = preview();
    mismatched.matched_text = "https://other.example.test".into();
    cases.push(("mismatch", URL.to_owned(), mismatched, vec![URL]));
    for source in [
        "ftp://example.test/a",
        "javascript:alert(1)",
        "xhttps://example.test/a",
    ] {
        let mut native = preview();
        native.matched_text = source.into();
        cases.push(("unsupported", source.to_owned(), native, vec![]));
    }
    for control in [
        "\u{1b}[31mforged",
        "bad\0title",
        "bad\u{202e}title",
        "bad\u{7}title",
    ] {
        let mut native = preview();
        native.title = control.into();
        cases.push(("title-control", URL.to_owned(), native, vec![URL]));
        let mut native = preview();
        native.description = control.into();
        cases.push(("description-control", URL.to_owned(), native, vec![URL]));
    }
    let mut title = preview();
    title.title = "t".repeat(TITLE_LIMIT + 1).into();
    cases.push(("oversize-title", URL.to_owned(), title, vec![URL]));
    let mut description = preview();
    description.description = "d".repeat(DESCRIPTION_LIMIT + 1).into();
    cases.push((
        "oversize-description",
        URL.to_owned(),
        description,
        vec![URL],
    ));

    for (index, (rule, source, invalid, destinations)) in cases.into_iter().enumerate() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rejected.db");
        let stable_id = format!("stable-{index}");
        let rejected_id = format!("rejected-{index}");
        {
            let mut app = TestApp::with_database(&path);
            ingest(&mut app, message(&stable_id, URL), Some(preview()));
            ingest(&mut app, message(&rejected_id, &source), Some(invalid));
            assert_source(&app, &rejected_id, &source, &destinations);
            assert!(
                app.native_link_preview(&rejected_id.clone().into())
                    .is_none(),
                "{rule}"
            );
            assert_source(&app, &stable_id, URL, &[URL]);
            assert_preview(&app, &stable_id, 0..URL.len(), &preview());
        }
        let mut app = TestApp::with_database(&path);
        app.load_data_from_db();
        assert_eq!(app.messages.len(), 2, "{rule}");
        assert_source(&app, &rejected_id, &source, &destinations);
        assert!(
            app.native_link_preview(&rejected_id.into()).is_none(),
            "{rule}"
        );
        assert_source(&app, &stable_id, URL, &[URL]);
        assert_preview(&app, &stable_id, 0..URL.len(), &preview());
    }
}

#[test]
fn metadata_bounds_and_missing_or_oversized_image_degrade_to_text_only() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bounds.db");
    let mut bounded = preview();
    bounded.title = "t".repeat(TITLE_LIMIT).into();
    bounded.description = "d".repeat(DESCRIPTION_LIMIT).into();
    bounded.jpeg_thumbnail = vec![1; THUMBNAIL_LIMIT];
    let mut text_only = preview();
    text_only.jpeg_thumbnail.clear();
    let mut oversized = text_only.clone();
    oversized.jpeg_thumbnail = vec![1; THUMBNAIL_LIMIT + 1];
    {
        let mut app = TestApp::with_database(&path);
        for (id, native) in [
            ("bounded", bounded.clone()),
            ("missing", text_only.clone()),
            ("oversized", oversized),
        ] {
            ingest(&mut app, message(id, URL), Some(native));
        }
        assert_preview(&app, "bounded", 0..URL.len(), &bounded);
        for id in ["missing", "oversized"] {
            assert_source(&app, id, URL, &[URL]);
            assert_preview(&app, id, 0..URL.len(), &text_only);
        }
    }
    let mut app = TestApp::with_database(&path);
    app.load_data_from_db();
    assert_preview(&app, "bounded", 0..URL.len(), &bounded);
    for id in ["missing", "oversized"] {
        assert_source(&app, id, URL, &[URL]);
        assert_preview(&app, id, 0..URL.len(), &text_only);
    }
}

#[test]
fn unicode_metadata_survives_legacy_unchanged_writes_but_not_source_overwrites() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy-update.db");
    let mut native = preview();
    native.title = "标题 🙂 שלום".into();
    native.description = "وصف طبيعي 日本語".into();
    {
        let mut app = TestApp::with_database(&path);
        let original = message("legacy-update", URL);
        ingest(&mut app, original.clone(), Some(native.clone()));
        app.db_handler.add_message(&original);
        assert_preview(&app, "legacy-update", 0..URL.len(), &native);
    }
    {
        let mut app = TestApp::with_database(&path);
        app.load_data_from_db();
        assert_preview(&app, "legacy-update", 0..URL.len(), &native);
        app.db_handler.add_message(&message(
            "legacy-update",
            "changed https://other.example.test",
        ));
    }
    let mut app = TestApp::with_database(&path);
    app.load_data_from_db();
    assert_source(
        &app,
        "legacy-update",
        "changed https://other.example.test",
        &["https://other.example.test"],
    );
    assert!(app.native_link_preview(&"legacy-update".into()).is_none());
}

#[test]
fn ineligible_status_and_nontext_consume_adjunct_without_retaining_preview() {
    let mut app = TestApp::new();
    let mut status = message("native-status", URL);
    status.info.chat = wr::JID::from("status@broadcast".to_owned());
    ingest(&mut app, status, Some(preview()));
    assert!(app.native_link_preview(&"native-status".into()).is_none());
    assert_source(&app, "native-status", URL, &[URL]);

    let original = message("native-nontext", URL);
    wr::store_native_link_preview(&original.info, URL, preview());
    let mut nontext = original.clone();
    nontext.message = wr::MessageContent::ViewOnceUnavailable;
    assert!(!app.process_message(nontext, true));
    assert!(wr::take_native_link_preview(&original.info, URL).is_none());
    assert!(app.native_link_preview(&original.info.id).is_none());
    assert!(matches!(
        app.messages["native-nontext"].message,
        wr::MessageContent::ViewOnceUnavailable
    ));
}

/// Both legacy text and native-preview ingestion enqueue a chat/message write
/// before action projection enqueues ordinary message writebacks. The writer
/// must preserve that order even after pending replacements become markers.
#[test]
fn queued_ingestion_and_edit_writebacks_keep_latest_text_with_or_without_preview() {
    let expected = "latest https://latest.example.test/path?q=%2F#part";
    let destinations = ["https://latest.example.test/path?q=%2F#part"];
    for native in [None, Some(preview())] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("queued-edits.db");
        {
            let mut app = TestApp::with_database(&path);
            let original = message("queued-edit", URL);
            ingest(&mut app, original.clone(), native.clone());
            assert_source(&app, "queued-edit", URL, &[URL]);
            if let Some(native) = &native {
                assert_preview(&app, "queued-edit", 0..URL.len(), native);
            } else {
                assert!(app.native_link_preview(&"queued-edit".into()).is_none());
            }
            for (action_id, replacement, occurred_at, arrival_order) in [
                ("latest-edit", expected, 3, 2),
                ("older-edit", "older https://older.example.test", 2, 1),
            ] {
                assert!(app.handle_whatsapp_event(wr::Event::MessageAction {
                    action_id: action_id.into(),
                    target_message_id: original.info.id.clone(),
                    chat: original.info.chat.clone(),
                    sender: original.info.sender.clone(),
                    kind: wr::MessageActionKind::Edit {
                        replacement: replacement.into(),
                    },
                    occurred_at,
                    arrival_order,
                }));
                assert_source(&app, "queued-edit", expected, &destinations);
                assert!(app.native_link_preview(&"queued-edit".into()).is_none());
                assert!(app.message_status(&"queued-edit".into()).edited);
            }
        }
        let mut app = TestApp::with_database(&path);
        app.load_data_from_db();
        assert_source(&app, "queued-edit", expected, &destinations);
        assert!(app.native_link_preview(&"queued-edit".into()).is_none());
        assert!(app.message_status(&"queued-edit".into()).edited);
        let actions = app.db_handler.get_message_actions();
        assert_eq!(actions.len(), 2);
        assert!(
            actions.iter().all(|action| matches!(
                &action.kind,
                wp_tui::app::MessageActionKind::Edit { replacement } if replacement.is_empty()
            )),
            "materialized edits must not retain replacement bodies"
        );
    }
}

#[test]
fn edit_and_delete_remove_stale_native_metadata_in_memory_and_after_restart() {
    for (id, kind, expected) in [
        (
            "edited",
            wr::MessageActionKind::Edit {
                replacement: "replacement https://new.example.test".into(),
            },
            "replacement https://new.example.test",
        ),
        (
            "deleted",
            wr::MessageActionKind::Delete,
            "This message was deleted.",
        ),
    ] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("actions.db");
        {
            let mut app = TestApp::with_database(&path);
            let original = message(id, URL);
            ingest(&mut app, original.clone(), Some(preview()));
            assert_preview(&app, id, 0..URL.len(), &preview());
            assert!(app.handle_whatsapp_event(wr::Event::MessageAction {
                action_id: format!("action-{id}").into(),
                target_message_id: original.info.id,
                chat: original.info.chat,
                sender: original.info.sender,
                kind,
                occurred_at: 2,
                arrival_order: 1,
            }));
            assert_eq!(body(&app.messages[id]), expected);
            assert!(app.native_link_preview(&id.into()).is_none());
        }
        let mut app = TestApp::with_database(&path);
        app.load_data_from_db();
        assert_eq!(body(&app.messages[id]), expected);
        assert!(app.native_link_preview(&id.into()).is_none());
    }
}

#[test]
fn edit_before_target_keeps_effective_text_and_no_preview_after_restart() {
    let expected = "edited https://new.example.test/path?q=%2F#part";
    let destinations = ["https://new.example.test/path?q=%2F#part"];
    for native in [None, Some(preview())] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("edit-before-target.db");
        let original = message("pending-target", URL);
        {
            let mut app = TestApp::with_database(&path);
            assert!(app.handle_whatsapp_event(wr::Event::MessageAction {
                action_id: "pending-edit".into(),
                target_message_id: original.info.id.clone(),
                chat: original.info.chat.clone(),
                sender: original.info.sender.clone(),
                kind: wr::MessageActionKind::Edit {
                    replacement: expected.into(),
                },
                occurred_at: 2,
                arrival_order: 1,
            }));
            assert!(!app.messages.contains_key(&original.info.id));
            ingest(&mut app, original.clone(), native);
            assert_source(&app, "pending-target", expected, &destinations);
            assert!(app.message_status(&original.info.id).edited);
            assert!(!app.message_status(&original.info.id).deleted);
            assert!(app.native_link_preview(&original.info.id).is_none());
        }
        let mut app = TestApp::with_database(&path);
        app.load_data_from_db();
        assert_source(&app, "pending-target", expected, &destinations);
        assert!(app.message_status(&original.info.id).edited);
        assert!(!app.message_status(&original.info.id).deleted);
        assert!(app.native_link_preview(&original.info.id).is_none());
        let actions = app.db_handler.get_message_actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action_id.as_ref(), "pending-edit");
        assert!(
            matches!(&actions[0].kind, wp_tui::app::MessageActionKind::Edit { replacement } if replacement.is_empty())
        );
    }
}
