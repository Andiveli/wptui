use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use whatsrust as wr;

use crate::app::{DELETED_MESSAGE_TEXT, MessageAction, MessageActionKind};

use super::{DATABASE_WRITE_LOCK, schema};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessageActionPersistence {
    Inserted,
    DuplicateActionID,
    Reconciled,
}

pub(super) fn record(db: &mut Connection, action: &MessageAction) -> MessageActionPersistence {
    schema::prepare_legacy_forwarding_schema(db);
    let kind = action_kind(&action.kind);
    let replacement = match &action.kind {
        MessageActionKind::Edit { replacement } => Some(replacement.as_ref()),
        MessageActionKind::Delete => None,
    };
    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
    let transaction = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO message_actions (action_id, target_message_id, chat_jid, sender_jid, kind, replacement, occurred_at, arrival_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                action.action_id,
                action.target_message_id,
                action.chat.0,
                action.sender.0,
                kind,
                replacement,
                action.occurred_at,
                action.arrival_order as i64,
            ],
        )
        .unwrap()
        == 1;
    if !inserted {
        transaction.commit().unwrap();
        return MessageActionPersistence::DuplicateActionID;
    }
    match &action.kind {
        MessageActionKind::Delete => {
            if transaction
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'text_messages'",
                    [],
                    |_| Ok(()),
                )
                .optional()
                .unwrap()
                .is_some()
            {
                transaction
                    .execute(
                        "DELETE FROM message_actions WHERE target_message_id = ?1 AND action_id != ?2",
                        rusqlite::params![action.target_message_id, action.action_id],
                    )
                    .unwrap();
                transaction
                    .execute(
                        "UPDATE text_messages SET message = ?1, quote_id = NULL, is_forwarded = 0, forwarding_score = 0 WHERE id = ?2",
                        rusqlite::params![DELETED_MESSAGE_TEXT, action.target_message_id],
                    )
                    .unwrap();
                transaction
                    .execute(
                        "INSERT OR REPLACE INTO text_messages (id, chat_jid, sender_jid, timestamp, quote_id, is_from_me, read, message, is_forwarded, forwarding_score) SELECT id, chat_jid, sender_jid, timestamp, NULL, is_from_me, read, ?1, 0, 0 FROM file_messages WHERE id = ?2",
                        rusqlite::params![DELETED_MESSAGE_TEXT, action.target_message_id],
                    )
                    .unwrap();
                transaction
                    .execute(
                        "DELETE FROM file_messages WHERE id = ?1",
                        rusqlite::params![action.target_message_id],
                    )
                    .unwrap();
                transaction
                    .execute(
                        "DELETE FROM forward_sources WHERE id = ?1 AND chat_jid = ?2",
                        rusqlite::params![action.target_message_id, action.chat.0],
                    )
                    .unwrap();
            }
        }
        MessageActionKind::Edit { replacement } => {
            let text_table_exists = transaction
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'text_messages'",
                    [],
                    |_| Ok(()),
                )
                .optional()
                .unwrap()
                .is_some();
            let target_exists = text_table_exists
                && transaction
                    .query_row(
                        "SELECT 1 FROM text_messages WHERE id = ?1",
                        rusqlite::params![action.target_message_id],
                        |_| Ok(()),
                    )
                    .optional()
                    .unwrap()
                    .is_some();
            if target_exists {
                let latest_action_id: String = transaction
                    .query_row(
                        "SELECT action_id FROM message_actions WHERE target_message_id = ?1 ORDER BY occurred_at DESC, arrival_order DESC, action_id DESC LIMIT 1",
                        rusqlite::params![action.target_message_id],
                        |row| row.get(0),
                    )
                    .unwrap();
                if latest_action_id == action.action_id.as_ref() {
                    transaction
                        .execute(
                            "UPDATE text_messages SET message = ?1 WHERE id = ?2",
                            rusqlite::params![replacement, action.target_message_id],
                        )
                        .unwrap();
                }
                transaction
                    .execute(
                        "UPDATE message_actions SET replacement = NULL WHERE action_id = ?1",
                        rusqlite::params![action.action_id],
                    )
                    .unwrap();
            }
        }
    }
    transaction.commit().unwrap();
    if matches!(action.kind, MessageActionKind::Delete) {
        wr::remove_forward_source(&action.chat, &action.target_message_id);
    }
    MessageActionPersistence::Inserted
}

pub(super) fn reconcile(
    db: &mut Connection,
    local_action_id: &str,
    server_action: &MessageAction,
) -> MessageActionPersistence {
    let kind = action_kind(&server_action.kind);
    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
    let transaction = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let server_exists = transaction
        .query_row(
            "SELECT 1 FROM message_actions WHERE action_id = ?1",
            rusqlite::params![server_action.action_id],
            |_| Ok(()),
        )
        .optional()
        .unwrap()
        .is_some();
    if server_exists
        || transaction
            .execute(
                "DELETE FROM message_actions WHERE action_id = ?1",
                rusqlite::params![local_action_id],
            )
            .unwrap()
            != 1
    {
        transaction.commit().unwrap();
        return MessageActionPersistence::DuplicateActionID;
    }
    transaction
        .execute(
            "INSERT INTO message_actions (action_id, target_message_id, chat_jid, sender_jid, kind, occurred_at, arrival_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                server_action.action_id,
                server_action.target_message_id,
                server_action.chat.0,
                server_action.sender.0,
                kind,
                server_action.occurred_at,
                server_action.arrival_order as i64,
            ],
        )
        .unwrap();
    transaction.commit().unwrap();
    MessageActionPersistence::Reconciled
}

pub(super) fn get(db: &Connection) -> Vec<MessageAction> {
    let mut query = db.prepare("SELECT action_id, target_message_id, chat_jid, sender_jid, kind, replacement, occurred_at, arrival_order FROM message_actions ORDER BY occurred_at, arrival_order, action_id").unwrap();
    query
        .query_map([], |row| {
            let kind = match row.get::<_, u8>(4)? {
                0 => MessageActionKind::Edit {
                    replacement: row.get::<_, Option<String>>(5)?.unwrap_or_default().into(),
                },
                1 => MessageActionKind::Delete,
                _ => return Err(rusqlite::Error::InvalidQuery),
            };
            Ok(MessageAction {
                action_id: row.get::<_, String>(0)?.into(),
                target_message_id: row.get::<_, String>(1)?.into(),
                chat: row.get::<_, String>(2)?.into(),
                sender: row.get::<_, String>(3)?.into(),
                kind,
                occurred_at: row.get(6)?,
                arrival_order: row.get::<_, i64>(7)? as u64,
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn action_kind(kind: &MessageActionKind) -> u8 {
    match kind {
        MessageActionKind::Edit { .. } => 0,
        MessageActionKind::Delete => 1,
    }
}
