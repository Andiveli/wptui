use std::ops::Range;

use rusqlite::{Connection, OptionalExtension};
use strum::IntoEnumIterator;
use whatsrust as wr;

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
