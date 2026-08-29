use std::path::Path;
use std::sync::{Arc, Mutex};

use log::debug;

use crate::app::Chat;
use whatsrust as wr;

use super::{DATABASE_WRITE_LOCK, chat_store, message_store, open_database};

pub(super) struct Worker {
    messages: Arc<Mutex<Vec<wr::Message>>>,
    chats: Arc<Mutex<Vec<Chat>>>,
    stop_requested: Arc<Mutex<bool>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Worker {
    pub(super) fn new(db_path: &Path) -> Self {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let chats = Arc::new(Mutex::new(Vec::new()));
        let stop_requested = Arc::new(Mutex::new(false));
        let thread_messages = Arc::clone(&messages);
        let thread_chats = Arc::clone(&chats);
        let thread_stop = Arc::clone(&stop_requested);
        let db_path = db_path.to_owned();
        let thread = std::thread::spawn(move || {
            let mut db = open_database(&db_path);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let new_chats = {
                    let mut queue = thread_chats.lock().unwrap();
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
                    let mut queue = thread_messages.lock().unwrap();
                    std::mem::take(&mut *queue)
                };
                if !messages.is_empty() {
                    debug!("Saving {} new messages to the database", messages.len());
                    let _write_lock = DATABASE_WRITE_LOCK.lock().unwrap();
                    message_store::persist(&mut db, messages);
                }
                if *thread_stop.lock().unwrap() {
                    break;
                }
            }
        });
        Self {
            messages,
            chats,
            stop_requested,
            thread: Some(thread),
        }
    }

    pub(super) fn queue_message(&self, message: wr::Message) {
        self.messages.lock().unwrap().push(message);
    }

    pub(super) fn queue_chat(&self, chat: Chat) {
        self.chats.lock().unwrap().push(chat);
    }

    pub(super) fn stop(&mut self) {
        *self.stop_requested.lock().unwrap() = true;
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            log::error!("Database writer worker terminated unexpectedly during shutdown");
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::Worker;

    #[test]
    fn dropping_worker_requests_shutdown() {
        let tempdir = tempfile::tempdir().unwrap();
        let worker = Worker::new(&tempdir.path().join("worker.db"));
        let stop_requested = worker.stop_requested.clone();

        drop(worker);

        assert!(*stop_requested.lock().unwrap());
    }

    #[test]
    fn stopping_worker_twice_is_idempotent() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut worker = Worker::new(&tempdir.path().join("worker.db"));

        worker.stop();
        worker.stop();

        assert!(*worker.stop_requested.lock().unwrap());
    }
}
