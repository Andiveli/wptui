use rusqlite::Connection;

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
