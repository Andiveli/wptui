use std::thread::{self, JoinHandle};
use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
};

use whatsrust as wr;

use super::events::{AppEvent, AppInput};

pub const MAX_QUEUED_TEXT_SENDS: usize = 64;
const MAX_COMPLETED_TEXT_SENDS: usize = 256;

#[derive(Clone, Debug)]
pub struct TextSendRequest {
    pub local_send_id: u64,
    pub chat: wr::JID,
    pub content: wr::MessageContent,
    pub quote: Option<wr::Message>,
    pub mentions: Vec<wr::Mention>,
    pub mention_ranges: Vec<Range<usize>>,
}

impl super::App<'_> {
    pub(crate) fn stage_text_send(
        &mut self,
        chat: wr::JID,
        content: wr::MessageContent,
        quote: Option<wr::Message>,
        mentions: Vec<wr::Mention>,
        mention_ranges: Vec<Range<usize>>,
    ) -> bool {
        let Some(local_send_id) = self.allocate_local_send_id() else {
            self.unavailable("Could not send message");
            return false;
        };
        let request = TextSendRequest {
            local_send_id,
            chat,
            content,
            quote,
            mentions,
            mention_ranges,
        };
        self.pending_outgoing_text
            .insert(local_send_id, request.clone());
        if !self.optimistic_text_send_worker.enqueue(request) {
            self.pending_outgoing_text.remove(&local_send_id);
            self.unavailable("Could not send message");
            return false;
        }
        true
    }

    fn allocate_local_send_id(&mut self) -> Option<u64> {
        let mut candidate = self.next_local_send_id;
        let attempts = self
            .pending_outgoing_text
            .len()
            .saturating_add(self.completed_text_send_ids.len())
            .saturating_add(1);
        for _ in 0..attempts {
            if candidate != 0
                && !self.pending_outgoing_text.contains_key(&candidate)
                && !self.completed_text_send_ids.contains(&candidate)
            {
                self.next_local_send_id = candidate.checked_add(1).unwrap_or(1);
                return Some(candidate);
            }
            candidate = candidate.checked_add(1).unwrap_or(1);
        }
        None
    }

    fn remember_completed(&mut self, local_send_id: u64) {
        if !self.completed_text_send_ids.contains(&local_send_id) {
            self.completed_text_send_ids.push_back(local_send_id);
            while self.completed_text_send_ids.len() > MAX_COMPLETED_TEXT_SENDS {
                self.completed_text_send_ids.pop_front();
            }
        }
    }

    pub(crate) fn complete_text_send(&mut self, local_send_id: u64, message: wr::Message) -> bool {
        if self.completed_text_send_ids.contains(&local_send_id) {
            return false;
        }
        let Some(request) = self.pending_outgoing_text.remove(&local_send_id) else {
            return false;
        };
        self.remember_completed(local_send_id);
        if let wr::MessageContent::Text(text) = &message.message {
            wr::store_message_mention_ranges(&message.info.id, text, request.mention_ranges);
        }
        if self.messages.contains_key(&message.info.id) {
            false
        } else {
            self.process_message_with_lookup(message, false, |_| Default::default())
        }
    }

    pub(crate) fn fail_text_send(&mut self, local_send_id: u64) -> bool {
        if self.completed_text_send_ids.contains(&local_send_id) {
            return false;
        }
        if self.pending_outgoing_text.remove(&local_send_id).is_some() {
            self.remember_completed(local_send_id);
            self.unavailable("Could not send message");
            true
        } else {
            false
        }
    }

    pub(crate) fn pending_messages_for_chat(&self, chat: &wr::JID) -> Vec<wr::Message> {
        let mut requests = self
            .pending_outgoing_text
            .values()
            .filter(|request| &request.chat == chat)
            .collect::<Vec<_>>();
        requests.sort_by_key(|request| request.local_send_id);
        requests
            .into_iter()
            .map(|request| {
                let id: wr::MessageId = format!("local-send-{}", request.local_send_id).into();
                if let wr::MessageContent::Text(text) = &request.content {
                    wr::store_message_mention_ranges(&id, text, request.mention_ranges.clone());
                }
                wr::Message {
                    info: wr::MessageInfo {
                        id,
                        chat: request.chat.clone(),
                        sender: request.chat.clone(),
                        mentions_self: false,
                        timestamp: self.now(),
                        is_from_me: true,
                        quote_id: request.quote.as_ref().map(|quote| quote.info.id.clone()),
                        read_by: 0,
                        forwarding: Default::default(),
                    },
                    message: request.content.clone(),
                }
            })
            .collect()
    }

    pub(crate) fn is_pending_message_id(id: &wr::MessageId) -> bool {
        id.starts_with("local-send-")
    }
}

pub trait TextSendPort: Send {
    fn send(&mut self, request: &TextSendRequest) -> bool;
}

#[derive(Clone, Default)]
struct CancellationGate(Arc<std::sync::Mutex<bool>>);

impl CancellationGate {
    fn admit(&self) -> bool {
        !*self.0.lock().unwrap()
    }

    fn cancel(&self) {
        *self.0.lock().unwrap() = true;
    }
}

pub struct Worker {
    tx: Option<SyncSender<Command>>,
    join: Option<JoinHandle<()>>,
    cancelled: CancellationGate,
    #[allow(dead_code)]
    exited: Arc<AtomicBool>,
}

enum Command {
    Send(TextSendRequest),
}

impl Worker {
    pub fn new(app_tx: mpsc::Sender<AppInput>, port: Box<dyn TextSendPort>) -> Self {
        let (tx, rx) = mpsc::sync_channel(MAX_QUEUED_TEXT_SENDS);
        let cancelled = CancellationGate::default();
        let exited = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker_exited = Arc::clone(&exited);
        let join = thread::spawn(move || run(rx, app_tx, port, worker_cancelled, worker_exited));
        Self {
            tx: Some(tx),
            join: Some(join),
            cancelled,
            exited,
        }
    }

    pub fn enqueue(&self, request: TextSendRequest) -> bool {
        self.tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(Command::Send(request)).is_ok())
    }

    #[cfg(test)]
    fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    pub fn shutdown(&mut self) {
        self.cancelled.cancel();
        self.tx.take();
        // Never join: the current Go call may run until its bounded context deadline.
        let _ = self.join.take();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(
    rx: Receiver<Command>,
    app_tx: mpsc::Sender<AppInput>,
    mut port: Box<dyn TextSendPort>,
    cancelled: CancellationGate,
    exited: Arc<AtomicBool>,
) {
    while let Ok(command) = rx.recv() {
        match command {
            Command::Send(request) => {
                if !cancelled.admit() {
                    break;
                }
                let ok = port.send(&request);
                if !cancelled.admit() {
                    break;
                }
                if !ok {
                    if app_tx
                        .send(AppInput::App(AppEvent::TextSendFailed {
                            local_send_id: request.local_send_id,
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    }
    exited.store(true, Ordering::Release);
}

pub struct WhatsAppTextSendPort;

impl TextSendPort for WhatsAppTextSendPort {
    fn send(&mut self, request: &TextSendRequest) -> bool {
        wr::send_text_message(
            &request.chat,
            &request.content,
            request.quote.as_ref(),
            &request.mentions,
            request.local_send_id,
        ) == wr::TextSendResult::Sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::ActionNotice;
    use crate::app::test_support::TestApp;
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, Instant};

    struct Port(Arc<Mutex<Vec<u64>>>);
    impl TextSendPort for Port {
        fn send(&mut self, request: &TextSendRequest) -> bool {
            self.0.lock().unwrap().push(request.local_send_id);
            false
        }
    }

    struct BlockingPort {
        started: std::sync::mpsc::Sender<()>,
        release: Arc<(Mutex<bool>, Condvar)>,
        calls: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl TextSendPort for BlockingPort {
        fn send(&mut self, _: &TextSendRequest) -> bool {
            // Models the production Go port returning on its five-second context
            // deadline/cancellation contract; the fake release makes that return deterministic.
            self.calls.fetch_add(1, Ordering::Relaxed);
            let _ = self.started.send(());
            let (lock, wake) = &*self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = wake.wait(released).unwrap();
            }
            false
        }
    }

    fn request(id: u64) -> TextSendRequest {
        TextSendRequest {
            local_send_id: id,
            chat: "chat@g.us".to_owned().into(),
            content: wr::MessageContent::Text("text".into()),
            quote: None,
            mentions: Vec::new(),
            mention_ranges: Vec::new(),
        }
    }

    #[test]
    fn worker_preserves_order_and_shutdown_is_bounded() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel();
        let mut worker = Worker::new(tx, Box::new(Port(Arc::clone(&seen))));
        assert!(worker.enqueue(request(1)));
        assert!(worker.enqueue(request(2)));
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            AppInput::App(AppEvent::TextSendFailed { local_send_id: 1 })
        ));
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            AppInput::App(AppEvent::TextSendFailed { local_send_id: 2 })
        ));
        worker.shutdown();
        assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn blocked_network_shutdown_returns_promptly_and_queue_is_bounded() {
        let (app_tx, _app_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut worker = Worker::new(
            app_tx,
            Box::new(BlockingPort {
                started: started_tx,
                release: Arc::clone(&release),
                calls: Arc::clone(&calls),
            }),
        );
        assert!(worker.enqueue(request(1)));
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let started = Instant::now();
        let mut rejected = false;
        for id in 2..=(MAX_QUEUED_TEXT_SENDS as u64 + 2) {
            if !worker.enqueue(request(id)) {
                rejected = true;
                break;
            }
        }
        assert!(rejected);
        worker.shutdown();
        assert!(started.elapsed() < Duration::from_millis(250));
        let (lock, wake) = &*release;
        *lock.lock().unwrap() = true;
        wake.notify_all();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !worker.has_exited() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(worker.has_exited());
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn cancellation_gate_rejects_admission_racing_shutdown() {
        let gate = CancellationGate::default();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let worker_gate = gate.clone();
        let worker_barrier = Arc::clone(&barrier);
        let admitted = std::thread::spawn(move || {
            worker_barrier.wait();
            worker_gate.admit()
        });
        gate.cancel();
        barrier.wait();
        assert!(!admitted.join().unwrap());
    }

    #[test]
    fn pending_text_is_local_only_and_failure_removes_it() {
        let mut app = TestApp::new();
        let chat: wr::JID = "chat@g.us".to_owned().into();
        app.stage_text_send(
            chat.clone(),
            wr::MessageContent::Text("@111".into()),
            None,
            vec![],
            vec![0..4],
        );
        assert_eq!(app.pending_messages_for_chat(&chat).len(), 1);
        let pending = app.pending_messages_for_chat(&chat).pop().unwrap();
        let wr::MessageContent::Text(text) = &pending.message else {
            panic!("expected pending text");
        };
        assert_eq!(
            wr::message_mention_ranges(&pending.info.id, text),
            vec![0..4]
        );
        assert!(!app.chat_messages.contains_key(&chat));
        assert!(!app.messages.keys().any(|id| id.starts_with("local-send-")));
        let local_send_id = *app.pending_outgoing_text.keys().next().unwrap();
        assert!(app.fail_text_send(local_send_id));
        assert!(app.pending_messages_for_chat(&chat).is_empty());
        assert_eq!(
            app.action_notice,
            Some(ActionNotice::Unavailable("Could not send message".into()))
        );
    }

    #[test]
    fn canonical_callback_reconciles_by_local_id_without_content_fifo() {
        let mut app = TestApp::new();
        let chat: wr::JID = "chat@g.us".to_owned().into();
        app.stage_text_send(
            chat.clone(),
            wr::MessageContent::Text("first".into()),
            None,
            vec![],
            vec![],
        );
        app.stage_text_send(
            chat.clone(),
            wr::MessageContent::Text("second".into()),
            None,
            vec![],
            vec![],
        );
        let pending = app.pending_messages_for_chat(&chat);
        assert!(
            matches!(&pending[0].message, wr::MessageContent::Text(text) if text.as_ref() == "first")
        );
        assert!(
            matches!(&pending[1].message, wr::MessageContent::Text(text) if text.as_ref() == "second")
        );
        let local_send_id = app
            .pending_outgoing_text
            .iter()
            .find(|(_, request)| matches!(&request.content, wr::MessageContent::Text(text) if text.as_ref() == "second"))
            .map(|(id, _)| *id)
            .unwrap();
        assert!(app.fail_text_send(local_send_id));
        assert_eq!(app.pending_messages_for_chat(&chat).len(), 1);
        assert!(
            matches!(&app.pending_messages_for_chat(&chat)[0].message, wr::MessageContent::Text(text) if text.as_ref() == "first")
        );
    }

    #[test]
    fn canonical_success_is_atomic_and_late_failure_or_echo_is_ignored() {
        let mut app = TestApp::new();
        let chat: wr::JID = "chat@g.us".to_owned().into();
        app.stage_text_send(
            chat.clone(),
            wr::MessageContent::Text("hello".into()),
            None,
            vec![],
            vec![],
        );
        let local_send_id = *app.pending_outgoing_text.keys().next().unwrap();
        let canonical = wr::Message {
            info: wr::MessageInfo {
                id: "server-1".into(),
                chat: chat.clone(),
                sender: chat,
                mentions_self: false,
                timestamp: 10,
                is_from_me: true,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text("hello".into()),
        };
        assert!(app.complete_text_send(local_send_id, canonical.clone()));
        assert!(app.pending_outgoing_text.is_empty());
        assert!(app.messages.contains_key("server-1"));
        assert!(!app.fail_text_send(local_send_id));
        assert!(!app.complete_text_send(local_send_id, canonical));
        assert_eq!(app.messages.len(), 1);
    }

    #[test]
    fn unrelated_canonical_message_does_not_reconcile_a_pending_send() {
        let mut app = TestApp::new();
        let chat: wr::JID = "chat@g.us".to_owned().into();
        app.stage_text_send(
            chat.clone(),
            wr::MessageContent::Text("pending".into()),
            None,
            vec![],
            vec![],
        );
        let unrelated = wr::Message {
            info: wr::MessageInfo {
                id: "unrelated".into(),
                chat: chat.clone(),
                sender: chat.clone(),
                mentions_self: false,
                timestamp: 1,
                is_from_me: false,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text("other".into()),
        };
        app.process_message_with_lookup(unrelated, false, |_| Default::default());
        assert_eq!(app.pending_messages_for_chat(&chat).len(), 1);
    }
}
