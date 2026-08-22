use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use log::debug;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use strum::IntoEnumIterator;
use whatsrust as wr;

use crate::app::{
    Chat, DELETED_MESSAGE_TEXT, MessageAction, MessageActionKind, STATUS_BROADCAST_CHAT,
};

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(2);

// SQLite allows only one writer. All handlers in this process share this lock,
// including the asynchronous queue writer started by each handler.
static DATABASE_WRITE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// WhatsApp statuses expire 24 hours after posting (server-side). The local
/// purge uses the same window so status broadcasts do not accumulate forever.
const STATUS_RETENTION_SECS: i64 = 24 * 60 * 60;

fn open_database(path: &Path) -> Connection {
    let db = Connection::open(path).unwrap();
    db.busy_timeout(SQLITE_BUSY_TIMEOUT).unwrap();
    db
}

pub(crate) fn try_open_database(path: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open(path)?;
    db.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
    Ok(db)
}

pub(crate) fn with_database_write_lock<T>(operation: impl FnOnce() -> T) -> T {
    let _lock = DATABASE_WRITE_LOCK.lock().unwrap();
    operation()
}

const READ_RECEIPT_SCHEMA_VERSION: i64 = 2;

fn ensure_read_receipt_schema(db: &Connection) -> rusqlite::Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < READ_RECEIPT_SCHEMA_VERSION {
        db.execute_batch("CREATE TABLE IF NOT EXISTS read_receipt_pending (chat TEXT NOT NULL, sender TEXT NOT NULL, message_id TEXT NOT NULL, timestamp INTEGER NOT NULL, kind INTEGER NOT NULL, PRIMARY KEY (chat, sender, message_id)); CREATE INDEX IF NOT EXISTS idx_read_receipt_pending_timestamp ON read_receipt_pending(timestamp); CREATE TABLE IF NOT EXISTS read_receipt_sent (chat TEXT NOT NULL, sender TEXT NOT NULL, message_id TEXT NOT NULL, PRIMARY KEY (chat, sender, message_id)); CREATE TABLE IF NOT EXISTS read_receipt_rejected (chat TEXT NOT NULL, sender TEXT NOT NULL, message_id TEXT NOT NULL, PRIMARY KEY (chat, sender, message_id)); PRAGMA user_version = 2")?;
    }
    Ok(())
}

fn ensure_forwarding_columns(db: &Connection, table: &str) {
    let columns = db
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    if columns.is_empty() {
        return;
    }
    for (column, definition) in [
        ("is_forwarded", "INTEGER NOT NULL DEFAULT 0"),
        ("forwarding_score", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !columns.iter().any(|present| present == column) {
            db.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )
            .unwrap();
        }
    }
}

fn ensure_mention_columns(db: &Connection) {
    for table in ["text_messages", "file_messages"] {
        let columns = db
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        if !columns.is_empty() {
            if !columns.iter().any(|column| column == "mention_ranges") {
                db.execute(
                    &format!("ALTER TABLE {table} ADD COLUMN mention_ranges TEXT"),
                    [],
                )
                .unwrap();
            }
            if !columns.iter().any(|column| column == "mentions_self") {
                db.execute(
                    &format!(
                        "ALTER TABLE {table} ADD COLUMN mentions_self INTEGER NOT NULL DEFAULT 0"
                    ),
                    [],
                )
                .unwrap();
            }
        }
    }
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessageActionPersistence {
    Inserted,
    DuplicateActionID,
    Reconciled,
}

impl DatabaseHandler {
    pub fn new(db_path: &Path) -> Self {
        let db = open_database(db_path);
        let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
        if let Err(error) = ensure_read_receipt_schema(&db) {
            log::error!("read-receipt schema migration failed: {error}");
        }
        db.execute(
            "CREATE TABLE IF NOT EXISTS chat_read_cursors (
                chat_jid TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            )",
            [],
        )
        .unwrap();
        db.execute(
            "CREATE TABLE IF NOT EXISTS status_read_cursors (
                contact_jid TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL
            )",
            [],
        )
        .unwrap();
        db.execute(
            "CREATE TABLE IF NOT EXISTS message_reactions (
                message_id TEXT NOT NULL,
                participant_jid TEXT NOT NULL,
                emoji TEXT NOT NULL,
                PRIMARY KEY (message_id, participant_jid)
            )",
            [],
        )
        .unwrap();
        db.execute(
            "CREATE TABLE IF NOT EXISTS message_actions (
                action_id TEXT PRIMARY KEY,
                target_message_id TEXT NOT NULL,
                chat_jid TEXT NOT NULL,
                sender_jid TEXT NOT NULL,
                kind INTEGER NOT NULL,
                occurred_at INTEGER NOT NULL,
                arrival_order INTEGER NOT NULL
            )",
            [],
        )
        .unwrap();

        db.execute("CREATE TABLE IF NOT EXISTS forward_sources (id TEXT NOT NULL, chat_jid TEXT NOT NULL, sender_jid TEXT NOT NULL, source BLOB NOT NULL, PRIMARY KEY (id, chat_jid, sender_jid))", []).unwrap();
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
                            .prepare("INSERT OR REPLACE INTO text_messages (id, chat_jid, sender_jid, timestamp, quote_id, is_from_me, read, message, is_forwarded, forwarding_score, mention_ranges, mentions_self) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                            .unwrap();
                        let mut file_stmt = tx
                            .prepare("INSERT OR REPLACE INTO file_messages (id, chat_jid, sender_jid, timestamp, quote_id, is_from_me, read, kind, path, file_id, caption, is_forwarded, forwarding_score, mention_ranges, mentions_self) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
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
        self.migrate_forwarding_columns();
        ensure_mention_columns(&self.db);
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
        let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
        if emoji.is_empty() {
            self.db
                .execute(
                    "DELETE FROM message_reactions WHERE message_id = ?1 AND participant_jid = ?2",
                    rusqlite::params![message_id, participant.0],
                )
                .unwrap();
        } else {
            self.db
                .execute(
                    "INSERT OR REPLACE INTO message_reactions (message_id, participant_jid, emoji) VALUES (?1, ?2, ?3)",
                    rusqlite::params![message_id, participant.0, emoji],
                )
                .unwrap();
        }
    }

    pub fn get_reactions(&self) -> Vec<(wr::MessageId, wr::JID, Arc<str>)> {
        let mut query = self
            .db
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

    /// Inserts once by the protocol action ID. A duplicate arriving from
    /// live delivery or history sync is ignored.
    pub fn record_message_action(&self, action: &MessageAction) -> MessageActionPersistence {
        self.migrate_forwarding_columns();
        let kind = match &action.kind {
            MessageActionKind::Edit { .. } => 0_u8,
            MessageActionKind::Delete => 1_u8,
        };
        let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
        let inserted = self
            .db
            .execute(
                "INSERT OR IGNORE INTO message_actions (action_id, target_message_id, chat_jid, sender_jid, kind, occurred_at, arrival_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    action.action_id,
                    action.target_message_id,
                    action.chat.0,
                    action.sender.0,
                    kind,
                    action.occurred_at,
                    action.arrival_order as i64,
                ],
            )
            .unwrap()
            == 1;
        if inserted
            && matches!(action.kind, MessageActionKind::Delete)
            && self
                .db
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'text_messages'",
                    [],
                    |_| Ok(()),
                )
                .optional()
                .unwrap()
                .is_some()
        {
            self.db
                .execute(
                    "DELETE FROM message_actions WHERE target_message_id = ?1 AND action_id != ?2",
                    rusqlite::params![action.target_message_id, action.action_id],
                )
                .unwrap();
            self.db
                    .execute(
                        "UPDATE text_messages SET message = ?1, quote_id = NULL, is_forwarded = 0, forwarding_score = 0 WHERE id = ?2",
                        rusqlite::params![DELETED_MESSAGE_TEXT, action.target_message_id],
                    )
                    .unwrap();
            self.db
                    .execute(
                        "INSERT OR REPLACE INTO text_messages (id, chat_jid, sender_jid, timestamp, quote_id, is_from_me, read, message, is_forwarded, forwarding_score) SELECT id, chat_jid, sender_jid, timestamp, NULL, is_from_me, read, ?1, 0, 0 FROM file_messages WHERE id = ?2",
                        rusqlite::params![DELETED_MESSAGE_TEXT, action.target_message_id],
                    )
                    .unwrap();
            self.db
                .execute(
                    "DELETE FROM file_messages WHERE id = ?1",
                    rusqlite::params![action.target_message_id],
                )
                .unwrap();
            self.db
                .execute(
                    "DELETE FROM forward_sources WHERE id = ?1 AND chat_jid = ?2",
                    rusqlite::params![action.target_message_id, action.chat.0],
                )
                .unwrap();
            wr::remove_forward_source(&action.chat, &action.target_message_id);
        }
        if inserted {
            MessageActionPersistence::Inserted
        } else {
            MessageActionPersistence::DuplicateActionID
        }
    }

    /// Atomically confirms one pending local action with its server action ID.
    pub fn reconcile_message_action(
        &mut self,
        local_action_id: &str,
        server_action: &MessageAction,
    ) -> MessageActionPersistence {
        let kind = match &server_action.kind {
            MessageActionKind::Edit { .. } => 0_u8,
            MessageActionKind::Delete => 1_u8,
        };
        let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
        let transaction = self
            .db
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

    pub fn get_message_actions(&self) -> Vec<MessageAction> {
        let mut query = self.db.prepare("SELECT action_id, target_message_id, chat_jid, sender_jid, kind, occurred_at, arrival_order FROM message_actions ORDER BY occurred_at, arrival_order, action_id").unwrap();
        query
            .query_map([], |row| {
                // An edit loads as a status marker; the displayed body always
                // comes from the current message row.
                let kind = match row.get::<_, u8>(4)? {
                    0 => MessageActionKind::Edit {
                        replacement: Arc::from(""),
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
                    occurred_at: row.get(5)?,
                    arrival_order: row.get::<_, i64>(6)? as u64,
                })
            })
            .unwrap()
            .map(Result::unwrap)
            .collect()
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
        self.migrate_forwarding_columns();
        ensure_mention_columns(&self.db);
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
        self.db
            .execute(
                "CREATE TABLE IF NOT EXISTS chats (
                    jid TEXT PRIMARY KEY
                )",
                [],
            )
            .unwrap();

        self.db
            .execute(
                "CREATE TABLE IF NOT EXISTS contacts (
                    jid TEXT PRIMARY KEY,
                    name TEXT NOT NULL
                )",
                [],
            )
            .unwrap();
        self.db
            .execute(
                "CREATE TABLE IF NOT EXISTS chat_read_cursors (
                    chat_jid TEXT PRIMARY KEY,
                    message_id TEXT NOT NULL,
                    timestamp INTEGER NOT NULL
                )",
                [],
            )
            .unwrap();
        self.db
            .execute(
                "CREATE TABLE IF NOT EXISTS status_read_cursors (
                    contact_jid TEXT PRIMARY KEY,
                    timestamp INTEGER NOT NULL
                )",
                [],
            )
            .unwrap();

        for kind in wr::MessageContent::iter() {
            match kind {
                wr::MessageContent::Text(_) => {
                    self.db
                        .execute(
                            "CREATE TABLE IF NOT EXISTS text_messages (
                                id TEXT PRIMARY KEY,
                                chat_jid TEXT,
                                sender_jid TEXT,
                                timestamp INTEGER,
                                quote_id TEXT,
                                is_from_me INTEGER,
                                read INTEGER,

                                message TEXT
                            )",
                            [],
                        )
                        .unwrap();
                }
                wr::MessageContent::File(_) => {
                    self.db
                        .execute(
                            "CREATE TABLE IF NOT EXISTS file_messages (
                                id TEXT PRIMARY KEY,
                                chat_jid TEXT,
                                sender_jid TEXT,
                                timestamp INTEGER,
                                quote_id TEXT,
                                is_from_me INTEGER,
                                read INTEGER,

                                kind INTEGER,
                                path TEXT,
                                file_id TEXT,
                                caption TEXT
                            )",
                            [],
                        )
                        .unwrap();
                }
            }
        }
        self.migrate_forwarding_columns();
        ensure_mention_columns(&self.db);
        self.migrate_message_action_columns();
    }

    pub fn set_last_read_cursor(
        &self,
        chat: &wr::JID,
        message_id: Option<wr::MessageId>,
        timestamp: i64,
    ) {
        let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
        self.db
            .execute(
                "INSERT INTO chat_read_cursors (chat_jid, message_id, timestamp)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(chat_jid) DO UPDATE SET message_id = excluded.message_id, timestamp = excluded.timestamp",
                rusqlite::params![chat.0, message_id, timestamp],
            )
            .unwrap();
    }

    pub fn read_cursors(&self) -> Vec<(wr::JID, wr::MessageId, i64)> {
        self.db
            .prepare("SELECT chat_jid, message_id, timestamp FROM chat_read_cursors")
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

    pub fn set_status_last_seen(
        &self,
        contact: &wr::JID,
        timestamp: i64,
    ) -> Result<usize, rusqlite::Error> {
        let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
        self.db
            .execute(
                "INSERT INTO status_read_cursors (contact_jid, timestamp) VALUES (?1, ?2)
                 ON CONFLICT(contact_jid) DO UPDATE SET timestamp = MAX(timestamp, excluded.timestamp)",
                rusqlite::params![contact.0, timestamp],
            )
    }

    pub fn status_last_seen(&self) -> Result<Vec<(wr::JID, i64)>, rusqlite::Error> {
        let mut query = self
            .db
            .prepare("SELECT contact_jid, timestamp FROM status_read_cursors")?;
        query
            .query_map([], |row| Ok((row.get::<_, String>(0)?.into(), row.get(1)?)))?
            .collect()
    }

    /// Migrates the `message_actions` table to the current schema. Any
    /// stored replacement body is folded into the message row first, then
    /// the now-unused column is removed. Runs under the write lock from
    /// `init`, before any handler reads the actions. The steps are
    /// idempotent: an interrupted migration resumes safely on the next open.
    fn migrate_message_action_columns(&self) {
        let has_replacement = self
            .db
            .prepare(
                "SELECT COUNT(*) FROM pragma_table_info('message_actions') WHERE name = 'replacement'",
            )
            .unwrap()
            .query_row([], |row| row.get::<_, i64>(0))
            .unwrap()
            > 0;
        if !has_replacement {
            return;
        }
        let mut effective_bodies: HashMap<String, String> = HashMap::new();
        let mut deleted_targets: HashSet<String> = HashSet::new();
        {
            let mut query = self
                .db
                .prepare(
                    "SELECT target_message_id, kind, replacement FROM message_actions ORDER BY occurred_at, arrival_order, action_id",
                )
                .unwrap();
            let rows = query
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            for (target, kind, replacement) in rows {
                // kind 0 is Edit; the last edit in the stable order wins.
                if kind == 1 {
                    deleted_targets.insert(target.clone());
                } else if kind == 0
                    && let Some(body) = replacement
                {
                    effective_bodies.insert(target, body);
                }
            }
        }
        for (target, body) in &effective_bodies {
            if deleted_targets.contains(target) {
                continue;
            }
            self.db
                .execute(
                    "UPDATE text_messages SET message = ?1 WHERE id = ?2",
                    rusqlite::params![body, target],
                )
                .unwrap();
        }
        for target in deleted_targets {
            self.db
                .execute(
                    "UPDATE text_messages SET message = ?1, quote_id = NULL WHERE id = ?2",
                    rusqlite::params![DELETED_MESSAGE_TEXT, target],
                )
                .unwrap();
            self.db
                .execute(
                    "DELETE FROM file_messages WHERE id = ?1",
                    rusqlite::params![target],
                )
                .unwrap();
        }
        self.db
            .execute("ALTER TABLE message_actions DROP COLUMN replacement", [])
            .unwrap();
    }

    fn migrate_forwarding_columns(&self) {
        ensure_forwarding_columns(&self.db, "text_messages");
        ensure_forwarding_columns(&self.db, "file_messages");
    }
}

impl DatabaseHandler {
    /// Deletes `status@broadcast` messages older than the 24-hour WhatsApp
    /// retention window. Returns the relative media paths of the purged
    /// file messages so the caller can remove them from disk.
    pub fn purge_expired_statuses(&mut self, now: i64) -> Vec<PathBuf> {
        let cutoff = now - STATUS_RETENTION_SECS;
        let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
        let tx = self
            .db
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
}
