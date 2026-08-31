use std::ops::Range;

use rusqlite::{Connection, OptionalExtension};
use strum::IntoEnumIterator;
use whatsrust as wr;

use crate::app::DELETED_MESSAGE_TEXT;

fn encode_mention_ranges(ranges: &[Range<usize>]) -> Option<String> {
    (!ranges.is_empty()).then(|| {
        ranges
            .iter()
            .map(|range| format!("{}:{}", range.start, range.end))
            .collect::<Vec<_>>()
            .join(",")
    })
}

fn decode_mention_ranges(encoded: Option<String>, text: &str) -> Vec<Range<usize>> {
    encoded
        .unwrap_or_default()
        .split(',')
        .filter_map(|item| {
            let (start, end) = item.split_once(':')?;
            let range = Range {
                start: start.parse().ok()?,
                end: end.parse().ok()?,
            };
            (range.start < range.end
                && range.end <= text.len()
                && text.is_char_boundary(range.start)
                && text.is_char_boundary(range.end))
            .then_some(range)
        })
        .collect()
}

pub(super) fn get_messages(db: &Connection) -> Vec<wr::Message> {
    super::schema::prepare_legacy_message_schema(db);
    let mut messages = Vec::new();
    for kind in wr::MessageContent::iter() {
        let msgs = match kind {
            wr::MessageContent::Text(_) => {
                let mut query = db.prepare("SELECT * FROM text_messages").unwrap();
                query
                    .query_map([], |row| {
                        let id: String = row.get(0).unwrap();
                        let chat_jid: String = row.get(1).unwrap();
                        let sender_jid: String = row.get(2).unwrap();
                        let timestamp: i64 = row.get(3).unwrap();
                        let quote_id: Option<String> = row.get(4).unwrap_or(None);
                        let is_from_me: bool = row.get(5).unwrap();
                        let read_by: u16 = row.get(6).unwrap();
                        let message: String = row.get(7).unwrap();
                        let is_forwarded: bool = row.get(8).unwrap();
                        let forwarding_score: u32 = row.get(9).unwrap();
                        let mention_ranges: Option<String> = row.get(10).unwrap_or(None);
                        let mentions_self: bool = row.get(11).unwrap_or(false);
                        let result = wr::Message {
                            info: wr::MessageInfo {
                                id: id.into(),
                                chat: chat_jid.into(),
                                sender: sender_jid.into(),
                                mentions_self,
                                timestamp,
                                quote_id: quote_id.map(Into::into),
                                is_from_me,
                                read_by,
                                forwarding: wr::ForwardingInfo {
                                    is_forwarded,
                                    score: forwarding_score,
                                },
                            },
                            message: wr::MessageContent::Text(message.clone().into()),
                        };
                        wr::store_message_mention_ranges(
                            &result.info.id,
                            &message,
                            decode_mention_ranges(mention_ranges, &message),
                        );
                        Ok(result)
                    })
                    .unwrap()
                    .collect::<Vec<Result<_, _>>>()
            }
            wr::MessageContent::File(_) => {
                let mut query = db.prepare("SELECT * FROM file_messages").unwrap();
                query
                    .query_map([], |row| {
                        let id: String = row.get(0).unwrap();
                        let chat_jid: String = row.get(1).unwrap();
                        let sender_jid: String = row.get(2).unwrap();
                        let timestamp: i64 = row.get(3).unwrap();
                        let quote_id: Option<String> = row.get(4).unwrap_or(None);
                        let is_from_me: bool = row.get(5).unwrap();
                        let read_by: u16 = row.get(6).unwrap();
                        let kind: u8 = row.get(7).unwrap();
                        let path: String = row.get(8).unwrap();
                        let file_id: String = row.get(9).unwrap();
                        let caption: Option<String> = row.get(10).unwrap_or(None);
                        let is_forwarded: bool = row.get(11).unwrap();
                        let forwarding_score: u32 = row.get(12).unwrap();
                        let mention_ranges: Option<String> = row.get(13).unwrap_or(None);
                        let mentions_self: bool = row.get(14).unwrap_or(false);
                        let result = wr::Message {
                            info: wr::MessageInfo {
                                id: id.into(),
                                chat: chat_jid.into(),
                                sender: sender_jid.into(),
                                mentions_self,
                                timestamp,
                                quote_id: quote_id.map(Into::into),
                                is_from_me,
                                read_by,
                                forwarding: wr::ForwardingInfo {
                                    is_forwarded,
                                    score: forwarding_score,
                                },
                            },
                            message: wr::MessageContent::File(wr::FileContent {
                                kind: wr::FileKind::from_repr(kind).unwrap(),
                                path: path.into(),
                                file_id: file_id.into(),
                                caption: caption.as_ref().map(|c| c.as_str().into()),
                            }),
                        };
                        if let Some(caption) = caption.as_deref() {
                            wr::store_message_mention_ranges(
                                &result.info.id,
                                caption,
                                decode_mention_ranges(mention_ranges, caption),
                            );
                        }
                        Ok(result)
                    })
                    .unwrap()
                    .collect::<Vec<Result<_, _>>>()
            }
            wr::MessageContent::ViewOnceUnavailable => {
                let mut query = db
                    .prepare("SELECT * FROM view_once_unavailable_messages")
                    .unwrap();
                query
                    .query_map([], |row| {
                        Ok(wr::Message {
                            info: wr::MessageInfo {
                                id: row.get::<_, String>(0)?.into(),
                                chat: row.get::<_, String>(1)?.into(),
                                sender: row.get::<_, String>(2)?.into(),
                                mentions_self: row.get(6).unwrap_or(false),
                                timestamp: row.get(3)?,
                                is_from_me: row.get(4)?,
                                quote_id: None,
                                read_by: row.get(5)?,
                                forwarding: Default::default(),
                            },
                            message: wr::MessageContent::ViewOnceUnavailable,
                        })
                    })
                    .unwrap()
                    .collect::<Vec<Result<_, _>>>()
            }
        };
        for msg in msgs {
            let msg = msg.unwrap();
            if let Some(source) = db.query_row(
                "SELECT source FROM forward_sources WHERE id = ?1 AND chat_jid = ?2 AND sender_jid = ?3",
                rusqlite::params![msg.info.id, msg.info.chat.0, msg.info.sender.0],
                |row| row.get::<_, Vec<u8>>(0),
            ).optional().unwrap() {
                wr::store_forward_source(&msg.info, source);
            }
            messages.push(msg);
        }
    }
    messages
}

pub(super) fn persist(db: &mut Connection, messages: Vec<wr::Message>) {
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    persist_in_transaction(&tx, messages);
    tx.commit().unwrap();
}

pub(super) fn persist_in_transaction(tx: &rusqlite::Transaction<'_>, messages: Vec<wr::Message>) {
    let mut text_stmt = tx.prepare("INSERT INTO text_messages (id, chat_jid, sender_jid, timestamp, quote_id, is_from_me, read, message, is_forwarded, forwarding_score, mention_ranges, mentions_self) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET chat_jid=excluded.chat_jid, sender_jid=excluded.sender_jid, timestamp=excluded.timestamp, quote_id=excluded.quote_id, is_from_me=(text_messages.is_from_me OR excluded.is_from_me), read=excluded.read, message=excluded.message, is_forwarded=excluded.is_forwarded, forwarding_score=excluded.forwarding_score, mention_ranges=excluded.mention_ranges, mentions_self=excluded.mentions_self").unwrap();
    let mut file_stmt = tx.prepare("INSERT INTO file_messages (id, chat_jid, sender_jid, timestamp, quote_id, is_from_me, read, kind, path, file_id, caption, is_forwarded, forwarding_score, mention_ranges, mentions_self) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET chat_jid=excluded.chat_jid, sender_jid=excluded.sender_jid, timestamp=excluded.timestamp, quote_id=excluded.quote_id, is_from_me=(file_messages.is_from_me OR excluded.is_from_me), read=excluded.read, kind=excluded.kind, path=excluded.path, file_id=excluded.file_id, is_forwarded=excluded.is_forwarded, forwarding_score=excluded.forwarding_score, mention_ranges=excluded.mention_ranges, mentions_self=excluded.mentions_self").unwrap();
    let mut view_once_stmt = tx.prepare("INSERT INTO view_once_unavailable_messages (id, chat_jid, sender_jid, timestamp, is_from_me, read, mentions_self) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET chat_jid=excluded.chat_jid, sender_jid=excluded.sender_jid, timestamp=excluded.timestamp, is_from_me=(view_once_unavailable_messages.is_from_me OR excluded.is_from_me), read=excluded.read, mentions_self=excluded.mentions_self").unwrap();
    let mut source_stmt = tx.prepare("INSERT OR REPLACE INTO forward_sources (id, chat_jid, sender_jid, source) VALUES (?, ?, ?, ?)").unwrap();
    for message in messages {
        let mut msg = message;
        let deleted = tx
            .query_row(
                "SELECT 1 FROM message_actions WHERE target_message_id = ?1 AND kind = 1 LIMIT 1",
                rusqlite::params![msg.info.id],
                |_| Ok(()),
            )
            .optional()
            .unwrap()
            .is_some();
        if deleted {
            msg.message = wr::MessageContent::Text(DELETED_MESSAGE_TEXT.into());
            msg.info.quote_id = None;
            msg.info.forwarding = Default::default();
        }
        let pending_edit = (!deleted)
            .then(|| {
                tx.query_row(
                    "SELECT action_id, replacement FROM message_actions WHERE target_message_id = ?1 ORDER BY occurred_at DESC, arrival_order DESC, action_id DESC LIMIT 1",
                    rusqlite::params![msg.info.id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .optional()
            })
            .transpose()
            .unwrap()
            .flatten()
            .and_then(|(action_id, replacement)| replacement.map(|replacement| (action_id, replacement)));
        if let Some((_, replacement)) = &pending_edit {
            msg.message = wr::MessageContent::Text(replacement.clone().into());
        }
        match &msg.message {
            wr::MessageContent::Text(text) => text_stmt
                .execute(rusqlite::params![
                    msg.info.id,
                    msg.info.chat.0,
                    msg.info.sender.0,
                    msg.info.timestamp,
                    msg.info.quote_id,
                    msg.info.is_from_me,
                    msg.info.read_by,
                    text,
                    msg.info.forwarding.is_forwarded,
                    msg.info.forwarding.score,
                    encode_mention_ranges(&wr::message_mention_ranges(&msg.info.id, text)),
                    msg.info.mentions_self
                ])
                .unwrap(),
            wr::MessageContent::File(file) => file_stmt
                .execute(rusqlite::params![
                    msg.info.id,
                    msg.info.chat.0,
                    msg.info.sender.0,
                    msg.info.timestamp,
                    msg.info.quote_id,
                    msg.info.is_from_me,
                    msg.info.read_by,
                    file.kind.clone() as u8,
                    file.path,
                    file.file_id,
                    file.caption,
                    msg.info.forwarding.is_forwarded,
                    msg.info.forwarding.score,
                    file.caption
                        .as_ref()
                        .and_then(|caption| encode_mention_ranges(&wr::message_mention_ranges(
                            &msg.info.id,
                            caption
                        ))),
                    msg.info.mentions_self
                ])
                .unwrap(),
            wr::MessageContent::ViewOnceUnavailable => view_once_stmt
                .execute(rusqlite::params![
                    msg.info.id,
                    msg.info.chat.0,
                    msg.info.sender.0,
                    msg.info.timestamp,
                    msg.info.is_from_me,
                    msg.info.read_by,
                    msg.info.mentions_self
                ])
                .unwrap(),
        };
        if pending_edit.is_some() {
            tx.execute(
                "UPDATE message_actions SET replacement = NULL WHERE target_message_id = ?1 AND kind = 0",
                rusqlite::params![msg.info.id],
            )
            .unwrap();
        }
        if let Some(source) = wr::forward_source(&msg.info) {
            source_stmt
                .execute(rusqlite::params![
                    msg.info.id,
                    msg.info.chat.0,
                    msg.info.sender.0,
                    source
                ])
                .unwrap();
        }
    }
    drop((text_stmt, file_stmt, view_once_stmt, source_stmt));
}
