use std::sync::Arc;

use rusqlite::Connection;
use whatsrust as wr;

use crate::app::Chat;

pub(super) fn persist(db: &mut Connection, chats: Vec<Chat>) {
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let mut statement = tx
        .prepare("INSERT OR REPLACE INTO chats (jid) VALUES (?)")
        .unwrap();
    for chat in chats {
        statement.execute(rusqlite::params![&*chat.jid.0]).unwrap();
    }
    drop(statement);
    tx.commit().unwrap();
}

pub(super) fn get_chats(db: &Connection) -> Vec<Chat> {
    let mut query = db.prepare("SELECT jid FROM chats").unwrap();
    query
        .query_map([], |row| {
            let jid: String = row.get(0).unwrap();
            Ok(Chat {
                jid: jid.into(),
                last_message_time: None,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
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
    let mut stmt = db.prepare("SELECT jid, name FROM contacts").unwrap();
    let rows = stmt
        .query_map([], |row| {
            let jid: String = row.get(0).unwrap();
            let name: String = row.get(1).unwrap();
            Ok((jid.into(), Arc::from(name)))
        })
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}
