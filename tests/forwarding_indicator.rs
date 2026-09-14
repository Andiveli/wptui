use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use tempfile::tempdir;
use whatsrust::{FileContent, FileKind, ForwardingInfo, JID, Message, MessageContent, MessageInfo};
use wp_tui::app::read_receipts::VisibilityPlan;
use wp_tui::app::{App, events::MediaRenderPlan};
use wp_tui::db::DatabaseHandler;
use wp_tui::ui::message_list::{AuthorGroupContext, message_height, render_messages_with_plan};
mod common;
use common::TestApp;

#[test]
fn forwarding_metadata_round_trips_for_text_and_files() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("forwarding.db");
    let text = message("text", MessageContent::Text("hello".into()), true, 1, false);
    let file = message(
        "file",
        MessageContent::File(FileContent {
            kind: FileKind::Document,
            path: "file.pdf".into(),
            file_id: "file-id".into(),
            caption: Some("report".into()),
        }),
        true,
        5,
        false,
    );

    let mut database = DatabaseHandler::new(&path);
    database.init();
    database.add_message(&text);
    database.add_message(&file);
    database.stop();

    let mut reloaded = DatabaseHandler::new(&path);
    reloaded.init();
    let messages = reloaded.get_messages();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].info.forwarding, text.info.forwarding);
    assert_eq!(messages[1].info.forwarding, file.info.forwarding);
    reloaded.stop();
}

#[test]
fn legacy_rows_default_to_not_forwarded_after_migration() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("legacy-forwarding.db");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE text_messages (id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER, quote_id TEXT, is_from_me INTEGER, read INTEGER, message TEXT);
             CREATE TABLE file_messages (id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER, quote_id TEXT, is_from_me INTEGER, read INTEGER, kind INTEGER, path TEXT, file_id TEXT, caption TEXT);
             INSERT INTO text_messages VALUES ('text', 'chat@example.test', 'chat@example.test', 0, NULL, 0, 0, 'hello');
             INSERT INTO file_messages VALUES ('file', 'chat@example.test', 'chat@example.test', 0, NULL, 0, 0, 3, 'file.pdf', 'file-id', NULL);",
        )
        .unwrap();
    drop(connection);

    let mut database = DatabaseHandler::new(&path);
    database.init();
    let messages = database.get_messages();
    assert!(
        messages
            .iter()
            .all(|message| message.info.forwarding == ForwardingInfo::default())
    );
    database.stop();
}

#[test]
fn forwarding_indicator_changes_text_and_file_height_from_protocol_metadata() {
    let mut app = TestApp::new();
    let plain = message(
        "plain",
        MessageContent::Text("hello".into()),
        false,
        0,
        false,
    );
    let incoming = message(
        "incoming",
        MessageContent::Text("hello".into()),
        true,
        1,
        false,
    );
    let many_times = message(
        "many-times",
        MessageContent::File(FileContent {
            kind: FileKind::Document,
            path: "file.pdf".into(),
            file_id: "file-id".into(),
            caption: Some("report".into()),
        }),
        true,
        5,
        false,
    );
    let outgoing = message(
        "outgoing",
        MessageContent::Text("hello".into()),
        true,
        1,
        true,
    );
    let own_source = message(
        "own-source",
        MessageContent::Text("hello".into()),
        false,
        0,
        true,
    );

    assert_eq!(height(&plain, 30, false, &mut app), 2);
    assert_eq!(height(&incoming, 30, false, &mut app), 3);
    assert_eq!(height(&many_times, 30, false, &mut app), 4);
    assert_eq!(height(&outgoing, 30, false, &mut app), 3);
    assert_eq!(height(&own_source, 30, false, &mut app), 2);
}

#[test]
fn forwarding_indicator_renders_from_protocol_metadata_for_all_directions() {
    let outgoing = message(
        "outgoing",
        MessageContent::Text("hello".into()),
        true,
        1,
        true,
    );
    let outgoing_many_times = message(
        "outgoing-many-times",
        MessageContent::Text("hello".into()),
        true,
        5,
        true,
    );
    let own_source = message(
        "own-source",
        MessageContent::Text("hello".into()),
        false,
        0,
        true,
    );
    let incoming = message(
        "incoming",
        MessageContent::Text("hello".into()),
        true,
        1,
        false,
    );

    assert!(rendered_rows(outgoing).contains("Forwarded"));
    assert!(rendered_rows(outgoing_many_times).contains("Forwarded many times"));
    assert!(!rendered_rows(own_source).contains("Forwarded"));
    assert!(rendered_rows(incoming).contains("Forwarded"));
}

fn height(message: &Message, width: usize, selected: bool, app: &mut App<'_>) -> usize {
    message_height(
        message,
        width,
        selected,
        AuthorGroupContext::STARTS_GROUP,
        app,
    )
}

fn rendered_rows(message: Message) -> String {
    let mut app = TestApp::new();
    let chat = message.info.chat.clone();
    app.chat_messages
        .entry(chat.clone())
        .or_default()
        .push(message.info.id.clone());
    app.messages.insert(message.info.id.clone(), message);
    app.sorted_chats.push(chat.clone());
    app.open_chat = Some(chat);
    app.chat_list_state.select(Some(0));

    let backend = TestBackend::new(40, 8);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            let mut visibility_plan = VisibilityPlan::default();
            render_messages_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                &mut visibility_plan,
                Rect::new(0, 0, 40, 8),
            );
        })
        .expect("message should render");
    let buffer = terminal.backend().buffer();
    (0..8)
        .map(|y| (0..40).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .concat()
}

fn message(
    id: &str,
    content: MessageContent,
    is_forwarded: bool,
    score: u32,
    is_from_me: bool,
) -> Message {
    let chat = JID::from("chat@example.test".to_owned());
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: chat,
            mentions_self: false,
            timestamp: 0,
            is_from_me,
            quote_id: None,
            read_by: 0,
            forwarding: ForwardingInfo {
                is_forwarded,
                score,
            },
        },
        message: content,
    }
}
