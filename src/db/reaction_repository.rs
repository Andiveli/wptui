use std::sync::Arc;

use rusqlite::Connection;
use whatsrust as wr;

use super::DATABASE_WRITE_LOCK;

pub(super) fn record(
    db: &Connection,
    message_id: &wr::MessageId,
    participant: wr::JID,
    emoji: Arc<str>,
) {
    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
    if emoji.is_empty() {
        db.execute(
            "DELETE FROM message_reactions WHERE message_id = ?1 AND participant_jid = ?2",
            rusqlite::params![message_id, participant.0],
        )
        .unwrap();
    } else {
        db.execute(
            "INSERT OR REPLACE INTO message_reactions (message_id, participant_jid, emoji) VALUES (?1, ?2, ?3)",
            rusqlite::params![message_id, participant.0, emoji],
        )
        .unwrap();
    }
}

pub(super) fn get(db: &Connection) -> Vec<(wr::MessageId, wr::JID, Arc<str>)> {
    let mut query = db
        .prepare("SELECT message_id, participant_jid, emoji FROM message_reactions ORDER BY message_id, participant_jid")
        .unwrap();
    query
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?.into(),
                row.get::<_, String>(1)?.into(),
                Arc::from(row.get::<_, String>(2)?),
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}
