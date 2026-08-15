use std::sync::Arc;

use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color};
use whatsrust::{FileContent, FileKind, JID, Message, MessageContent, MessageInfo};
use wp_tui::{
    app::{App, FileMeta, MessageAction, MessageActionKind, Metadata},
    ui::message_list::{AuthorGroupContext, message_height, render_messages},
};
mod common;
use common::TestApp;

#[test]
fn selected_message_renders_as_a_complete_rounded_card() {
    let mut app = app_with_messages(&[
        text_message("older", "plain message"),
        text_message("selected", "selected body"),
    ]);
    app.message_list_state
        .set_selected_message("selected".into());

    let rows = render_rows(&mut app, 32, 12);

    assert!(rows.iter().any(|row| row.contains("╭─")));
    assert!(rows.iter().any(|row| row.contains("│ selected body")));
    assert!(rows.iter().any(|row| row.contains("╰─")));
    assert!(rows.iter().any(|row| row.contains("plain message")));
    assert!(!rows.iter().any(|row| row.contains("│ plain message")));
}

#[test]
fn selected_card_contains_quote_reactions_and_closing_edges() {
    let quoted = text_message("quoted", "quoted content");
    let mut selected = text_message("selected", "reply body");
    selected.info.quote_id = Some(quoted.info.id.clone());
    let mut app = app_with_messages(&[quoted, selected]);
    app.reactions.insert(
        "selected".into(),
        [(JID::from("alice@example.test".to_owned()), Arc::from("👍"))]
            .into_iter()
            .collect(),
    );
    app.message_list_state
        .set_selected_message("selected".into());

    let rows = render_rows(&mut app, 40, 14);

    assert!(rows.iter().any(|row| row.contains("│ >")));
    assert!(rows.iter().any(|row| row.contains("│ reply body")));
    assert!(
        rows.iter()
            .any(|row| row.contains("│ [👍") && row.contains("1]"))
    );
    assert!(rows.iter().any(|row| row.trim_end().ends_with('│')));
}

#[test]
fn selected_loaded_media_card_reserves_the_preview_and_caption() {
    let message = file_message("image", "caption below preview");
    let mut app = app_with_messages(&[message]);
    app.metadata
        .insert("image".into(), Metadata::File(FileMeta::Loaded));
    app.message_list_state.set_selected_message("image".into());

    let rows = render_rows(&mut app, 44, 22);
    let top = rows.iter().position(|row| row.contains("╭─")).unwrap();
    let bottom = rows.iter().position(|row| row.contains("╰─")).unwrap();

    assert!(bottom - top >= 14);
    assert!(
        rows[top + 1..bottom]
            .iter()
            .all(|row| row.contains("│ ") && row.trim_end().ends_with('│'))
    );
    assert!(rows.iter().any(|row| row.contains("caption below preview")));
}

#[test]
fn selected_card_handles_compact_content_and_narrow_widths() {
    for width in 0..=6 {
        let mut app = app_with_messages(&[text_message("compact", "")]);
        app.message_list_state
            .set_selected_message("compact".into());

        let rows = render_rows(&mut app, width, 8);

        if width >= 4 {
            assert!(rows.iter().any(|row| row.contains('╭')));
            assert!(rows.iter().any(|row| row.contains('╰')));
        }
    }
}

#[test]
fn edited_status_is_inline_and_uses_metadata_style() {
    let message = text_message("edited", "body");
    let mut app = app_with_messages(&[message]);
    add_edit(&mut app, "edited", "body");

    let buffer = render_buffer(&mut app, 32, 8);
    let (y, row) = (0..8)
        .map(|y| {
            (
                y,
                (0..32).map(|x| buffer[(x, y)].symbol()).collect::<String>(),
            )
        })
        .find(|(_, row)| row.contains("body (edited)"))
        .expect("body and edited label should share a row");
    let status_x = row.find("(edited)").unwrap() as u16;
    let body_x = row.find("body").unwrap() as u16;

    assert_eq!(buffer[(status_x, y)].fg, Color::DarkGray);
    assert_ne!(buffer[(body_x, y)].fg, Color::DarkGray);
}

#[test]
fn deleted_status_renders_only_deleted_text() {
    let message = text_message("deleted", "this is the deleted message");
    let mut app = app_with_messages(&[message]);
    add_edit(&mut app, "deleted", "this is the deleted message");
    app.message_actions
        .get_mut("deleted")
        .unwrap()
        .push(MessageAction {
            action_id: "delete".into(),
            target_message_id: "deleted".into(),
            chat: JID::from("chat@example.test".to_owned()),
            sender: JID::from("chat@example.test".to_owned()),
            kind: MessageActionKind::Delete,
            occurred_at: 2,
            arrival_order: 2,
        });

    let rows = render_rows(&mut app, 48, 8);
    assert!(
        rows.iter()
            .any(|row| row.contains("This message was deleted."))
    );
    assert!(!rows.iter().any(|row| row.contains("(edited)")));
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("this is the deleted message"))
    );
}

#[test]
fn inline_status_wraps_with_the_cached_selected_card_height() {
    let message = text_message("narrow", "bodybody");
    let mut app = app_with_messages(&[message.clone()]);
    add_edit(&mut app, "narrow", "bodybody");
    app.message_list_state.set_selected_message("narrow".into());

    assert_eq!(
        message_height(
            &message,
            12,
            true,
            AuthorGroupContext::STARTS_GROUP,
            &mut app,
        ),
        4
    );
    assert!(app.message_height_cache.contains("narrow"));
    let rows = render_rows(&mut app, 12, 8);
    assert!(rows.iter().any(|row| row.contains("bodybody")));
    assert!(rows.iter().any(|row| row.contains("(edited)")));
    assert!(rows.iter().any(|row| row.contains('╰')));
}

#[test]
fn unicode_content_never_panics_and_height_matches_the_selected_card() {
    let cases = [
        "Y mío funciona? (editado prueba 2) (una vez más)",
        "café déjà vu",
        "emoji 👩‍💻 and family 👨‍👩‍👧‍👦",
        "combining e\u{301} and a\u{308}",
        "CJK 漢字かなカナ",
    ];

    for body in cases {
        for width in 0..=12 {
            let message = text_message("unicode", body);
            let mut app = app_with_messages(std::slice::from_ref(&message));
            add_edit(&mut app, "unicode", body);
            app.message_list_state
                .set_selected_message("unicode".into());

            let height = message_height(
                &message,
                width,
                true,
                AuthorGroupContext::STARTS_GROUP,
                &mut app,
            );
            let rows = render_rows(&mut app, width as u16, height.max(1) as u16);

            assert!(height >= 3, "body={body:?}, width={width}");
            if width >= 4 {
                assert!(
                    rows.iter().any(|row| row.contains('╰')),
                    "body={body:?}, width={width}"
                );
            }
        }
    }
}

#[test]
fn media_without_a_caption_uses_a_compact_status_line() {
    let message = Message {
        info: message_info("media"),
        message: MessageContent::File(FileContent {
            kind: FileKind::Document,
            path: "document.pdf".into(),
            file_id: "file-id".into(),
            caption: None,
        }),
    };
    let mut app = app_with_messages(&[message]);
    add_edit(&mut app, "media", "ignored");

    let rows = render_rows(&mut app, 32, 8);
    assert!(rows.iter().any(|row| row.contains("document.pdf")));
    assert!(rows.iter().any(|row| row.contains("(edited)")));
}

#[test]
fn moving_selection_moves_the_card_without_stale_layout() {
    let mut app = app_with_messages(&[
        text_message("older", "older body"),
        text_message("newer", "newer body"),
    ]);
    app.message_list_state.set_selected_message("newer".into());

    let first = render_rows(&mut app, 34, 12);
    assert!(first.iter().any(|row| row.contains("│ newer body")));
    assert!(!first.iter().any(|row| row.contains("│ older body")));

    app.message_list_state.select_next();
    let second = render_rows(&mut app, 34, 12);
    assert!(second.iter().any(|row| row.contains("│ older body")));
    assert!(!second.iter().any(|row| row.contains("│ newer body")));
}

#[test]
fn selected_message_scrolls_viewport_to_keep_the_card_with_padding() {
    let mut app = app_with_messages(&[
        text_message("first", "first body"),
        text_message("second", "second body"),
        text_message("third", "third body"),
        text_message("fourth", "fourth body"),
        text_message("fifth", "fifth body"),
    ]);
    app.message_list_state.set_selected_message("third".into());
    app.message_list_state.offset = 6;

    let first_rows = render_rows(&mut app, 34, 8);

    assert_eq!(app.message_list_state.offset, 0);
    assert!(first_rows.iter().any(|row| row.contains("│ third body")));

    app.message_list_state.set_selected_message("first".into());
    app.message_list_state.offset = 0;

    let third_rows = render_rows(&mut app, 34, 8);

    assert_eq!(app.message_list_state.offset, 4);
    assert!(third_rows.iter().any(|row| row.contains("│ first body")));
}

fn render_rows(app: &mut App<'_>, width: u16, height: u16) -> Vec<String> {
    let buffer = render_buffer(app, width, height);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn render_buffer(app: &mut App<'_>, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            render_messages(frame, app, Rect::new(0, 0, width, height));
        })
        .expect("messages should render");
    terminal.backend().buffer().clone()
}

fn add_edit(app: &mut App<'_>, id: &str, replacement: &str) {
    app.message_actions.insert(
        id.into(),
        vec![MessageAction {
            action_id: format!("edit-{id}").into(),
            target_message_id: id.into(),
            chat: JID::from("chat@example.test".to_owned()),
            sender: JID::from("chat@example.test".to_owned()),
            kind: MessageActionKind::Edit {
                replacement: replacement.into(),
            },
            occurred_at: 1,
            arrival_order: 1,
        }],
    );
}

fn app_with_messages(messages: &[Message]) -> TestApp {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    app.contacts.insert(chat.clone(), "Alice".into());
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

fn text_message(id: &str, text: &str) -> Message {
    Message {
        info: message_info(id),
        message: MessageContent::Text(text.into()),
    }
}

fn file_message(id: &str, caption: &str) -> Message {
    Message {
        info: message_info(id),
        message: MessageContent::File(FileContent {
            kind: FileKind::Image,
            path: "image.png".into(),
            file_id: "file-id".into(),
            caption: Some(caption.into()),
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
