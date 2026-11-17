use std::{any::Any, panic::catch_unwind, path::PathBuf};

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
        port.load()
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

#[test]
fn unlocked_hydration_preserves_complete_seeded_history() {
    let history = History::seed();
    history.assert_complete(&history.load());
    history.assert_complete(&history.load());
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

fn panic_text(payload: &(dyn Any + Send)) -> &str {
    if let Some(text) = payload.downcast_ref::<String>() {
        text
    } else if let Some(text) = payload.downcast_ref::<&str>() {
        text
    } else {
        panic!("expected a SQLite error panic with a string payload");
    }
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
    // This exercises the public load path, not necessarily message_store's
    // row-result unwrap: the earlier chats SELECT can fail first.
    let outcome = catch_unwind(|| history.load());
    transaction.rollback().unwrap();
    // Check persisted state even on the expected RED panic path.
    history.assert_complete(&history.load());

    match outcome {
        Ok(stored) => history.assert_complete(&stored),
        Err(payload) => {
            let text = panic_text(payload.as_ref());
            assert!(
                text.contains("code: DatabaseBusy, extended_code: 5")
                    && text.contains("database is locked"),
                "unexpected hydration panic, rather than SQLITE_BUSY (5): {text}"
            );
            // The current infallible port has no typed error outcome to assert.
            // Catching its panic is diagnostic, never an accepted success.
            panic!("hydration must handle exhausted SQLITE_BUSY without unwinding: {text}");
        }
    }
}
