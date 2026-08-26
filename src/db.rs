use std::{
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use log::debug;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use strum::IntoEnumIterator;
use whatsrust as wr;

use crate::app::{Chat, DELETED_MESSAGE_TEXT, MessageAction};

#[path = "db/action_repository.rs"]
mod action_repository;
#[path = "db/connection.rs"]
mod connection;
#[path = "db/cursor_repository.rs"]
mod cursor_repository;
#[path = "db/reaction_repository.rs"]
mod reaction_repository;
#[path = "db/retention.rs"]
mod retention;
#[path = "schema.rs"]
mod schema;
pub use action_repository::MessageActionPersistence;
pub(crate) use connection::{
    DATABASE_WRITE_LOCK, open_database, try_open_database, with_database_write_lock,
};

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

pub struct DatabaseHandler {
    db: Connection,
    new_messages_queue: Arc<Mutex<Vec<wr::Message>>>,
    new_chats_queue: Arc<Mutex<Vec<Chat>>>,
    should_stop: Arc<Mutex<bool>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl DatabaseHandler {
    pub fn new(db_path: &Path) -> Self {
        let db = open_database(db_path);
        let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
        schema::prepare(&db);
        let new_messages_queue = Arc::new(Mutex::new(Vec::<wr::Message>::new()));
        let new_chats_queue = Arc::new(Mutex::new(Vec::<Chat>::new()));
        let should_stop = Arc::new(Mutex::new(false));

        let new_messages_queue_clone = Arc::clone(&new_messages_queue);
        let new_chats_queue_clone = Arc::clone(&new_chats_queue);
        let should_stop_clone = Arc::clone(&should_stop);
        let db_path = db_path.to_owned();
        let thread = std::thread::spawn(move || {
            let mut db = open_database(&db_path);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let new_chats = {
                    let mut queue = new_chats_queue_clone.lock().unwrap();
                    let mut chats = Vec::new();
                    while let Some(chat) = queue.pop() {
                        chats.push(chat);
                    }
                    chats
                };
                if !new_chats.is_empty() {
                    debug!("Saving {} new chats to the database", new_chats.len());
                    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
                    let tx = db
                        .transaction_with_behavior(TransactionBehavior::Immediate)
                        .unwrap();
                    {
                        let mut statement = tx
                            .prepare("INSERT OR REPLACE INTO chats (jid) VALUES (?)")
                            .unwrap();
                        for chat in new_chats {
                            statement.execute(rusqlite::params![&*chat.jid.0]).unwrap();
                        }
                    }
                    tx.commit().unwrap();
                }

                let messages = {
                    let mut queue = new_messages_queue_clone.lock().unwrap();
                    std::mem::take(&mut *queue)
                };
                if !messages.is_empty() {
                    debug!("Saving {} new messages to the database", messages.len());
                    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
                    let tx = db
                        .transaction_with_behavior(TransactionBehavior::Immediate)
                        .unwrap();

                    {
                        let mut text_stmt = tx
                            .prepare("INSERT INTO text_messages (id, chat_jid, sender_jid, timestamp, quote_id, is_from_me, read, message, is_forwarded, forwarding_score, mention_ranges, mentions_self) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET chat_jid=excluded.chat_jid, sender_jid=excluded.sender_jid, timestamp=excluded.timestamp, quote_id=excluded.quote_id, is_from_me=(text_messages.is_from_me OR excluded.is_from_me), read=excluded.read, message=excluded.message, is_forwarded=excluded.is_forwarded, forwarding_score=excluded.forwarding_score, mention_ranges=excluded.mention_ranges, mentions_self=excluded.mentions_self")
                            .unwrap();
                        let mut file_stmt = tx
                            .prepare("INSERT INTO file_messages (id, chat_jid, sender_jid, timestamp, quote_id, is_from_me, read, kind, path, file_id, caption, is_forwarded, forwarding_score, mention_ranges, mentions_self) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET chat_jid=excluded.chat_jid, sender_jid=excluded.sender_jid, timestamp=excluded.timestamp, quote_id=excluded.quote_id, is_from_me=(file_messages.is_from_me OR excluded.is_from_me), read=excluded.read, kind=excluded.kind, path=excluded.path, file_id=excluded.file_id, caption=excluded.caption, is_forwarded=excluded.is_forwarded, forwarding_score=excluded.forwarding_score, mention_ranges=excluded.mention_ranges, mentions_self=excluded.mentions_self")
                            .unwrap();
                        let mut view_once_stmt = tx
                            .prepare("INSERT INTO view_once_unavailable_messages (id, chat_jid, sender_jid, timestamp, is_from_me, read, mentions_self) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET chat_jid=excluded.chat_jid, sender_jid=excluded.sender_jid, timestamp=excluded.timestamp, is_from_me=(view_once_unavailable_messages.is_from_me OR excluded.is_from_me), read=excluded.read, mentions_self=excluded.mentions_self")
                            .unwrap();
                        let mut source_stmt = tx.prepare("INSERT OR REPLACE INTO forward_sources (id, chat_jid, sender_jid, source) VALUES (?, ?, ?, ?)").unwrap();
                        for message in &messages {
                            let mut msg = message.clone();
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
                            match &msg.message {
                                wr::MessageContent::Text(text) => {
                                    text_stmt
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
                                            encode_mention_ranges(&wr::message_mention_ranges(
                                                &msg.info.id,
                                                text,
                                            )),
                                            msg.info.mentions_self,
                                        ])
                                        .unwrap();
                                }
                                wr::MessageContent::File(file) => {
                                    file_stmt
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
                                            file.caption.as_ref().and_then(|caption| {
                                                encode_mention_ranges(&wr::message_mention_ranges(
                                                    &msg.info.id,
                                                    caption,
                                                ))
                                            }),
                                            msg.info.mentions_self,
                                        ])
                                        .unwrap();
                                }
                                wr::MessageContent::ViewOnceUnavailable => {
                                    view_once_stmt
                                        .execute(rusqlite::params![
                                            msg.info.id,
                                            msg.info.chat.0,
                                            msg.info.sender.0,
                                            msg.info.timestamp,
                                            msg.info.is_from_me,
                                            msg.info.read_by,
                                            msg.info.mentions_self,
                                        ])
                                        .unwrap();
                                }
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
                    }
                    tx.commit().unwrap();
                }

                let should_stop = should_stop_clone.lock().unwrap();
                if *should_stop {
                    break;
                }
                drop(should_stop);
            }
        });

        Self {
            db,
            new_messages_queue,
            new_chats_queue,
            should_stop,
            thread: Some(thread),
        }
    }

    pub fn stop(&mut self) {
        let mut should_stop = self.should_stop.lock().unwrap();
        *should_stop = true;
        drop(should_stop);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }

    pub fn add_message(&self, message: &wr::Message) {
        schema::prepare_legacy_message_schema(&self.db);
        let mut queue = self.new_messages_queue.lock().unwrap();
        queue.push(message.clone());
    }

    pub fn add_chat(&self, chat: &Chat) {
        let mut queue = self.new_chats_queue.lock().unwrap();
        queue.push(chat.clone());
    }

    pub fn record_reaction(
        &self,
        message_id: &wr::MessageId,
        participant: wr::JID,
        emoji: Arc<str>,
    ) {
        reaction_repository::record(&self.db, message_id, participant, emoji);
    }

    pub fn get_reactions(&self) -> Vec<(wr::MessageId, wr::JID, Arc<str>)> {
        reaction_repository::get(&self.db)
    }

    /// Inserts once by the protocol action ID. A duplicate arriving from
    /// live delivery or history sync is ignored.
    pub fn record_message_action(&self, action: &MessageAction) -> MessageActionPersistence {
        action_repository::record(&self.db, action)
    }

    /// Atomically confirms one pending local action with its server action ID.
    pub fn reconcile_message_action(
        &mut self,
        local_action_id: &str,
        server_action: &MessageAction,
    ) -> MessageActionPersistence {
        action_repository::reconcile(&mut self.db, local_action_id, server_action)
    }

    pub fn get_message_actions(&self) -> Vec<MessageAction> {
        action_repository::get(&self.db)
    }

    pub fn get_chats(&self) -> Vec<Chat> {
        let mut query = self.db.prepare("SELECT jid FROM chats").unwrap();
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

    pub fn get_messages(&self) -> Vec<wr::Message> {
        schema::prepare_legacy_message_schema(&self.db);
        let mut messages = Vec::new();
        for kind in wr::MessageContent::iter() {
            let msgs = match kind {
                wr::MessageContent::Text(_) => {
                    let mut query = self.db.prepare("SELECT * FROM text_messages").unwrap();
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
                            let mention_ranges = decode_mention_ranges(mention_ranges, &message);

                            let result = wr::Message {
                                info: wr::MessageInfo {
                                    id: id.into(),
                                    chat: chat_jid.into(),
                                    sender: sender_jid.into(),
                                    mentions_self,
                                    timestamp,
                                    quote_id: quote_id.map(|q| q.into()),
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
                                mention_ranges,
                            );
                            Ok(result)
                        })
                        .unwrap()
                        .collect::<Vec<Result<_, _>>>()
                }
                wr::MessageContent::File(_) => {
                    let mut query = self.db.prepare("SELECT * FROM file_messages").unwrap();
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
                            let mention_ranges = caption
                                .as_deref()
                                .map(|text| decode_mention_ranges(mention_ranges, text))
                                .unwrap_or_default();

                            let result = wr::Message {
                                info: wr::MessageInfo {
                                    id: id.into(),
                                    chat: chat_jid.into(),
                                    sender: sender_jid.into(),
                                    mentions_self,
                                    timestamp,
                                    quote_id: quote_id.map(|q| q.into()),
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
                                    mention_ranges,
                                );
                            }
                            Ok(result)
                        })
                        .unwrap()
                        .collect::<Vec<Result<_, _>>>()
                }
                wr::MessageContent::ViewOnceUnavailable => {
                    let mut query = self
                        .db
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
                if let Some(source) = self.db.query_row(
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

    pub fn add_contact(&self, jid: &wr::JID, name: &str) {
        let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
        self.db
            .execute(
                "INSERT OR REPLACE INTO contacts (jid, name) VALUES (?1, ?2)",
                rusqlite::params![&*jid.0, name],
            )
            .unwrap();
    }

    pub fn get_contacts(&self) -> Vec<(wr::JID, Arc<str>)> {
        let mut stmt = self.db.prepare("SELECT jid, name FROM contacts").unwrap();
        let rows = stmt
            .query_map([], |row| {
                let jid: String = row.get(0).unwrap();
                let name: String = row.get(1).unwrap();
                Ok((jid.into(), Arc::from(name)))
            })
            .unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    pub fn init(&self) {
        let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
        schema::initialize(&self.db, DELETED_MESSAGE_TEXT);
    }

    pub fn set_last_read_cursor(
        &self,
        chat: &wr::JID,
        message_id: Option<wr::MessageId>,
        timestamp: i64,
    ) {
        cursor_repository::set_last_read(&self.db, chat, message_id, timestamp);
    }

    pub fn read_cursors(&self) -> Vec<(wr::JID, wr::MessageId, i64)> {
        cursor_repository::read_cursors(&self.db)
    }

    pub fn set_status_last_seen(
        &self,
        contact: &wr::JID,
        timestamp: i64,
    ) -> Result<usize, rusqlite::Error> {
        cursor_repository::set_status_last_seen(&self.db, contact, timestamp)
    }

    pub fn status_last_seen(&self) -> Result<Vec<(wr::JID, i64)>, rusqlite::Error> {
        cursor_repository::status_last_seen(&self.db)
    }
}

impl DatabaseHandler {
    /// Deletes `status@broadcast` messages older than the 24-hour WhatsApp
    /// retention window. Returns the relative media paths of the purged
    /// file messages so the caller can remove them from disk.
    pub fn purge_expired_statuses(&mut self, now: i64) -> Vec<PathBuf> {
        retention::purge(&mut self.db, now)
    }
}
