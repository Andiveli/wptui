use rusqlite::Connection;
use whatsrust as wr;

use super::DATABASE_WRITE_LOCK;

pub(super) fn set_last_read(
    db: &Connection,
    chat: &wr::JID,
    message_id: Option<wr::MessageId>,
    timestamp: i64,
) {
    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
    db.execute(
        "INSERT INTO chat_read_cursors (chat_jid, message_id, timestamp)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(chat_jid) DO UPDATE SET message_id = excluded.message_id, timestamp = excluded.timestamp",
        rusqlite::params![chat.0, message_id, timestamp],
    )
    .unwrap();
}

pub(super) fn read_cursors(db: &Connection) -> Vec<(wr::JID, wr::MessageId, i64)> {
    db.prepare("SELECT chat_jid, message_id, timestamp FROM chat_read_cursors")
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?.into(),
                row.get::<_, String>(1)?.into(),
                row.get(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

pub(super) fn set_status_last_seen(
    db: &Connection,
    contact: &wr::JID,
    timestamp: i64,
) -> Result<usize, rusqlite::Error> {
    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
    db.execute(
        "INSERT INTO status_read_cursors (contact_jid, timestamp) VALUES (?1, ?2)
         ON CONFLICT(contact_jid) DO UPDATE SET timestamp = MAX(timestamp, excluded.timestamp)",
        rusqlite::params![contact.0, timestamp],
    )
}

pub(super) fn status_last_seen(db: &Connection) -> Result<Vec<(wr::JID, i64)>, rusqlite::Error> {
    let mut query = db.prepare("SELECT contact_jid, timestamp FROM status_read_cursors")?;
    query
        .query_map([], |row| Ok((row.get::<_, String>(0)?.into(), row.get(1)?)))?
        .collect()
}
