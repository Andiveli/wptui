use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

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

fn ensure_read_receipt_schema(db: &Connection) -> rusqlite::Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < READ_RECEIPT_SCHEMA_VERSION {
        db.execute_batch("CREATE TABLE IF NOT EXISTS read_receipt_pending (chat TEXT NOT NULL, sender TEXT NOT NULL, message_id TEXT NOT NULL, timestamp INTEGER NOT NULL, kind INTEGER NOT NULL, PRIMARY KEY (chat, sender, message_id)); CREATE INDEX IF NOT EXISTS idx_read_receipt_pending_timestamp ON read_receipt_pending(timestamp); CREATE TABLE IF NOT EXISTS read_receipt_sent (chat TEXT NOT NULL, sender TEXT NOT NULL, message_id TEXT NOT NULL, PRIMARY KEY (chat, sender, message_id)); CREATE TABLE IF NOT EXISTS read_receipt_rejected (chat TEXT NOT NULL, sender TEXT NOT NULL, message_id TEXT NOT NULL, PRIMARY KEY (chat, sender, message_id)); PRAGMA user_version = 2")?;
    }
    Ok(())
}
