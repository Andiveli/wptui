use std::thread::{self, JoinHandle};
use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
    pub display_content: wr::MessageContent,
    pub display_mention_ranges: Vec<Range<usize>>,
}

impl super::App<'_> {
    #[cfg(test)]
    pub(crate) fn stage_text_send(
        &mut self,
        chat: wr::JID,
        content: wr::MessageContent,
        quote: Option<wr::Message>,
        mentions: Vec<wr::Mention>,
        mention_ranges: Vec<Range<usize>>,
    ) -> bool {
        let display_text = match &content {
            wr::MessageContent::Text(text) => text.clone(),
            wr::MessageContent::File(file) => file.caption.clone().unwrap_or_default(),
            wr::MessageContent::ViewOnceUnavailable => {
                unreachable!("view-once unavailable content cannot be staged for outbound text")
            }
        };
        self.stage_text_send_with_display(
            chat,
            content,
            quote,
            mentions,
            mention_ranges.clone(),
            display_text,
            mention_ranges,
        )
    }

    pub(crate) fn stage_text_send_with_display(
        &mut self,
        chat: wr::JID,
        content: wr::MessageContent,
        quote: Option<wr::Message>,
        mentions: Vec<wr::Mention>,
        mention_ranges: Vec<Range<usize>>,
        display_text: impl Into<Arc<str>>,
        display_mention_ranges: Vec<Range<usize>>,
    ) -> bool {
        self.stage_outbound_batch_with_display(
            chat,
            vec![content],
            quote,
            mentions,
            mention_ranges,
            vec![wr::MessageContent::Text(display_text.into())],
            vec![display_mention_ranges],
        )
    }

    pub(crate) fn stage_outbound_batch(
        &mut self,
        chat: wr::JID,
        contents: Vec<wr::MessageContent>,
        quote: Option<wr::Message>,
        mentions: Vec<wr::Mention>,
        mention_ranges: Vec<Range<usize>>,
    ) -> bool {
        let display_contents = contents.clone();
        let display_mention_ranges = vec![mention_ranges.clone(); contents.len()];
        self.stage_outbound_batch_with_display(
            chat,
            contents,
            quote,
            mentions,
            mention_ranges,
            display_contents,
            display_mention_ranges,
        )
    }

    fn stage_outbound_batch_with_display(
        &mut self,
        chat: wr::JID,
        contents: Vec<wr::MessageContent>,
        quote: Option<wr::Message>,
        mentions: Vec<wr::Mention>,
        mention_ranges: Vec<Range<usize>>,
        display_contents: Vec<wr::MessageContent>,
        display_mention_ranges: Vec<Vec<Range<usize>>>,
    ) -> bool {
        if contents.is_empty()
            || contents.len() != display_contents.len()
            || contents.len() != display_mention_ranges.len()
        {
            return false;
        }
        let Some(local_send_ids) = self.allocate_local_send_ids(contents.len()) else {
            self.unavailable("Could not send message");
            return false;
        };
        let requests = local_send_ids
            .into_iter()
            .zip(contents)
            .zip(display_contents.into_iter().zip(display_mention_ranges))
            .map(
                |((local_send_id, content), (display_content, display_mention_ranges))| {
                    TextSendRequest {
                        local_send_id,
                        chat: chat.clone(),
                        content,
                        quote: quote.clone(),
                        mentions: mentions.clone(),
                        mention_ranges: mention_ranges.clone(),
                        display_content,
                        display_mention_ranges,
                    }
                },
            )
            .collect::<Vec<_>>();
        for request in &requests {
            self.pending_outgoing_text
                .insert(request.local_send_id, request.clone());
        }
        if !self
            .optimistic_text_send_worker
            .enqueue_batch(requests.clone())
        {
            for request in requests {
                self.pending_outgoing_text.remove(&request.local_send_id);
            }
            self.unavailable("Could not send message");
            return false;
        }
        true
    }

    fn allocate_local_send_ids(&mut self, count: usize) -> Option<Vec<u64>> {
        let mut ids = Vec::with_capacity(count);
        let mut candidate = self.next_local_send_id;
        let attempts = self
            .pending_outgoing_text
            .len()
            .saturating_add(self.completed_text_send_ids.len())
            .saturating_add(count);
        for _ in 0..attempts {
            if candidate != 0
                && !self.pending_outgoing_text.contains_key(&candidate)
                && !self.completed_text_send_ids.contains(&candidate)
                && !ids.contains(&candidate)
            {
                ids.push(candidate);
                if ids.len() == count {
                    self.next_local_send_id = candidate.checked_add(1).unwrap_or(1);
                    return Some(ids);
                }
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
        let Some(_request) = self.pending_outgoing_text.remove(&local_send_id) else {
            return false;
        };
        self.remember_completed(local_send_id);
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
                if let wr::MessageContent::Text(text) = &request.display_content {
                    wr::store_message_mention_ranges(
                        &id,
                        text,
                        request.display_mention_ranges.clone(),
                    );
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
                    message: request.display_content.clone(),
                }
            })
            .collect()
    }

    pub(crate) fn is_pending_message_id(id: &wr::MessageId) -> bool {
        id.starts_with("local-send-")
    }
}

pub trait TextSendPort: Send {
    fn send(&mut self, request: &TextSendRequest) -> Result<(), wr::OutboundSendFailure>;
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
    queued: Arc<AtomicUsize>,
    #[allow(dead_code)]
    exited: Arc<AtomicBool>,
}

enum Command {
    SendBatch(Vec<TextSendRequest>),
}

impl Worker {
    pub fn new(app_tx: mpsc::Sender<AppInput>, port: Box<dyn TextSendPort>) -> Self {
        let (tx, rx) = mpsc::sync_channel(MAX_QUEUED_TEXT_SENDS);
        let cancelled = CancellationGate::default();
        let exited = Arc::new(AtomicBool::new(false));
        let queued = Arc::new(AtomicUsize::new(0));
        let worker_cancelled = cancelled.clone();
        let worker_exited = Arc::clone(&exited);
        let worker_queued = Arc::clone(&queued);
        let join = thread::spawn(move || {
            run(
                rx,
                app_tx,
                port,
                worker_cancelled,
                worker_queued,
                worker_exited,
            )
        });
        Self {
            tx: Some(tx),
            join: Some(join),
            cancelled,
            queued,
            exited,
        }
    }

    pub fn enqueue(&self, request: TextSendRequest) -> bool {
        self.enqueue_batch(vec![request])
    }

    pub fn enqueue_batch(&self, requests: Vec<TextSendRequest>) -> bool {
        let count = requests.len();
        if count == 0 || count > MAX_QUEUED_TEXT_SENDS || !reserve_queue_slots(&self.queued, count)
        {
            return false;
        }
        let queued = self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(Command::SendBatch(requests)).is_ok());
        if !queued {
            self.queued.fetch_sub(count, Ordering::AcqRel);
        }
        queued
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

fn reserve_queue_slots(queued: &AtomicUsize, count: usize) -> bool {
    let mut current = queued.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(count) else {
            return false;
        };
        if next > MAX_QUEUED_TEXT_SENDS {
            return false;
        }
        match queued.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(observed) => current = observed,
        }
    }
}

fn run(
    rx: Receiver<Command>,
    app_tx: mpsc::Sender<AppInput>,
    mut port: Box<dyn TextSendPort>,
    cancelled: CancellationGate,
    queued: Arc<AtomicUsize>,
    exited: Arc<AtomicBool>,
) {
    while let Ok(command) = rx.recv() {
        let Command::SendBatch(requests) = command;
        queued.fetch_sub(requests.len(), Ordering::AcqRel);
        for request in requests {
            if !cancelled.admit() {
                exited.store(true, Ordering::Release);
                return;
            }
            if port.send(&request).is_err()
                && app_tx
                    .send(AppInput::App(AppEvent::OutboundSendFailed {
                        local_send_id: request.local_send_id,
                    }))
                    .is_err()
            {
                exited.store(true, Ordering::Release);
                return;
            }
        }
    }
    exited.store(true, Ordering::Release);
}

pub struct WhatsAppTextSendPort;

impl TextSendPort for WhatsAppTextSendPort {
    fn send(&mut self, request: &TextSendRequest) -> Result<(), wr::OutboundSendFailure> {
        wr::send_outbound_message(
            &request.chat,
            &request.content,
            request.quote.as_ref(),
            &request.mentions,
            request.local_send_id,
        )
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
        fn send(&mut self, request: &TextSendRequest) -> Result<(), wr::OutboundSendFailure> {
            self.0.lock().unwrap().push(request.local_send_id);
            Err(wr::OutboundSendFailure::TransportFailed)
        }
    }

    struct BlockingPort {
        started: std::sync::mpsc::Sender<()>,
        release: Arc<(Mutex<bool>, Condvar)>,
        calls: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl TextSendPort for BlockingPort {
        fn send(&mut self, _: &TextSendRequest) -> Result<(), wr::OutboundSendFailure> {
            // Models the production Go port returning on its five-second context
            // deadline/cancellation contract; the fake release makes that return deterministic.
            self.calls.fetch_add(1, Ordering::Relaxed);
            let _ = self.started.send(());
            let (lock, wake) = &*self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = wake.wait(released).unwrap();
            }
            Err(wr::OutboundSendFailure::TransportFailed)
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
            display_content: wr::MessageContent::Text("text".into()),
            display_mention_ranges: Vec::new(),
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
            AppInput::App(AppEvent::OutboundSendFailed { local_send_id: 1 })
        ));
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            AppInput::App(AppEvent::OutboundSendFailed { local_send_id: 2 })
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
        app.stage_text_send_with_display(
            chat.clone(),
            wr::MessageContent::Text("@111".into()),
            None,
            vec![],
            vec![0..4],
            "@Álvaro",
            vec![0.."@Álvaro".len()],
        );
        assert_eq!(app.pending_messages_for_chat(&chat).len(), 1);
        let pending = app.pending_messages_for_chat(&chat).pop().unwrap();
        let wr::MessageContent::Text(text) = &pending.message else {
            panic!("expected pending text");
        };
        assert_eq!(text.as_ref(), "@Álvaro");
        assert_eq!(
            wr::message_mention_ranges(&pending.info.id, text),
            vec![0..8]
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
    fn batch_admission_preserves_files_quote_mentions_and_order() {
        let mut app = TestApp::new();
        let chat: wr::JID = "chat@g.us".to_owned().into();
        let quote = wr::Message {
            info: wr::MessageInfo {
                id: "quoted".into(),
                chat: chat.clone(),
                sender: chat.clone(),
                mentions_self: false,
                timestamp: 0,
                is_from_me: false,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text("quoted".into()),
        };
        let mentions = vec![wr::Mention {
            jid: "111@s.whatsapp.net".to_owned().into(),
            numeric_user: "111".into(),
        }];
        let messages = vec![
            wr::MessageContent::File(wr::FileContent {
                kind: wr::FileKind::Image,
                path: "first.png".into(),
                file_id: "".into(),
                caption: Some("@111 caption".into()),
            }),
            wr::MessageContent::File(wr::FileContent {
                kind: wr::FileKind::Document,
                path: "second.pdf".into(),
                file_id: "".into(),
                caption: None,
            }),
        ];

        assert!(app.stage_outbound_batch(
            chat.clone(),
            messages,
            Some(quote),
            mentions.clone(),
            vec![0..4]
        ));
        let pending = app.pending_messages_for_chat(&chat);
        assert_eq!(pending.len(), 2);
        assert!(matches!(
            &pending[0].message,
            wr::MessageContent::File(file)
                if matches!(file.kind, wr::FileKind::Image)
                    && file.path.as_ref() == "first.png"
                    && file.caption.as_deref() == Some("@111 caption")
        ));
        assert!(matches!(
            &pending[1].message,
            wr::MessageContent::File(file)
                if matches!(file.kind, wr::FileKind::Document)
                    && file.path.as_ref() == "second.pdf"
                    && file.caption.is_none()
        ));
        let staged = app.pending_outgoing_text.values().collect::<Vec<_>>();
        assert!(staged.iter().all(|request| request.quote.is_some()));
        assert!(staged.iter().all(|request| request.mentions == mentions));
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
    fn canonical_callback_ranges_are_not_overwritten_by_composer_ranges() {
        let mut app = TestApp::new();
        let chat: wr::JID = "chat@g.us".to_owned().into();
        app.stage_text_send(
            chat.clone(),
            wr::MessageContent::Text("@111".into()),
            None,
            vec![],
            vec![0..4],
        );
        let local_send_id = *app.pending_outgoing_text.keys().next().unwrap();
        let canonical = wr::Message {
            info: wr::MessageInfo {
                id: "server-mention".into(),
                chat: chat.clone(),
                sender: chat,
                mentions_self: false,
                timestamp: 1,
                is_from_me: true,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text("@阿丽".into()),
        };

        wr::store_message_mention_ranges(&canonical.info.id, "@阿丽", vec![0.."@阿丽".len()]);
        assert!(app.complete_text_send(local_send_id, canonical.clone()));
        assert_eq!(
            wr::message_mention_ranges(&canonical.info.id, "@阿丽"),
            vec![0.."@阿丽".len()]
        );
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
