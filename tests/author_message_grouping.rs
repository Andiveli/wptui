use std::sync::Arc;
use wp_tui::app::read_receipts::VisibilityPlan;

use chrono::{Local, TimeZone};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use whatsrust::{FileContent, FileKind, JID, Message, MessageContent, MessageInfo};
use wp_tui::{
    app::{App, FileMeta, Metadata, events::MediaRenderPlan},
    ui::message_list::{
        AUTHOR_GROUP_MAX_GAP, IMAGE_HEIGHT, render_messages_with_plan, starts_author_group,
    },
};
mod common;
use common::TestApp;

#[test]
fn first_message_starts_an_author_group() {
    let message = text_message("first", "alice@example.test", 1_700_000_000, false, "First");

    assert!(starts_author_group(None, &message));
}

#[test]
fn same_sender_inside_five_minutes_is_a_continuation() {
    let previous = text_message(
        "previous",
        "alice@example.test",
        1_700_000_000,
        false,
        "First",
    );
    let current = text_message(
        "current",
        "alice@example.test",
        1_700_000_000 + AUTHOR_GROUP_MAX_GAP - 1,
        false,
        "Second",
    );

    assert!(!starts_author_group(Some(&previous), &current));
}

#[test]
fn same_sender_at_zero_gap_is_a_continuation() {
    let previous = text_message(
        "previous",
        "alice@example.test",
        1_700_000_000,
        false,
        "First",
    );
    let current = text_message(
        "current",
        "alice@example.test",
        1_700_000_000,
        false,
        "Second",
    );

    assert!(!starts_author_group(Some(&previous), &current));
}

#[test]
fn out_of_order_timestamp_starts_a_new_group_deterministically() {
    let previous = text_message(
        "previous",
        "alice@example.test",
        1_700_000_001,
        false,
        "First",
    );
    let current = text_message(
        "current",
        "alice@example.test",
        1_700_000_000,
        false,
        "Second",
    );

    assert!(starts_author_group(Some(&previous), &current));
}

#[test]
fn exactly_five_minutes_starts_an_author_group() {
    let previous = text_message(
        "previous",
        "alice@example.test",
        1_700_000_000,
        false,
        "First",
    );
    let current = text_message(
        "current",
        "alice@example.test",
        1_700_000_000 + AUTHOR_GROUP_MAX_GAP,
        false,
        "Second",
    );

    assert!(starts_author_group(Some(&previous), &current));
}

#[test]
fn sender_change_starts_an_author_group() {
    let previous = text_message(
        "previous",
        "alice@example.test",
        1_700_000_000,
        false,
        "First",
    );
    let current = text_message(
        "current",
        "bob@example.test",
        1_700_000_001,
        false,
        "Second",
    );

    assert!(starts_author_group(Some(&previous), &current));
}

#[test]
fn local_day_boundary_starts_an_author_group() {
    let first_day = Local
        .with_ymd_and_hms(2024, 1, 2, 23, 59, 0)
        .single()
        .unwrap();
    let next_day = first_day + chrono::Duration::minutes(2);
    assert_ne!(first_day.date_naive(), next_day.date_naive());
    let previous = text_message(
        "previous",
        "alice@example.test",
        first_day.timestamp(),
        false,
        "First",
    );
    let current = text_message(
        "current",
        "alice@example.test",
        next_day.timestamp(),
        false,
        "Second",
    );

    assert!(starts_author_group(Some(&previous), &current));
}

#[test]
fn self_messages_group_by_self_identity() {
    let previous = text_message(
        "previous",
        "device-one@example.test",
        1_700_000_000,
        true,
        "First",
    );
    let current = text_message(
        "current",
        "device-two@example.test",
        1_700_000_001,
        true,
        "Second",
    );

    assert!(!starts_author_group(Some(&previous), &current));
}

#[test]
fn chronological_predecessor_drives_bottom_up_render_grouping() {
    let messages = [
        text_message(
            "oldest",
            "alice@example.test",
            1_700_000_000,
            false,
            "Oldest body",
        ),
        text_message(
            "middle",
            "alice@example.test",
            1_700_000_001,
            false,
            "Middle body",
        ),
        text_message(
            "newest",
            "bob@example.test",
            1_700_000_002,
            false,
            "Newest body",
        ),
    ];
    let mut app = app_with_messages(&messages);

    let rows = render_rows(&mut app, 48, 14);

    assert_eq!(rows.iter().filter(|row| row.contains("Alice")).count(), 1);
    assert_eq!(rows.iter().filter(|row| row.contains("Bob")).count(), 1);
}

#[test]
fn author_group_boundaries_retain_a_blank_row() {
    let messages = [
        text_message(
            "older",
            "alice@example.test",
            1_700_000_000,
            false,
            "Older line",
        ),
        text_message(
            "newer",
            "bob@example.test",
            1_700_000_001,
            false,
            "Newer line",
        ),
    ];
    let mut app = app_with_messages(&messages);

    let rows = render_rows(&mut app, 48, 12);

    assert_eq!(row_of(&rows, "Older line") + 3, row_of(&rows, "Newer line"));
}

#[test]
fn grouped_continuations_expand_for_selection_and_collapse_after_it_moves() {
    let messages = [
        text_message(
            "oldest",
            "alice@example.test",
            1_700_000_000,
            false,
            "First line",
        ),
        text_message(
            "middle",
            "alice@example.test",
            1_700_000_001,
            false,
            "Second line",
        ),
        text_message(
            "newest",
            "alice@example.test",
            1_700_000_002,
            false,
            "Third line",
        ),
    ];
    let mut app = app_with_messages(&messages);

    let compact = render_rows(&mut app, 48, 16);
    assert_adjacent(&compact, "First line", "Second line");
    assert_adjacent(&compact, "Second line", "Third line");

    app.message_list_state.set_selected_message("middle".into());

    let selected = render_rows(&mut app, 48, 16);
    assert_eq!(
        row_of(&selected, "First line") + 3,
        row_of(&selected, "Second line")
    );
    assert_eq!(
        row_of(&selected, "Second line") + 3,
        row_of(&selected, "Third line")
    );

    app.message_list_state.select_previous();
    let moved = render_rows(&mut app, 48, 16);
    assert_adjacent(&moved, "First line", "Second line");
    assert!(app.message_height_cache.contains("middle"));
}

#[test]
fn selected_grouped_continuation_shows_timestamp_on_bottom_border() {
    let messages = [
        text_message(
            "older",
            "alice@example.test",
            1_700_000_000,
            false,
            "Older body",
        ),
        text_message(
            "selected",
            "alice@example.test",
            1_700_000_061,
            false,
            "Selected body",
        ),
    ];
    let expected_time = Local
        .timestamp_opt(1_700_000_061, 0)
        .single()
        .unwrap()
        .format("%H:%M")
        .to_string();
    let mut app = app_with_messages(&messages);
    app.message_list_state
        .set_selected_message("selected".into());

    let rows = render_rows(&mut app, 40, 12);
    let bottom = rows.iter().find(|row| row.contains('╰')).unwrap();

    assert!(bottom.contains(&expected_time));
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("Alice (") && row.contains(&expected_time))
    );
}

#[test]
fn narrow_selected_continuation_is_bounded_and_shows_timestamp_when_it_fits() {
    let older_timestamp = Local::now()
        .date_naive()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_local_timezone(Local)
        .single()
        .unwrap()
        .timestamp();
    let selected_timestamp = older_timestamp + 61;
    let messages = [
        text_message(
            "older",
            "alice@example.test",
            older_timestamp,
            false,
            "Older",
        ),
        text_message(
            "selected",
            "alice@example.test",
            selected_timestamp,
            false,
            "Selected",
        ),
    ];

    let expected_time = Local
        .timestamp_opt(selected_timestamp, 0)
        .single()
        .unwrap()
        .format("%H:%M")
        .to_string();

    for width in [0, 1, 2, 3, 4, 6, 8, 10, 12, 13, 14, 16, 24] {
        let mut app = app_with_messages(&messages);
        app.message_list_state
            .set_selected_message("selected".into());
        let rows = render_rows(&mut app, width, 8);

        assert_eq!(rows.len(), 8);
        assert!(rows.iter().all(|row| row.chars().count() <= width as usize));
        if width >= 14 {
            assert!(
                rows.iter().any(|row| row.contains(&expected_time)),
                "width {width} should accommodate the selected timestamp"
            );
        }
    }
}

#[test]
fn grouped_continuations_preserve_quote_reactions_and_media() {
    let quoted = text_message(
        "quoted",
        "bob@example.test",
        1_699_999_000,
        false,
        "Quoted content",
    );
    let mut reply = text_message(
        "reply",
        "alice@example.test",
        1_700_000_001,
        false,
        "Reply body",
    );
    reply.info.quote_id = Some(quoted.info.id.clone());
    let image = file_message(
        "image",
        "alice@example.test",
        1_700_000_002,
        "Image caption",
    );
    let older = text_message(
        "older",
        "alice@example.test",
        1_700_000_000,
        false,
        "Older body",
    );
    let mut app = app_with_messages(&[quoted, older, reply, image]);
    app.reactions.insert(
        "reply".into(),
        [(JID::from("reader@example.test".to_owned()), Arc::from("ok"))]
            .into_iter()
            .collect(),
    );
    app.metadata
        .insert("image".into(), Metadata::File(FileMeta::Loaded));

    let rows = render_rows(&mut app, 52, 30);

    assert!(rows.iter().any(|row| row.contains("> Bob: Quoted content")));
    assert!(rows.iter().any(|row| row.contains("Reply body")));
    assert!(rows.iter().any(|row| row.contains("[ok 1]")));
    assert!(rows.iter().any(|row| row.contains("Image caption")));
    assert_eq!(rows.iter().filter(|row| row.contains("Alice")).count(), 1);
    let preview_row = rows.iter().position(|row| row.contains('🖼')).unwrap();
    let caption_row = rows
        .iter()
        .position(|row| row.contains("Image caption"))
        .unwrap();
    assert_eq!(caption_row - preview_row, IMAGE_HEIGHT);
    assert_adjacent(&rows, "Older body", "> Bob: Quoted content");
    assert_adjacent(&rows, "[ok 1]", "🖼");
}

#[test]
fn edited_and_reordered_neighbors_recompute_grouping_after_cache_population() {
    let messages = [
        text_message(
            "oldest",
            "alice@example.test",
            1_700_000_000,
            false,
            "Oldest",
        ),
        text_message(
            "middle",
            "alice@example.test",
            1_700_000_001,
            false,
            "Middle",
        ),
        text_message(
            "newest",
            "alice@example.test",
            1_700_000_002,
            false,
            "Newest",
        ),
    ];
    let mut app = app_with_messages(&messages);

    let initial = render_rows(&mut app, 48, 16);
    assert_eq!(
        initial.iter().filter(|row| row.contains("Alice")).count(),
        1
    );

    app.messages.get_mut("middle").unwrap().info.sender = JID::from("bob@example.test".to_owned());
    app.invalidate_message_sequence_for_test(&JID::from("chat@example.test".to_owned()));
    let edited = render_rows(&mut app, 48, 16);
    assert_eq!(edited.iter().filter(|row| row.contains("Alice")).count(), 2);
    assert_eq!(edited.iter().filter(|row| row.contains("Bob")).count(), 1);

    let chat = JID::from("chat@example.test".to_owned());
    app.chat_messages.insert(
        chat,
        vec!["middle".into(), "oldest".into(), "newest".into()],
    );
    app.invalidate_message_sequence_for_test(&JID::from("chat@example.test".to_owned()));
    let reordered = render_rows(&mut app, 48, 16);
    assert_eq!(
        reordered.iter().filter(|row| row.contains("Alice")).count(),
        1
    );
    assert_eq!(
        reordered.iter().filter(|row| row.contains("Bob")).count(),
        1
    );
}

fn render_rows(app: &mut App<'_>, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            let mut visibility_plan = VisibilityPlan::default();
            render_messages_with_plan(
                frame,
                app,
                &mut media_render_plan,
                &mut visibility_plan,
                Rect::new(0, 0, width, height),
            );
        })
        .expect("messages should render");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

fn assert_adjacent(rows: &[String], older: &str, newer: &str) {
    assert_eq!(row_of(rows, older) + 1, row_of(rows, newer));
}

fn row_of(rows: &[String], content: &str) -> usize {
    rows.iter()
        .position(|row| row.contains(content))
        .unwrap_or_else(|| panic!("rendered buffer should contain {content:?}"))
}

fn app_with_messages(messages: &[Message]) -> TestApp {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    app.contacts
        .insert(JID::from("alice@example.test".to_owned()), "Alice".into());
    app.contacts
        .insert(JID::from("bob@example.test".to_owned()), "Bob".into());
    for message in messages {
        app.chat_messages
            .entry(chat.clone())
            .or_default()
            .push(message.info.id.clone());
        app.messages
            .insert(message.info.id.clone(), message.clone());
    }
    app.sorted_chats.push(chat.clone());
    app.open_chat = Some(chat);
    app.chat_list_state.select(Some(0));
    app
}

fn text_message(id: &str, sender: &str, timestamp: i64, is_from_me: bool, text: &str) -> Message {
    Message {
        info: message_info(id, sender, timestamp, is_from_me),
        message: MessageContent::Text(text.into()),
    }
}

fn file_message(id: &str, sender: &str, timestamp: i64, caption: &str) -> Message {
    Message {
        info: message_info(id, sender, timestamp, false),
        message: MessageContent::File(FileContent {
            kind: FileKind::Image,
            path: "image.png".into(),
            file_id: "file-id".into(),
            caption: Some(caption.into()),
        }),
    }
}

fn message_info(id: &str, sender: &str, timestamp: i64, is_from_me: bool) -> MessageInfo {
    MessageInfo {
        id: id.into(),
        chat: JID::from("chat@example.test".to_owned()),
        sender: JID::from(sender.to_owned()),
        mentions_self: false,
        timestamp,
        is_from_me,
        quote_id: None,
        read_by: 0,
        forwarding: Default::default(),
    }
}
