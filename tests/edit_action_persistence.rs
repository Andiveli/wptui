use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use tempfile::tempdir;
use whatsrust::{
    Event, JID, Message, MessageActionKind as WireActionKind, MessageContent, MessageInfo,
};
use wp_tui::db::{DatabaseHandler, MessageActionPersistence};
use wp_tui::{
    app::{MessageAction, MessageActionKind},
    ui::message_list::render_messages,
};
mod common;
use common::TestApp;

fn action(
    id: &str,
    target: &str,
    kind: MessageActionKind,
    occurred_at: i64,
    arrival_order: u64,
) -> MessageAction {
    MessageAction {
        action_id: id.into(),
        target_message_id: target.into(),
        chat: JID::from("chat@example.test".to_owned()),
        sender: JID::from("sender@example.test".to_owned()),
        kind,
        occurred_at,
        arrival_order,
    }
}

fn text_message(id: &str, chat: &JID, sender: &JID, body: &str, timestamp: i64) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: sender.clone(),
            mentions_self: false,
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::Text(body.into()),
    }
}

/// Edit action IDs deduplicate across a restart. Loaded edit actions are
/// status markers; the displayed body comes from the current message row.
#[test]
fn edit_action_ids_deduplicate_on_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("actions.db");
    let mut database = DatabaseHandler::new(&path);
    database.init();
    assert_eq!(
        database.record_message_action(&action(
            "edit-later",
            "target",
            MessageActionKind::Edit {
                replacement: "second".into()
            },
            10,
            2,
        )),
        MessageActionPersistence::Inserted
    );
    assert_eq!(
        database.record_message_action(&action(
            "edit-first",
            "target",
            MessageActionKind::Edit {
                replacement: "first".into()
            },
            10,
            1,
        )),
        MessageActionPersistence::Inserted
    );
    assert_eq!(
        database.record_message_action(&action(
            "edit-first",
            "target",
            MessageActionKind::Edit {
                replacement: "duplicate".into()
            },
            10,
            1,
        )),
        MessageActionPersistence::DuplicateActionID
    );
    database.stop();

    let mut restarted = DatabaseHandler::new(&path);
    restarted.init();
    let actions = restarted.get_message_actions();
    assert_eq!(
        actions
            .iter()
            .map(|action| action.action_id.as_ref())
            .collect::<Vec<_>>(),
        ["edit-first", "edit-later"]
    );
    assert!(
        actions.iter().all(|action| matches!(
            &action.kind,
            MessageActionKind::Edit { replacement } if replacement.is_empty()
        )),
        "edit actions persist without replacement bodies"
    );
    restarted.stop();
}

/// Distinct server edits are recorded as markers.
#[test]
fn repeated_server_replacements_with_distinct_action_ids_are_retained() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("actions.db");
    let mut database = DatabaseHandler::new(&path);

    assert_eq!(
        database.record_message_action(&action(
            "server-edit-a-1",
            "target",
            MessageActionKind::Edit {
                replacement: "replacement".into(),
            },
            10,
            1,
        )),
        MessageActionPersistence::Inserted
    );
    assert_eq!(
        database.record_message_action(&action(
            "server-edit-a-2",
            "target",
            MessageActionKind::Edit {
                replacement: "replacement".into(),
            },
            10,
            2,
        )),
        MessageActionPersistence::Inserted
    );
    assert_eq!(database.get_message_actions().len(), 2);
    database.stop();
}

/// Edited messages keep their current effective content across edits and
/// reloads.
#[test]
fn edited_messages_retain_only_current_content_across_edits_and_reload() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("actions.db");
    let chat = JID::from("chat@example.test".to_owned());
    let sender = JID::from("sender@example.test".to_owned());
    let original = text_message("target", &chat, &sender, "before", 1);
    let mut app = TestApp::with_database(&path);
    app.db_handler.add_message(&original);
    app.messages.insert("target".into(), original.clone());
    app.chat_messages
        .entry(chat.clone())
        .or_default()
        .push("target".into());
    app.sorted_chats.push(chat.clone());
    app.open_chat = Some(chat.clone());
    app.chat_list_state.select(Some(0));

    for (action_id, replacement, occurred_at, arrival_order) in [
        ("remote-edit-a-1", "A", 2, 1),
        ("remote-edit-b", "B", 3, 2),
        ("remote-edit-a-2", "A", 4, 3),
    ] {
        assert!(app.handle_whatsapp_event(Event::MessageAction {
            action_id: action_id.into(),
            target_message_id: "target".into(),
            chat: chat.clone(),
            sender: sender.clone(),
            kind: WireActionKind::Edit {
                replacement: replacement.into(),
            },
            occurred_at,
            arrival_order,
        }));
    }

    // The current effective content is exposed while running.
    assert!(
        matches!(&app.messages["target"].message, MessageContent::Text(body) if body.as_ref() == "A")
    );
    assert!(app.message_status(&"target".into()).edited);

    // The rendered conversation shows the current body with the edited label.
    let backend = TestBackend::new(40, 8);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_messages(frame, &mut app, Rect::new(0, 0, 40, 8));
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut rendered = String::new();
    for y in 0..8 {
        for x in 0..40 {
            rendered.push_str(buffer[(x, y)].symbol());
        }
    }
    assert!(rendered.contains("A (edited)"));
    assert!(!rendered.contains("before"));

    app.db_handler.stop();
    let mut reloaded = TestApp::with_database(&path);
    reloaded.load_data_from_db();

    // Reload keeps the current effective body and the edited status.
    assert!(
        matches!(&reloaded.messages["target"].message, MessageContent::Text(body) if body.as_ref() == "A")
    );
    assert!(reloaded.message_status(&"target".into()).edited);
    let actions = reloaded.db_handler.get_message_actions();
    assert_eq!(actions.len(), 3);
    assert!(
        actions.iter().all(|action| matches!(
            &action.kind,
            MessageActionKind::Edit { replacement } if replacement.is_empty()
        )),
        "edit actions persist without replacement bodies"
    );
    reloaded.db_handler.stop();
}

/// Out-of-order server edits still project the current effective body.
#[test]
fn out_of_order_edits_project_current_body() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("actions.db");
    let chat = JID::from("chat@example.test".to_owned());
    let sender = JID::from("sender@example.test".to_owned());
    let original = text_message("target", &chat, &sender, "before", 1);
    let mut app = TestApp::with_database(&path);
    app.db_handler.add_message(&original);
    app.messages.insert("target".into(), original.clone());
    app.chat_messages
        .entry(chat.clone())
        .or_default()
        .push("target".into());
    app.sorted_chats.push(chat.clone());
    app.open_chat = Some(chat.clone());
    app.chat_list_state.select(Some(0));

    // Delivered in the opposite of their stable (occurred_at, arrival_order)
    // order; the sorted replay must still pick the newest effective body.
    for (action_id, replacement, occurred_at, arrival_order) in
        [("remote-edit-b", "B", 3, 2), ("remote-edit-a", "A", 2, 1)]
    {
        assert!(app.handle_whatsapp_event(Event::MessageAction {
            action_id: action_id.into(),
            target_message_id: "target".into(),
            chat: chat.clone(),
            sender: sender.clone(),
            kind: WireActionKind::Edit {
                replacement: replacement.into(),
            },
            occurred_at,
            arrival_order,
        }));
    }

    assert!(
        matches!(&app.messages["target"].message, MessageContent::Text(body) if body.as_ref() == "B")
    );
    app.db_handler.stop();
}

/// Databases may hold a stored replacement body alongside the message row;
/// the migration folds it into the message row and removes the column.
#[test]
fn persisted_replacement_is_migrated_and_not_exposed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("replacement.db");
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE text_messages (
                    id TEXT PRIMARY KEY,
                    chat_jid TEXT,
                    sender_jid TEXT,
                    timestamp INTEGER,
                    quote_id TEXT,
                    is_from_me INTEGER,
                    read INTEGER,
                    message TEXT,
                    is_forwarded INTEGER NOT NULL DEFAULT 0,
                    forwarding_score INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE message_actions (
                    action_id TEXT PRIMARY KEY,
                    target_message_id TEXT NOT NULL,
                    chat_jid TEXT NOT NULL,
                    sender_jid TEXT NOT NULL,
                    kind INTEGER NOT NULL,
                    replacement TEXT,
                    occurred_at INTEGER NOT NULL,
                    arrival_order INTEGER NOT NULL
                );
                INSERT INTO text_messages (id, chat_jid, sender_jid, timestamp, is_from_me, read, message)
                    VALUES ('target', 'chat@example.test', 'sender@example.test', 1, 0, 0, 'original');
                INSERT INTO message_actions (action_id, target_message_id, chat_jid, sender_jid, kind, replacement, occurred_at, arrival_order)
                    VALUES ('edit-a', 'target', 'chat@example.test', 'sender@example.test', 0, 'A', 2, 1);
                INSERT INTO message_actions (action_id, target_message_id, chat_jid, sender_jid, kind, replacement, occurred_at, arrival_order)
                    VALUES ('edit-b', 'target', 'chat@example.test', 'sender@example.test', 0, 'B', 3, 2);",
            )
            .unwrap();
    }

    let mut database = DatabaseHandler::new(&path);
    database.init();

    // The current effective body, not the original, is what loads.
    let messages = database.get_messages();
    let target = messages
        .iter()
        .find(|message| message.info.id.as_ref() == "target")
        .expect("migrated message row must exist");
    assert!(
        matches!(&target.message, MessageContent::Text(body) if body.as_ref() == "B"),
        "migration must fold the current effective body into the message row"
    );

    // Replacement bodies are not exposed through the public API.
    let actions = database.get_message_actions();
    assert_eq!(actions.len(), 2);
    assert!(
        actions.iter().all(|action| matches!(
            &action.kind,
            MessageActionKind::Edit { replacement } if replacement.is_empty()
        )),
        "replacement bodies must not be exposed"
    );

    // The replacement column itself is removed by the migration.
    let connection = rusqlite::Connection::open(&path).unwrap();
    let replacement_columns: i64 = connection
        .prepare(
            "SELECT COUNT(*) FROM pragma_table_info('message_actions') WHERE name = 'replacement'",
        )
        .unwrap()
        .query_row([], |row| row.get(0))
        .unwrap();
    assert_eq!(replacement_columns, 0);
    database.stop();
}

/// A database carrying a persisted delete action (kind 1) must load the
/// deleted message: the delete marker deterministically purges the stored
/// body and any file row for the deleted message.
#[test]
fn persisted_delete_action_marks_the_message_deleted() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("delete-action.db");
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE text_messages (
                    id TEXT PRIMARY KEY,
                    chat_jid TEXT,
                    sender_jid TEXT,
                    timestamp INTEGER,
                    quote_id TEXT,
                    is_from_me INTEGER,
                    read INTEGER,
                    message TEXT,
                    is_forwarded INTEGER NOT NULL DEFAULT 0,
                    forwarding_score INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE file_messages (
                    id TEXT PRIMARY KEY,
                    chat_jid TEXT,
                    sender_jid TEXT,
                    timestamp INTEGER,
                    quote_id TEXT,
                    is_from_me INTEGER,
                    read INTEGER,
                    kind INTEGER,
                    path TEXT,
                    file_id TEXT,
                    caption TEXT,
                    is_forwarded INTEGER NOT NULL DEFAULT 0,
                    forwarding_score INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE message_actions (
                    action_id TEXT PRIMARY KEY,
                    target_message_id TEXT NOT NULL,
                    chat_jid TEXT NOT NULL,
                    sender_jid TEXT NOT NULL,
                    kind INTEGER NOT NULL,
                    replacement TEXT,
                    occurred_at INTEGER NOT NULL,
                    arrival_order INTEGER NOT NULL
                );
                INSERT INTO text_messages (id, chat_jid, sender_jid, timestamp, is_from_me, read, message)
                    VALUES ('deleted-text', 'chat@example.test', 'sender@example.test', 1, 0, 0, 'secret body');
                INSERT INTO file_messages (id, chat_jid, sender_jid, timestamp, is_from_me, read, kind, path, file_id, caption)
                    VALUES ('deleted-file', 'chat@example.test', 'sender@example.test', 1, 0, 0, 0, 'images/secret.png', 'file-1', 'secret caption');
                INSERT INTO message_actions (action_id, target_message_id, chat_jid, sender_jid, kind, replacement, occurred_at, arrival_order)
                    VALUES ('delete-text', 'deleted-text', 'chat@example.test', 'sender@example.test', 1, NULL, 2, 1);
                INSERT INTO message_actions (action_id, target_message_id, chat_jid, sender_jid, kind, replacement, occurred_at, arrival_order)
                    VALUES ('delete-file', 'deleted-file', 'chat@example.test', 'sender@example.test', 1, NULL, 2, 2);",
            )
            .unwrap();
    }

    let mut database = DatabaseHandler::new(&path);
    database.init();

    let messages = database.get_messages();
    let text = messages
        .iter()
        .find(|message| message.info.id.as_ref() == "deleted-text")
        .expect("deleted text message must load as deleted");
    assert!(
        matches!(&text.message, MessageContent::Text(body) if body.as_ref() == "This message was deleted."),
        "delete markers must purge the stored body"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.info.id.as_ref() == "deleted-file"),
        "deleted file rows must be purged"
    );

    // No original content may remain in any stored row.
    let connection = rusqlite::Connection::open(&path).unwrap();
    let leaked_text: i64 = connection
        .prepare("SELECT COUNT(*) FROM text_messages WHERE message = 'secret body'")
        .unwrap()
        .query_row([], |row| row.get(0))
        .unwrap();
    assert_eq!(leaked_text, 0);
    let leaked_files: i64 = connection
        .prepare("SELECT COUNT(*) FROM file_messages WHERE id = 'deleted-file'")
        .unwrap()
        .query_row([], |row| row.get(0))
        .unwrap();
    assert_eq!(leaked_files, 0);
    database.stop();
}

/// Delete markers persist across a restart as deleted status.
#[test]
fn delete_status_persists_across_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("actions.db");
    let chat = JID::from("chat@example.test".to_owned());
    let original = text_message("target", &chat, &chat, "body", 1);
    let mut database = DatabaseHandler::new(&path);
    database.init();
    database.add_message(&original);
    assert_eq!(
        database.record_message_action(&action(
            "delete",
            "target",
            MessageActionKind::Delete,
            2,
            1
        )),
        MessageActionPersistence::Inserted
    );
    database.stop();

    let mut restarted = DatabaseHandler::new(&path);
    restarted.init();
    assert!(
        restarted
            .get_message_actions()
            .iter()
            .any(|action| action.kind == MessageActionKind::Delete)
    );
    let messages = restarted.get_messages();
    assert!(
        matches!(&messages[0].message, MessageContent::Text(body) if body.as_ref() == "This message was deleted."),
        "deleted message loads as the deleted-message text"
    );
    restarted.stop();
}

/// Local edits are projected and reconciled against their server echo; the
/// current body persists across a restart.
#[test]
fn server_echo_reconciles_one_local_edit_and_keeps_the_current_body() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("actions.db");
    let chat = JID::from("chat@example.test".to_owned());
    let mut original = text_message("target", &chat, &chat, "before", 1);
    original.info.is_from_me = true;
    let mut app = TestApp::with_database(&path);
    app.db_handler.add_message(&original);
    app.messages.insert("target".into(), original.clone());

    app.apply_message_action(action(
        "local-edit:target:1",
        "target",
        MessageActionKind::Edit {
            replacement: "A".into(),
        },
        2,
        1,
    ));
    app.apply_message_action(action(
        "server-edit-a-1",
        "target",
        MessageActionKind::Edit {
            replacement: "A".into(),
        },
        3,
        2,
    ));
    assert_eq!(app.message_actions["target"].len(), 1);
    assert_eq!(
        app.message_actions["target"][0].action_id.as_ref(),
        "server-edit-a-1"
    );

    app.apply_message_action(action(
        "server-edit-a-2",
        "target",
        MessageActionKind::Edit {
            replacement: "A".into(),
        },
        4,
        3,
    ));
    assert_eq!(app.message_actions["target"].len(), 2);
    assert!(
        matches!(&app.messages["target"].message, MessageContent::Text(body) if body.as_ref() == "A")
    );
    app.db_handler.stop();

    let mut reloaded = TestApp::with_database(&path);
    reloaded.load_data_from_db();
    assert_eq!(reloaded.message_actions["target"].len(), 2);
    assert_eq!(
        reloaded.message_actions["target"]
            .iter()
            .map(|action| action.action_id.as_ref())
            .collect::<Vec<_>>(),
        ["server-edit-a-1", "server-edit-a-2"]
    );
    assert!(
        matches!(&reloaded.messages["target"].message, MessageContent::Text(body) if body.as_ref() == "A")
    );
    assert!(reloaded.message_status(&"target".into()).edited);
    reloaded.db_handler.stop();
}

/// Protobuf forward sources persist across a restart and are invalidated by
/// a delete or revoke action.
#[test]
fn protobuf_forward_sources_for_text_and_files_persist_across_restart_and_are_invalidated_by_delete_or_revoke()
 {
    let directory = tempdir().unwrap();
    let path = directory.path().join("actions.db");
    let chat = JID::from("chat@example.test".to_owned());
    let sender = JID::from("sender@example.test".to_owned());
    let text = text_message("text", &chat, &sender, "text", 1);
    let file = Message {
        info: MessageInfo {
            id: "file".into(),
            chat: chat.clone(),
            sender: sender.clone(),
            mentions_self: false,
            timestamp: 2,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::File(Default::default()),
    };
    let text_source = vec![1, 2, 3, 4];
    let file_source = vec![5, 6, 7, 8];
    whatsrust::store_forward_source(&text.info, text_source.clone());
    whatsrust::store_forward_source(&file.info, file_source.clone());
    let mut database = DatabaseHandler::new(&path);
    database.init();
    database.add_message(&text);
    database.add_message(&file);
    database.stop();

    whatsrust::remove_forward_source(&chat, &text.info.id);
    whatsrust::remove_forward_source(&chat, &file.info.id);
    let mut restarted = DatabaseHandler::new(&path);
    restarted.init();
    let messages = restarted.get_messages();
    assert_eq!(messages.len(), 2);
    assert_eq!(whatsrust::forward_source(&text.info), Some(text_source));
    assert_eq!(whatsrust::forward_source(&file.info), Some(file_source));
    assert_eq!(
        restarted.record_message_action(&action("delete", "text", MessageActionKind::Delete, 3, 1)),
        MessageActionPersistence::Inserted
    );
    assert_eq!(whatsrust::forward_source(&text.info), None);
    assert_eq!(
        restarted.record_message_action(&action("revoke", "file", MessageActionKind::Delete, 4, 2)),
        MessageActionPersistence::Inserted
    );
    assert_eq!(whatsrust::forward_source(&file.info), None);
    restarted.stop();

    let mut after_delete = DatabaseHandler::new(&path);
    after_delete.init();
    after_delete.get_messages();
    assert_eq!(whatsrust::forward_source(&text.info), None);
    assert_eq!(whatsrust::forward_source(&file.info), None);
    after_delete.stop();
}
