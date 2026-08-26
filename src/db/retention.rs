use std::path::PathBuf;

use rusqlite::{Connection, TransactionBehavior};

use crate::app::STATUS_BROADCAST_CHAT;

use super::DATABASE_WRITE_LOCK;

/// WhatsApp statuses expire 24 hours after posting (server-side). The local
/// purge uses the same window so status broadcasts do not accumulate forever.
const STATUS_RETENTION_SECS: i64 = 24 * 60 * 60;

pub(super) fn purge(db: &mut Connection, now: i64) -> Vec<PathBuf> {
    let cutoff = now - STATUS_RETENTION_SECS;
    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();

    let purged_paths = {
        let mut query = tx
            .prepare("SELECT path FROM file_messages WHERE chat_jid = ?1 AND timestamp < ?2")
            .unwrap();
        query
            .query_map(rusqlite::params![STATUS_BROADCAST_CHAT, cutoff], |row| {
                row.get::<_, String>(0)
            })
            .unwrap()
            .filter_map(Result::ok)
            .map(PathBuf::from)
            .collect::<Vec<_>>()
    };

    tx.execute(
        "DELETE FROM file_messages WHERE chat_jid = ?1 AND timestamp < ?2",
        rusqlite::params![STATUS_BROADCAST_CHAT, cutoff],
    )
    .unwrap();
    tx.execute(
        "DELETE FROM text_messages WHERE chat_jid = ?1 AND timestamp < ?2",
        rusqlite::params![STATUS_BROADCAST_CHAT, cutoff],
    )
    .unwrap();
    tx.commit().unwrap();
    purged_paths
}
