use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use log::debug;
use rusqlite::Connection;
use whatsrust as wr;

use crate::app::{Chat, DELETED_MESSAGE_TEXT, MessageAction};

#[path = "db/action_repository.rs"]
mod action_repository;
#[path = "db/chat_store.rs"]
mod chat_store;
#[path = "db/connection.rs"]
mod connection;
#[path = "db/cursor_repository.rs"]
mod cursor_repository;
#[path = "db/message_store.rs"]
mod message_store;
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
                    chat_store::persist(&mut db, new_chats);
                }

                let messages = {
                    let mut queue = new_messages_queue_clone.lock().unwrap();
                    std::mem::take(&mut *queue)
                };
                if !messages.is_empty() {
                    debug!("Saving {} new messages to the database", messages.len());
                    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
                    message_store::persist(&mut db, messages);
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
        message_store::get_messages(&self.db)
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
