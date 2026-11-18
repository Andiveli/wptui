use std::sync::Arc;

use rusqlite::Connection;
use whatsrust as wr;

use crate::app::Chat;

pub(super) fn persist(db: &mut Connection, chats: Vec<Chat>) {
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    persist_in_transaction(&tx, chats);
    tx.commit().unwrap();
}

pub(super) fn persist_in_transaction(tx: &rusqlite::Transaction<'_>, chats: Vec<Chat>) {
    let mut statement = tx
        .prepare("INSERT OR REPLACE INTO chats (jid) VALUES (?)")
        .unwrap();
    for chat in chats {
        statement.execute(rusqlite::params![&*chat.jid.0]).unwrap();
    }
    drop(statement);
}

pub(super) fn get_chats(db: &Connection) -> Vec<Chat> {
    try_get_chats(db).unwrap()
}

pub(super) fn try_get_chats(db: &Connection) -> rusqlite::Result<Vec<Chat>> {
    let mut query = db.prepare("SELECT jid FROM chats")?;
    query
        .query_map([], |row| {
            let jid: String = row.get(0)?;
            Ok(Chat {
                jid: jid.into(),
                last_message_time: None,
            })
        })?
        .collect()
}

pub(super) fn add_contact(db: &Connection, jid: &wr::JID, name: &str) {
    let _write_lock = super::DATABASE_WRITE_LOCK.lock().unwrap();
    db.execute(
        "INSERT OR REPLACE INTO contacts (jid, name) VALUES (?1, ?2)",
        rusqlite::params![&*jid.0, name],
    )
    .unwrap();
}

pub(super) fn get_contacts(db: &Connection) -> Vec<(wr::JID, Arc<str>)> {
    try_get_contacts(db).unwrap()
}

pub(super) fn try_get_contacts(db: &Connection) -> rusqlite::Result<Vec<(wr::JID, Arc<str>)>> {
    let mut stmt = db.prepare("SELECT jid, name FROM contacts")?;
    stmt.query_map([], |row| {
        let jid: String = row.get(0)?;
        let name: String = row.get(1)?;
        Ok((jid.into(), Arc::from(name)))
    })?
    .collect()
}
