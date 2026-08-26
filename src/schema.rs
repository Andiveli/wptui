use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use strum::IntoEnumIterator;
use whatsrust as wr;

const READ_RECEIPT_SCHEMA_VERSION: i64 = 2;

/// Prepares the tables required by the handler before full initialization.
/// This preserves the historical constructor lifecycle.
pub(crate) fn prepare(db: &Connection) {
    if let Err(error) = ensure_read_receipt_schema(db) {
        log::error!("read-receipt schema migration failed: {error}");
    }
    db.execute(
        "CREATE TABLE IF NOT EXISTS chat_read_cursors (
            chat_jid TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            timestamp INTEGER NOT NULL
        )",
        [],
    )
    .unwrap();
    db.execute(
        "CREATE TABLE IF NOT EXISTS status_read_cursors (
            contact_jid TEXT PRIMARY KEY,
            timestamp INTEGER NOT NULL
        )",
        [],
    )
    .unwrap();
    db.execute(
        "CREATE TABLE IF NOT EXISTS message_reactions (
            message_id TEXT NOT NULL,
            participant_jid TEXT NOT NULL,
            emoji TEXT NOT NULL,
            PRIMARY KEY (message_id, participant_jid)
        )",
        [],
    )
    .unwrap();
    db.execute(
        "CREATE TABLE IF NOT EXISTS message_actions (
            action_id TEXT PRIMARY KEY,
            target_message_id TEXT NOT NULL,
            chat_jid TEXT NOT NULL,
            sender_jid TEXT NOT NULL,
            kind INTEGER NOT NULL,
            occurred_at INTEGER NOT NULL,
            arrival_order INTEGER NOT NULL
        )",
        [],
    )
    .unwrap();
    db.execute(
        "CREATE TABLE IF NOT EXISTS forward_sources (id TEXT NOT NULL, chat_jid TEXT NOT NULL, sender_jid TEXT NOT NULL, source BLOB NOT NULL, PRIMARY KEY (id, chat_jid, sender_jid))",
        [],
    )
    .unwrap();
}

/// Applies message-table migrations needed by ordinary operations before
/// callers explicitly request full schema initialization.
pub(crate) fn prepare_legacy_message_schema(db: &Connection) {
    prepare_legacy_forwarding_schema(db);
    ensure_mention_columns(db);
}

pub(crate) fn prepare_legacy_forwarding_schema(db: &Connection) {
    ensure_forwarding_columns(db, "text_messages");
    ensure_forwarding_columns(db, "file_messages");
}

/// Creates the complete message schema and applies its legacy migrations.
/// Callers hold the process-wide database write lock.
pub(crate) fn initialize(db: &Connection, deleted_message_text: &str) {
    db.execute(
        "CREATE TABLE IF NOT EXISTS chats (jid TEXT PRIMARY KEY)",
        [],
    )
    .unwrap();
    db.execute(
        "CREATE TABLE IF NOT EXISTS contacts (jid TEXT PRIMARY KEY, name TEXT NOT NULL)",
        [],
    )
    .unwrap();
    db.execute(
        "CREATE TABLE IF NOT EXISTS chat_read_cursors (chat_jid TEXT PRIMARY KEY, message_id TEXT NOT NULL, timestamp INTEGER NOT NULL)",
        [],
    )
    .unwrap();
    db.execute(
        "CREATE TABLE IF NOT EXISTS status_read_cursors (contact_jid TEXT PRIMARY KEY, timestamp INTEGER NOT NULL)",
        [],
    )
    .unwrap();
    for kind in wr::MessageContent::iter() {
        match kind {
            wr::MessageContent::Text(_) => {
                db.execute(
                    "CREATE TABLE IF NOT EXISTS text_messages (
                        id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER,
                        quote_id TEXT, is_from_me INTEGER, read INTEGER, message TEXT
                    )",
                    [],
                )
                .unwrap();
            }
            wr::MessageContent::File(_) => {
                db.execute(
                    "CREATE TABLE IF NOT EXISTS file_messages (
                        id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER,
                        quote_id TEXT, is_from_me INTEGER, read INTEGER, kind INTEGER,
                        path TEXT, file_id TEXT, caption TEXT
                    )",
                    [],
                )
                .unwrap();
            }
            wr::MessageContent::ViewOnceUnavailable => {}
        }
    }
    db.execute(
        "CREATE TABLE IF NOT EXISTS view_once_unavailable_messages (
            id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER,
            is_from_me INTEGER, read INTEGER, mentions_self INTEGER NOT NULL DEFAULT 0
        )",
        [],
    )
    .unwrap();
    prepare_legacy_message_schema(db);
    migrate_message_action_columns(db, deleted_message_text);
}

fn ensure_read_receipt_schema(db: &Connection) -> rusqlite::Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < READ_RECEIPT_SCHEMA_VERSION {
        db.execute_batch("CREATE TABLE IF NOT EXISTS read_receipt_pending (chat TEXT NOT NULL, sender TEXT NOT NULL, message_id TEXT NOT NULL, timestamp INTEGER NOT NULL, kind INTEGER NOT NULL, PRIMARY KEY (chat, sender, message_id)); CREATE INDEX IF NOT EXISTS idx_read_receipt_pending_timestamp ON read_receipt_pending(timestamp); CREATE TABLE IF NOT EXISTS read_receipt_sent (chat TEXT NOT NULL, sender TEXT NOT NULL, message_id TEXT NOT NULL, PRIMARY KEY (chat, sender, message_id)); CREATE TABLE IF NOT EXISTS read_receipt_rejected (chat TEXT NOT NULL, sender TEXT NOT NULL, message_id TEXT NOT NULL, PRIMARY KEY (chat, sender, message_id)); PRAGMA user_version = 2")?;
    }
    Ok(())
}

fn columns(db: &Connection, table: &str) -> Vec<String> {
    db.prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn ensure_forwarding_columns(db: &Connection, table: &str) {
    let present = columns(db, table);
    if present.is_empty() {
        return;
    }
    for (column, definition) in [
        ("is_forwarded", "INTEGER NOT NULL DEFAULT 0"),
        ("forwarding_score", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !present.iter().any(|item| item == column) {
            db.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )
            .unwrap();
        }
    }
}

fn ensure_mention_columns(db: &Connection) {
    for table in ["text_messages", "file_messages"] {
        let present = columns(db, table);
        if !present.is_empty() {
            if !present.iter().any(|column| column == "mention_ranges") {
                db.execute(
                    &format!("ALTER TABLE {table} ADD COLUMN mention_ranges TEXT"),
                    [],
                )
                .unwrap();
            }
            if !present.iter().any(|column| column == "mentions_self") {
                db.execute(
                    &format!(
                        "ALTER TABLE {table} ADD COLUMN mentions_self INTEGER NOT NULL DEFAULT 0"
                    ),
                    [],
                )
                .unwrap();
            }
        }
    }
}

/// Migrates legacy action replacement bodies into message rows, then removes
/// the obsolete action column. The caller holds the database write lock.
fn migrate_message_action_columns(db: &Connection, deleted_message_text: &str) {
    let has_replacement = db
        .prepare(
            "SELECT COUNT(*) FROM pragma_table_info('message_actions') WHERE name = 'replacement'",
        )
        .unwrap()
        .query_row([], |row| row.get::<_, i64>(0))
        .unwrap()
        > 0;
    if !has_replacement {
        return;
    }
    let mut effective_bodies = HashMap::new();
    let mut deleted_targets = HashSet::new();
    let rows = db
        .prepare("SELECT target_message_id, kind, replacement FROM message_actions ORDER BY occurred_at, arrival_order, action_id")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<String>>(2)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (target, kind, replacement) in rows {
        // kind 0 is Edit; the last edit in stable order wins.
        if kind == 1 {
            deleted_targets.insert(target);
        } else if kind == 0
            && let Some(body) = replacement
        {
            effective_bodies.insert(target, body);
        }
    }
    for (target, body) in &effective_bodies {
        if !deleted_targets.contains(target) {
            db.execute(
                "UPDATE text_messages SET message = ?1 WHERE id = ?2",
                rusqlite::params![body, target],
            )
            .unwrap();
        }
    }
    for target in deleted_targets {
        db.execute(
            "UPDATE text_messages SET message = ?1, quote_id = NULL WHERE id = ?2",
            rusqlite::params![deleted_message_text, target],
        )
        .unwrap();
        db.execute(
            "DELETE FROM file_messages WHERE id = ?1",
            rusqlite::params![target],
        )
        .unwrap();
    }
    db.execute("ALTER TABLE message_actions DROP COLUMN replacement", [])
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_schema_initialization_is_idempotent() {
        let db = Connection::open_in_memory().unwrap();
        initialize(&db, "[deleted]");
        initialize(&db, "[deleted]");
        assert_eq!(columns(&db, "text_messages").len(), 12);
        assert_eq!(columns(&db, "file_messages").len(), 15);
    }

    #[test]
    fn legacy_message_columns_migrate_forwarding_and_mentions() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE text_messages (id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER, quote_id TEXT, is_from_me INTEGER, read INTEGER, message TEXT);
             CREATE TABLE file_messages (id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER, quote_id TEXT, is_from_me INTEGER, read INTEGER, kind INTEGER, path TEXT, file_id TEXT, caption TEXT);",
        )
        .unwrap();
        prepare_legacy_message_schema(&db);
        prepare_legacy_message_schema(&db);
        assert!(columns(&db, "text_messages").contains(&"is_forwarded".into()));
        assert!(columns(&db, "text_messages").contains(&"mention_ranges".into()));
        assert!(columns(&db, "text_messages").contains(&"mentions_self".into()));
        assert!(columns(&db, "file_messages").contains(&"forwarding_score".into()));
    }

    #[test]
    fn legacy_action_replacements_migrate_in_order_and_are_idempotent() {
        let db = Connection::open_in_memory().unwrap();
        prepare(&db);
        db.execute_batch(
            "CREATE TABLE text_messages (id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER, quote_id TEXT, is_from_me INTEGER, read INTEGER, message TEXT);
             CREATE TABLE file_messages (id TEXT PRIMARY KEY, chat_jid TEXT, sender_jid TEXT, timestamp INTEGER, quote_id TEXT, is_from_me INTEGER, read INTEGER, kind INTEGER, path TEXT, file_id TEXT, caption TEXT);
             INSERT INTO text_messages VALUES ('target', 'chat', 'sender', 1, 'quote', 0, 0, 'old');
             ALTER TABLE message_actions ADD COLUMN replacement TEXT;
             INSERT INTO message_actions (action_id, target_message_id, chat_jid, sender_jid, kind, replacement, occurred_at, arrival_order) VALUES ('edit-1', 'target', 'chat', 'sender', 0, 'first', 1, 1);
             INSERT INTO message_actions (action_id, target_message_id, chat_jid, sender_jid, kind, replacement, occurred_at, arrival_order) VALUES ('edit-2', 'target', 'chat', 'sender', 0, 'last', 2, 1);
             INSERT INTO message_actions (action_id, target_message_id, chat_jid, sender_jid, kind, replacement, occurred_at, arrival_order) VALUES ('delete-1', 'target', 'chat', 'sender', 1, NULL, 3, 1);",
        )
        .unwrap();

        initialize(&db, "[deleted]");
        initialize(&db, "[deleted]");

        assert_eq!(
            db.query_row(
                "SELECT message FROM text_messages WHERE id = 'target'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "[deleted]"
        );
        assert_eq!(
            db.query_row(
                "SELECT message, quote_id FROM text_messages WHERE id = 'target'",
                [],
                |row| { Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)) }
            )
            .unwrap(),
            ("[deleted]".to_owned(), None)
        );
        // Persisted edits intentionally load as empty replacement markers; the
        // migrated message row is the source of the displayed body.
        assert_eq!(
            db.query_row(
                "SELECT group_concat(kind, ',') FROM (SELECT kind FROM message_actions WHERE target_message_id = 'target' ORDER BY occurred_at, arrival_order, action_id)",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "0,0,1"
        );
        assert!(!columns(&db, "message_actions").contains(&"replacement".into()));
    }
}
