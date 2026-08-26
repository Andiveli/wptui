use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;
use strum::IntoEnumIterator;
use whatsrust as wr;

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(2);
const READ_RECEIPT_SCHEMA_VERSION: i64 = 2;

pub(crate) fn open_database(path: &Path) -> Connection {
    let db = Connection::open(path).unwrap();
    db.busy_timeout(SQLITE_BUSY_TIMEOUT).unwrap();
    db
}

pub(crate) fn try_open_database(path: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open(path)?;
    db.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
    Ok(db)
}

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
pub(crate) fn initialize(db: &Connection) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_schema_initialization_is_idempotent() {
        let db = Connection::open_in_memory().unwrap();
        initialize(&db);
        initialize(&db);
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
}
