use std::path::Path;
use std::sync::{Arc, Mutex};

use log::debug;

use crate::app::Chat;
use whatsrust as wr;

use super::{DATABASE_WRITE_LOCK, chat_store, message_store, open_database};

#[derive(Clone)]
pub(super) struct QueueHandle {
    state: Arc<Mutex<QueueState>>,
}

struct QueueState {
    commands: Vec<Command>,
    accepting: bool,
}

enum Command {
    Message(wr::Message),
    Chat(Chat),
    ChatMessage(Chat, wr::Message),
}

impl QueueHandle {
    fn enqueue(&self, command: Command) {
        let mut state = self.state.lock().unwrap();
        if state.accepting {
            state.commands.push(command);
        }
    }

    pub(super) fn queue_message(&self, message: wr::Message) {
        self.enqueue(Command::Message(message));
    }

    pub(super) fn queue_chat(&self, chat: Chat) {
        self.enqueue(Command::Chat(chat));
    }

    pub(super) fn queue_chat_message(&self, chat: Chat, message: wr::Message) {
        self.enqueue(Command::ChatMessage(chat, message));
    }
}

pub(super) struct Worker {
    queue: QueueHandle,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Worker {
    pub(super) fn new(db_path: &Path) -> Self {
        let queue = QueueHandle {
            state: Arc::new(Mutex::new(QueueState {
                commands: Vec::new(),
                accepting: true,
            })),
        };

        let thread_queue = queue.clone();
        let db_path = db_path.to_owned();
        let thread = std::thread::spawn(move || {
            let mut db = open_database(&db_path);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let commands = {
                    let mut state = thread_queue.state.lock().unwrap();
                    std::mem::take(&mut state.commands)
                };
                let mut chats = Vec::new();
                let mut messages = Vec::new();
                let mut chat_messages = Vec::new();
                for command in commands {
                    match command {
                        Command::Chat(chat) => chats.push(chat),
                        Command::Message(message) => messages.push(message),
                        Command::ChatMessage(chat, message) => {
                            chat_messages.push((chat, message));
                        }
                    }
                }
                if !chats.is_empty() {
                    debug!("Saving {} new chats to the database", chats.len());
                    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
                    chat_store::persist(&mut db, chats);
                }
                if !messages.is_empty() {
                    debug!("Saving {} new messages to the database", messages.len());
                    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
                    message_store::persist(&mut db, messages);
                }
                for (chat, message) in chat_messages {
                    debug!("Saving a new chat and message to the database");
                    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
                    persist_chat_message(&mut db, chat, message);
                }
                let should_stop = {
                    let state = thread_queue.state.lock().unwrap();
                    !state.accepting && state.commands.is_empty()
                };
                if should_stop {
                    break;
                }
            }
        });
        Self {
            queue,
            thread: Some(thread),
        }
    }

    pub(super) fn queue(&self) -> QueueHandle {
        self.queue.clone()
    }

    pub(super) fn queue_message(&self, message: wr::Message) {
        self.queue.queue_message(message);
    }

    pub(super) fn queue_chat(&self, chat: Chat) {
        self.queue.queue_chat(chat);
    }

    pub(super) fn stop(&mut self) {
        {
            let mut state = self.queue.state.lock().unwrap();
            state.accepting = false;
        }

        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            log::error!("Database writer worker terminated unexpectedly during shutdown");
        }
    }
}

fn persist_chat_message(db: &mut rusqlite::Connection, chat: Chat, message: wr::Message) {
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    chat_store::persist_in_transaction(&tx, vec![chat]);
    message_store::persist_in_transaction(&tx, vec![message]);
    tx.commit().unwrap();
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::{QueueHandle, Worker, open_database, persist_chat_message};
    use crate::app::Chat;
    use whatsrust as wr;

    fn chat() -> Chat {
        Chat {
            jid: wr::JID::from("chat@example.com".to_owned()),
            last_message_time: None,
        }
    }

    fn message(chat: &Chat) -> wr::Message {
        wr::Message {
            info: wr::MessageInfo {
                id: "message-id".into(),
                chat: chat.jid.clone(),
                sender: wr::JID::from("sender@example.com".to_owned()),
                mentions_self: false,
                timestamp: 1,
                quote_id: None,
                is_from_me: false,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text("message body".into()),
        }
    }

    #[test]
    fn persist_chat_message_commits_the_chat_and_message_together() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut db = open_database(&tempdir.path().join("worker.db"));
        super::super::schema::initialize(&db, crate::app::DELETED_MESSAGE_TEXT);
        let chat = chat();

        persist_chat_message(&mut db, chat.clone(), message(&chat));

        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM chats", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM text_messages", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn cloned_queue_rejects_commands_after_stop() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut worker = Worker::new(&tempdir.path().join("worker.db"));
        let queue: QueueHandle = worker.queue();
        let chat = chat();

        worker.stop();
        queue.queue_chat_message(chat.clone(), message(&chat));

        let state = queue.state.lock().unwrap();
        assert!(!state.accepting);
        assert!(state.commands.is_empty());
    }

    #[test]
    fn stopping_worker_twice_closes_the_queue_and_drains_accepted_commands() {
        let tempdir = tempfile::tempdir().unwrap();
        let db_path = tempdir.path().join("worker.db");
        let db = open_database(&db_path);
        super::super::schema::initialize(&db, crate::app::DELETED_MESSAGE_TEXT);
        drop(db);
        let mut worker = Worker::new(&db_path);
        let queue = worker.queue();
        let chat = chat();

        worker.queue_chat(chat.clone());
        worker.queue_message(message(&chat));
        worker.stop();
        worker.stop();

        let state = queue.state.lock().unwrap();
        assert!(!state.accepting);
        assert!(state.commands.is_empty());
        drop(state);

        let db = open_database(&db_path);
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM chats", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM text_messages", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
