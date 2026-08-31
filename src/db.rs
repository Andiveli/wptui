use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use rusqlite::Connection;
use whatsrust as wr;

use crate::app::{Chat, DELETED_MESSAGE_TEXT, MessageAction};

#[path = "db/action_repository.rs"]
mod action_repository;
#[path = "db/chat_store.rs"]
mod chat_store;
#[path = "db/chat_store_hydration.rs"]
mod chat_store_hydration;
#[path = "db/chat_store_writer.rs"]
mod chat_store_writer;
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
#[path = "db/worker.rs"]
mod worker;
pub use action_repository::MessageActionPersistence;
pub use chat_store_hydration::SqliteChatStoreHydration;
pub use chat_store_writer::SqliteChatStoreWriter;
pub(crate) use connection::{
    DATABASE_WRITE_LOCK, open_database, try_open_database, with_database_write_lock,
};

pub struct DatabaseHandler {
    db: Connection,
    worker: worker::Worker,
}

impl DatabaseHandler {
    pub fn new(db_path: &Path) -> Self {
        let db = open_database(db_path);
        let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
        schema::prepare(&db);
        Self {
            db,
            worker: worker::Worker::new(db_path),
        }
    }

    pub fn stop(&mut self) {
        self.worker.stop();
    }

    pub fn chat_store_writer(&self) -> SqliteChatStoreWriter {
        SqliteChatStoreWriter::new(self.worker.queue())
    }

    pub fn add_message(&self, message: &wr::Message) {
        schema::prepare_legacy_message_schema(&self.db);
        self.worker.queue_message(message.clone());
    }

    pub fn add_chat(&self, chat: &Chat) {
        self.worker.queue_chat(chat.clone());
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
    pub fn record_message_action(&mut self, action: &MessageAction) -> MessageActionPersistence {
        action_repository::record(&mut self.db, action)
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
        chat_store::get_chats(&self.db)
    }

    pub fn get_messages(&self) -> Vec<wr::Message> {
        message_store::get_messages(&self.db)
    }

    pub fn add_contact(&self, jid: &wr::JID, name: &str) {
        chat_store::add_contact(&self.db, jid, name);
    }

    pub fn get_contacts(&self) -> Vec<(wr::JID, Arc<str>)> {
        chat_store::get_contacts(&self.db)
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
