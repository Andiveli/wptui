use std::ffi::{CString, c_char};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::abi::{C_MarkAsRead, C_MarkChatReadSync};
use crate::models::{JID, MessageId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkAsReadError {
    Disconnected,
    Transient,
    Permanent,
}

fn with_borrowed_mark_read_args<T>(
    msg_id: &MessageId,
    chat_jid: &JID,
    sender_jid: &JID,
    send: impl FnOnce(*const c_char, *const c_char, *const c_char) -> T,
) -> Result<T, MarkAsReadError> {
    let msg_id_c = CString::new(msg_id.as_ref()).map_err(|_| MarkAsReadError::Permanent)?;
    let chat_jid_c = CString::new(chat_jid.0.as_ref()).map_err(|_| MarkAsReadError::Permanent)?;
    let sender_jid_c =
        CString::new(sender_jid.0.as_ref()).map_err(|_| MarkAsReadError::Permanent)?;
    Ok(send(
        msg_id_c.as_ptr(),
        chat_jid_c.as_ptr(),
        sender_jid_c.as_ptr(),
    ))
}

pub fn mark_as_read(
    msg_id: &MessageId,
    chat_jid: &JID,
    sender_jid: &JID,
) -> Result<(), MarkAsReadError> {
    let result =
        with_borrowed_mark_read_args(msg_id, chat_jid, sender_jid, |id, chat, sender| unsafe {
            C_MarkAsRead(id, chat, sender)
        })?;
    match result {
        0 => Ok(()),
        1 => Err(MarkAsReadError::Disconnected),
        3 => Err(MarkAsReadError::Permanent),
        _ => Err(MarkAsReadError::Transient),
    }
}

const READ_SYNC_QUEUE_CAPACITY: usize = 64;

struct ReadSyncRequest {
    chat: String,
    message: String,
    timestamp: i64,
    from_me: bool,
    participant: Option<String>,
}

impl ReadSyncRequest {
    fn new(
        chat_jid: &JID,
        message_id: &MessageId,
        timestamp: i64,
        from_me: bool,
        participant_jid: Option<&JID>,
    ) -> Self {
        Self {
            chat: chat_jid.0.to_string(),
            message: message_id.to_string(),
            timestamp,
            from_me,
            participant: participant_jid.map(|jid| jid.0.to_string()),
        }
    }

    fn send(self) {
        let chat_c = match CString::new(self.chat) {
            Ok(value) => value,
            Err(_) => {
                log::warn!("chat read sync skipped: invalid chat JID");
                return;
            }
        };
        let message_c = match CString::new(self.message) {
            Ok(value) => value,
            Err(_) => {
                log::warn!("chat read sync skipped: invalid message ID");
                return;
            }
        };
        let participant_c = self.participant.and_then(|value| CString::new(value).ok());
        let participant_ptr = participant_c
            .as_ref()
            .map_or(std::ptr::null(), |value| value.as_ptr());
        let result = unsafe {
            C_MarkChatReadSync(
                chat_c.as_ptr(),
                message_c.as_ptr(),
                self.timestamp,
                self.from_me,
                participant_ptr,
            )
        };
        if result != 0 {
            log::warn!("chat read sync failed with bridge status {result}");
        }
    }
}

/// Owns queued chat-read bridge calls for one application runtime.
pub struct ReadSyncWorker {
    tx: Mutex<Option<SyncSender<ReadSyncRequest>>>,
    cancelled: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl ReadSyncWorker {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel(READ_SYNC_QUEUE_CAPACITY);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let join = thread::spawn(move || run(rx, worker_cancelled));
        Self {
            tx: Mutex::new(Some(tx)),
            cancelled,
            join: Some(join),
        }
    }

    /// Queues a read update without blocking the terminal event loop.
    pub fn schedule(
        &self,
        chat_jid: &JID,
        message_id: &MessageId,
        timestamp: i64,
        from_me: bool,
        participant_jid: Option<&JID>,
    ) -> bool {
        if self.cancelled.load(Ordering::Acquire) {
            return false;
        }
        self.tx.lock().is_ok_and(|tx| {
            tx.as_ref().is_some_and(|tx| {
                tx.try_send(ReadSyncRequest::new(
                    chat_jid,
                    message_id,
                    timestamp,
                    from_me,
                    participant_jid,
                ))
                .is_ok()
            })
        })
    }

    pub fn is_shutdown(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Cancels queued calls and joins after the currently bounded bridge call returns.
    pub fn shutdown(&mut self) {
        if self.join.is_none() {
            return;
        }
        self.cancelled.store(true, Ordering::Release);
        if let Ok(mut tx) = self.tx.lock() {
            tx.take();
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for ReadSyncWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(rx: Receiver<ReadSyncRequest>, cancelled: Arc<AtomicBool>) {
    while let Ok(request) = rx.recv() {
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        request.send();
    }
}

/// Executes one fully adapted chat-read bridge call synchronously.
///
/// Application UI code should instead schedule work through [`ReadSyncWorker`].
pub fn sync_chat_read(
    chat_jid: &JID,
    message_id: &MessageId,
    timestamp: i64,
    from_me: bool,
    participant_jid: Option<&JID>,
) {
    ReadSyncRequest::new(chat_jid, message_id, timestamp, from_me, participant_jid).send();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_ffi_arguments_can_be_reused_without_owned_pointer_leaks() {
        let id: MessageId = "message".into();
        let chat = JID::from("chat@s.whatsapp.net".to_owned());
        let sender = JID::from("sender@s.whatsapp.net".to_owned());
        for _ in 0..1_000 {
            with_borrowed_mark_read_args(&id, &chat, &sender, |id, chat, sender| {
                assert!(!id.is_null() && !chat.is_null() && !sender.is_null());
            })
            .unwrap();
        }
    }

    #[test]
    fn shutdown_joins_the_read_sync_worker() {
        let mut worker = ReadSyncWorker::new();

        worker.shutdown();

        assert!(worker.join.is_none());
    }

    #[test]
    fn shutdown_rejects_later_jobs_without_calling_the_bridge() {
        let mut worker = ReadSyncWorker::new();
        worker.shutdown();

        assert!(!worker.schedule(
            &JID::from("chat@s.whatsapp.net".to_owned()),
            &MessageId::from("message"),
            1,
            false,
            None,
        ));
    }
}
