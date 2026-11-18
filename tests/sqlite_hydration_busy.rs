use std::{panic::catch_unwind, path::PathBuf};

use rusqlite::{Connection, TransactionBehavior};
use tempfile::{TempDir, tempdir};
use wp_tui::{
    app::{
        Chat,
        chat_store::hydration_port::{ChatStoreHydration, ChatStoreHydrationPort},
    },
    db::{DatabaseHandler, SqliteChatStoreHydration},
};

struct History {
    directory: TempDir,
    chat: Chat,
    messages: Vec<whatsrust::Message>,
}

impl History {
    fn seed() -> Self {
        let directory = tempdir().unwrap();
        let chat = Chat {
            jid: "hydration@example.test".to_owned().into(),
            last_message_time: None,
        };
        let messages = [("first", "original body", 42), ("second", "later body", 84)]
            .into_iter()
            .map(|(id, body, timestamp)| whatsrust::Message {
                info: whatsrust::MessageInfo {
                    id: id.into(),
                    chat: chat.jid.clone(),
                    sender: chat.jid.clone(),
                    mentions_self: false,
                    timestamp,
                    is_from_me: false,
                    quote_id: None,
                    read_by: 0,
                    forwarding: Default::default(),
                },
                message: whatsrust::MessageContent::Text(body.into()),
            })
            .collect::<Vec<_>>();
        let history = Self {
            directory,
            chat,
            messages,
        };
        let mut handler = DatabaseHandler::new(&history.path());
        handler.init();
        handler.add_chat(&history.chat);
        for message in &history.messages {
            handler.add_message(message);
        }
        // stop() drains accepted writes and joins the worker; no sleep or
        // scheduler assumption is needed before acquiring a separate lock.
        handler.stop();
        handler.add_contact(&history.chat.jid, "Saved contact");
        handler.record_reaction(
            &history.messages[0].info.id,
            history.chat.jid.clone(),
            "saved reaction".into(),
        );
        history
    }

    fn path(&self) -> PathBuf {
        self.directory.path().join("history.db")
    }

    fn load(&self) -> ChatStoreHydration {
        let hydration = SqliteChatStoreHydration::new(self.path());
        let port: &dyn ChatStoreHydrationPort = &hydration;
        port.try_load_complete().unwrap().snapshot
    }

    fn try_load(
        &self,
    ) -> rusqlite::Result<wp_tui::app::chat_store::hydration_port::CompleteChatStoreHydration> {
        let hydration = SqliteChatStoreHydration::new(self.path());
        let port: &dyn ChatStoreHydrationPort = &hydration;
        port.try_load_complete()
    }

    fn lock_connection(&self) -> Connection {
        let connection = Connection::open(self.path()).unwrap();
        // An exclusive transaction blocks readers in rollback-journal mode,
        // unlike WAL. This changes only the disposable test database.
        let mode: String = connection
            .query_row("PRAGMA journal_mode = DELETE", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "delete");
        connection
    }

    fn assert_complete(&self, stored: &ChatStoreHydration) {
        assert_eq!(stored.chats.len(), 1);
        assert_eq!(stored.chats[0].jid, self.chat.jid);
        assert_eq!(stored.chats[0].last_message_time, None);
        assert_eq!(
            stored.contacts,
            vec![(self.chat.jid.clone(), "Saved contact".into())]
        );
        assert_eq!(stored.messages.len(), self.messages.len());
        for expected in &self.messages {
            let matches = stored
                .messages
                .iter()
                .filter(|message| message.info.id == expected.info.id)
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1);
            let actual = matches[0];
            assert_eq!(actual.info.chat, expected.info.chat);
            assert_eq!(actual.info.sender, expected.info.sender);
            assert_eq!(actual.info.timestamp, expected.info.timestamp);
            assert_eq!(actual.info.is_from_me, expected.info.is_from_me);
            assert_eq!(actual.info.quote_id, expected.info.quote_id);
            assert_eq!(actual.info.read_by, expected.info.read_by);
            assert_eq!(actual.info.mentions_self, expected.info.mentions_self);
            assert_eq!(
                actual.info.forwarding.is_forwarded,
                expected.info.forwarding.is_forwarded
            );
            assert_eq!(actual.info.forwarding.score, expected.info.forwarding.score);
            match (&actual.message, &expected.message) {
                (
                    whatsrust::MessageContent::Text(actual),
                    whatsrust::MessageContent::Text(expected),
                ) => assert_eq!(actual, expected),
                _ => panic!("hydration changed the stored message kind"),
            }
        }
        assert_eq!(
            stored.reactions,
            vec![(
                self.messages[0].info.id.clone(),
                self.chat.jid.clone(),
                "saved reaction".into(),
            )]
        );
    }
}

// Each optional field is corrupted independently so an earlier decoding failure
// cannot conceal another field's historical fallback contract.
fn assert_optional_metadata_fallbacks(route: &str) {
    for (table, column) in [
        ("text_messages", "quote_id"),
        ("text_messages", "mention_ranges"),
        ("text_messages", "mentions_self"),
        ("file_messages", "quote_id"),
        ("file_messages", "caption"),
        ("file_messages", "mention_ranges"),
        ("file_messages", "mentions_self"),
        ("view_once_unavailable_messages", "mentions_self"),
    ] {
        let (mut history, ids) = metadata_history(&format!("{route}-{table}-{column}"));
        let target = match table {
            "text_messages" => &ids[0],
            "file_messages" => &ids[1],
            "view_once_unavailable_messages" => &ids[2],
            _ => unreachable!(),
        };
        let db = Connection::open(history.path()).unwrap();
        // Fixture mutation only: BLOB cannot decode as Option<String> or bool.
        db.execute(
            &format!("UPDATE {table} SET {column} = X'FF' WHERE id = ?1"),
            [target],
        )
        .unwrap();
        let expected = history
            .messages
            .iter_mut()
            .find(|message| message.info.id.as_ref() == target)
            .unwrap();
        match column {
            "quote_id" => expected.info.quote_id = None,
            "mentions_self" => expected.info.mentions_self = false,
            "caption" => {
                let whatsrust::MessageContent::File(file) = &mut expected.message else {
                    unreachable!()
                };
                file.caption = None;
            }
            "mention_ranges" => {}
            _ => unreachable!(),
        }

        // No caught panic is accepted: both legacy routes must return normally,
        // and the complete route must return Ok with every original message.
        let snapshot = match route {
            "handler" => {
                let mut handler = DatabaseHandler::new(&history.path());
                handler.stop();
                ChatStoreHydration {
                    chats: handler.get_chats(),
                    contacts: handler.get_contacts(),
                    messages: handler.get_messages(),
                    reactions: handler.get_reactions(),
                }
            }
            "legacy-port" => SqliteChatStoreHydration::new(history.path()).load(),
            "complete-port" => {
                let complete = history
                    .try_load()
                    .expect("optional metadata must retain legacy defaults");
                assert!(complete.native_link_previews.is_empty());
                complete.snapshot
            }
            _ => unreachable!(),
        };
        assert_metadata_history(&history, &snapshot);
        for message in &snapshot.messages {
            let text = match &message.message {
                whatsrust::MessageContent::Text(text) => Some(text.as_ref()),
                whatsrust::MessageContent::File(file) => file.caption.as_deref(),
                whatsrust::MessageContent::ViewOnceUnavailable => None,
            };
            if let Some(text) = text {
                let expected_ranges = if ids[..2].iter().any(|id| message.info.id.as_ref() == id)
                    && !(column == "mention_ranges" && message.info.id.as_ref() == target)
                {
                    vec![0..4]
                } else {
                    vec![]
                };
                assert_eq!(
                    whatsrust::message_mention_ranges(&message.info.id, text),
                    expected_ranges,
                    "{route}: {table}.{column}"
                );
            }
        }
    }
}

fn metadata_history(prefix: &str) -> (History, [String; 3]) {
    let mut history = History::seed();
    let ids = [
        format!("{prefix}-text"),
        format!("{prefix}-file"),
        format!("{prefix}-view-once"),
    ];
    let mut text = history.messages[0].clone();
    text.info.id = ids[0].clone().into();
    text.info.sender = "original-sender@example.test".to_owned().into();
    text.info.timestamp = 126;
    text.info.quote_id = Some("original-quote".into());
    text.info.read_by = 2;
    text.info.mentions_self = true;
    text.info.forwarding = whatsrust::ForwardingInfo {
        is_forwarded: true,
        score: 2,
    };
    text.message = whatsrust::MessageContent::Text(
        "body with original destination https://example.test/path?q=1".into(),
    );
    let mut file = text.clone();
    file.info.id = ids[1].clone().into();
    file.info.timestamp = 168;
    file.message = whatsrust::MessageContent::File(whatsrust::FileContent {
        kind: whatsrust::FileKind::Document,
        path: "documents/original.pdf".into(),
        file_id: "original-file-id".into(),
        caption: Some("file caption https://example.test/file".into()),
    });
    let mut view_once = text.clone();
    view_once.info.id = ids[2].clone().into();
    view_once.info.timestamp = 210;
    view_once.info.quote_id = None;
    view_once.info.forwarding = Default::default();
    view_once.message = whatsrust::MessageContent::ViewOnceUnavailable;
    let mut handler = DatabaseHandler::new(&history.path());
    for message in [text, file, view_once] {
        let source = match &message.message {
            whatsrust::MessageContent::Text(text) => Some(text.as_ref()),
            whatsrust::MessageContent::File(file) => file.caption.as_deref(),
            whatsrust::MessageContent::ViewOnceUnavailable => None,
        };
        if let Some(source) = source {
            whatsrust::store_message_mention_ranges(&message.info.id, source, vec![0..4]);
        }
        handler.add_message(&message);
        history.messages.push(message);
    }
    handler.stop();
    (history, ids)
}

fn assert_metadata_history(history: &History, actual: &ChatStoreHydration) {
    assert_eq!(actual.chats.len(), 1);
    assert_eq!(actual.chats[0].jid, history.chat.jid);
    assert_eq!(actual.chats[0].last_message_time, None);
    assert_eq!(
        actual.contacts,
        vec![(history.chat.jid.clone(), "Saved contact".into())]
    );
    assert_eq!(
        actual.reactions,
        vec![(
            history.messages[0].info.id.clone(),
            history.chat.jid.clone(),
            "saved reaction".into()
        )]
    );
    assert_eq!(actual.messages.len(), history.messages.len());
    for expected in &history.messages {
        let matches = actual
            .messages
            .iter()
            .filter(|message| message.info.id == expected.info.id)
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
        let message = matches[0];
        assert_eq!(message.info.chat, expected.info.chat);
        assert_eq!(message.info.sender, expected.info.sender);
        assert_eq!(message.info.timestamp, expected.info.timestamp);
        assert_eq!(message.info.quote_id, expected.info.quote_id);
        assert_eq!(message.info.is_from_me, expected.info.is_from_me);
        assert_eq!(message.info.read_by, expected.info.read_by);
        assert_eq!(message.info.mentions_self, expected.info.mentions_self);
        assert_eq!(message.info.forwarding, expected.info.forwarding);
        match (&message.message, &expected.message) {
            (
                whatsrust::MessageContent::Text(actual),
                whatsrust::MessageContent::Text(expected),
            ) => assert_eq!(actual, expected),
            (
                whatsrust::MessageContent::File(actual),
                whatsrust::MessageContent::File(expected),
            ) => {
                assert_eq!(actual.kind.clone() as u8, expected.kind.clone() as u8);
                assert_eq!(actual.path, expected.path);
                assert_eq!(actual.file_id, expected.file_id);
                assert_eq!(actual.caption, expected.caption);
            }
            (
                whatsrust::MessageContent::ViewOnceUnavailable,
                whatsrust::MessageContent::ViewOnceUnavailable,
            ) => {}
            _ => panic!("hydration changed the original message type"),
        }
    }
}

#[test]
fn legacy_handler_preserves_malformed_optional_metadata_defaults() {
    assert_optional_metadata_fallbacks("handler");
}

#[test]
fn legacy_hydration_preserves_malformed_optional_metadata_defaults() {
    assert_optional_metadata_fallbacks("legacy-port");
}

#[test]
fn complete_hydration_preserves_malformed_optional_metadata_defaults() {
    assert_optional_metadata_fallbacks("complete-port");
}

#[test]
fn complete_hydration_rejects_malformed_mandatory_fields_without_partial_success() {
    for (table, column, index, target_index) in [
        ("text_messages", "message", 7, 0),
        ("file_messages", "path", 8, 1),
        ("view_once_unavailable_messages", "timestamp", 3, 2),
    ] {
        let (history, ids) = metadata_history(&format!("mandatory-{table}"));
        let db = Connection::open(history.path()).unwrap();
        db.execute(
            &format!("UPDATE {table} SET {column} = X'FF' WHERE id = ?1"),
            [&ids[target_index]],
        )
        .unwrap();
        match history.try_load() {
            Err(rusqlite::Error::InvalidColumnType(
                actual_index,
                name,
                rusqlite::types::Type::Blob,
            )) => {
                assert_eq!(actual_index, index);
                assert_eq!(name, column);
            }
            Err(error) => panic!("unexpected mandatory-field error: {error}"),
            Ok(_) => panic!("malformed {table}.{column} must not yield empty or partial success"),
        }
    }
}

#[test]
fn unlocked_hydration_preserves_complete_seeded_history() {
    let history = History::seed();
    history.assert_complete(&history.load());
    history.assert_complete(&history.load());
    // Existing infallible callers retain the same complete history.
    let legacy = SqliteChatStoreHydration::new(history.path());
    history.assert_complete(&legacy.load());
}

#[test]
fn reserved_writer_lock_still_allows_complete_hydration() {
    let history = History::seed();
    let mut connection = history.lock_connection();
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();

    // A reserved writer lock is not an exclusive reader-blocking lock.
    history.assert_complete(&history.load());
    transaction.rollback().unwrap();
    history.assert_complete(&history.load());
}

#[test]
fn exclusive_lock_on_a_different_database_does_not_block_hydration() {
    let history = History::seed();
    let other = History::seed();
    assert_ne!(history.path(), other.path());
    let mut connection = other.lock_connection();
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Exclusive)
        .unwrap();

    history.assert_complete(&history.load());
    transaction.rollback().unwrap();
    other.assert_complete(&other.load());
}

#[test]
fn exhausted_exclusive_lock_must_not_panic_or_return_incomplete_history() {
    let history = History::seed();
    history.assert_complete(&history.load());
    let mut connection = history.lock_connection();
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Exclusive)
        .unwrap();

    // Keep the lock until load returns/unwinds: it cannot disappear before the
    // existing two-second busy timeout expires. No elapsed-time assertion,
    // sleep, background release, or modified busy timeout is involved.
    // Exercise the actual public fallible adapter, including schema preparation.
    let outcome = catch_unwind(|| history.try_load());
    transaction.rollback().unwrap();
    // Check persisted state even on the expected RED panic path.
    history.assert_complete(&history.load());

    let result = outcome.expect("fallible hydration must not unwind");
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("exclusive lock must not produce successful partial history"),
    };
    match error {
        rusqlite::Error::SqliteFailure(code, message) => {
            assert_eq!(code.code, rusqlite::ErrorCode::DatabaseBusy);
            assert_eq!(code.extended_code, 5);
            assert_eq!(message.as_deref(), Some("database is locked"));
        }
        error => panic!("expected SQLITE_BUSY (5), got {error}"),
    }
}
