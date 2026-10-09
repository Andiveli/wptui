use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;

use whatsrust as wr;

use crate::app::events::{AppEvent, AppInput};

const MAX_QUEUED_STATUS_SENDS: usize = 64;

enum Command {
    Send(wr::MessageContent),
    Batch(Vec<wr::MessageContent>),
}

pub trait StatusSendPort: Send {
    fn send(&mut self, content: &wr::MessageContent) -> wr::StatusSendResult;
}

pub struct WhatsAppStatusSendPort;

impl StatusSendPort for WhatsAppStatusSendPort {
    fn send(&mut self, content: &wr::MessageContent) -> wr::StatusSendResult {
        wr::send_status(content)
    }
}

pub struct Worker {
    tx: Option<SyncSender<Command>>,
}

impl Worker {
    pub fn new(app_tx: mpsc::Sender<AppInput>, port: Box<dyn StatusSendPort>) -> Self {
        let (tx, rx) = mpsc::sync_channel(MAX_QUEUED_STATUS_SENDS);
        thread::spawn(move || run(rx, app_tx, port));
        Self { tx: Some(tx) }
    }

    pub fn enqueue(&self, content: wr::MessageContent) -> bool {
        self.tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(Command::Send(content)).is_ok())
    }

    pub fn enqueue_batch(&self, content: Vec<wr::MessageContent>) -> bool {
        !content.is_empty()
            && content.len() <= MAX_QUEUED_STATUS_SENDS
            && self
                .tx
                .as_ref()
                .is_some_and(|tx| tx.try_send(Command::Batch(content)).is_ok())
    }
}

fn run(rx: Receiver<Command>, app_tx: mpsc::Sender<AppInput>, mut port: Box<dyn StatusSendPort>) {
    while let Ok(command) = rx.recv() {
        let event = match command {
            Command::Send(content) => match port.send(&content) {
                wr::StatusSendResult::Sent => AppEvent::StatusSendSucceeded,
                result => AppEvent::StatusSendFailed(result),
            },
            Command::Batch(contents) => {
                let mut sent = 0;
                let mut failure = None;
                for content in &contents {
                    match port.send(content) {
                        wr::StatusSendResult::Sent => sent += 1,
                        result => {
                            failure = Some(result);
                            break;
                        }
                    }
                }
                AppEvent::StatusBatchFinished { sent, failure }
            }
        };
        if app_tx.send(AppInput::App(event)).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    use super::*;
    use crate::app::events::{AppEvent, AppInput};
    use whatsrust as wr;

    struct RecordingPort(Arc<Mutex<Vec<wr::MessageContent>>>);

    impl StatusSendPort for RecordingPort {
        fn send(&mut self, content: &wr::MessageContent) -> wr::StatusSendResult {
            self.0.lock().unwrap().push(content.clone());
            wr::StatusSendResult::Sent
        }
    }

    #[test]
    fn worker_submits_text_images_and_videos_as_typed_success_events() {
        let (tx, rx) = mpsc::channel();
        let sent = Arc::new(Mutex::new(Vec::new()));
        let worker = Worker::new(tx, Box::new(RecordingPort(Arc::clone(&sent))));
        let requests = vec![
            wr::MessageContent::Text("hello".into()),
            file("image.png", wr::FileKind::Image),
            file("video.mp4", wr::FileKind::Video),
        ];

        for request in requests.clone() {
            assert!(worker.enqueue(request));
        }
        for _ in &requests {
            assert!(matches!(
                rx.recv_timeout(Duration::from_secs(1)),
                Ok(AppInput::App(AppEvent::StatusSendSucceeded))
            ));
        }
        let sent = sent.lock().unwrap();
        assert!(matches!(&sent[0], wr::MessageContent::Text(text) if text.as_ref() == "hello"));
        assert!(
            matches!(&sent[1], wr::MessageContent::File(file) if file.kind.clone() as u8 == wr::FileKind::Image as u8)
        );
        assert!(
            matches!(&sent[2], wr::MessageContent::File(file) if file.kind.clone() as u8 == wr::FileKind::Video as u8)
        );
    }

    #[test]
    fn worker_returns_a_typed_failure_event() {
        struct FailingPort;
        impl StatusSendPort for FailingPort {
            fn send(&mut self, _: &wr::MessageContent) -> wr::StatusSendResult {
                wr::StatusSendResult::MediaPreparationFailed
            }
        }

        let (tx, rx) = mpsc::channel();
        let worker = Worker::new(tx, Box::new(FailingPort));
        assert!(worker.enqueue(file("image.png", wr::FileKind::Image)));
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)),
            Ok(AppInput::App(AppEvent::StatusSendFailed(
                wr::StatusSendResult::MediaPreparationFailed
            )))
        ));
    }

    #[test]
    fn batch_stops_at_failure_and_reports_confirmed_prefix() {
        struct FailSecondPort(Arc<Mutex<usize>>);
        impl StatusSendPort for FailSecondPort {
            fn send(&mut self, _: &wr::MessageContent) -> wr::StatusSendResult {
                let mut calls = self.0.lock().unwrap();
                *calls += 1;
                if *calls == 2 {
                    wr::StatusSendResult::SendFailed
                } else {
                    wr::StatusSendResult::Sent
                }
            }
        }
        let (tx, rx) = mpsc::channel();
        let calls = Arc::new(Mutex::new(0));
        let worker = Worker::new(tx, Box::new(FailSecondPort(Arc::clone(&calls))));
        assert!(worker.enqueue_batch(vec![
            wr::MessageContent::Text("one".into()),
            wr::MessageContent::Text("two".into()),
            wr::MessageContent::Text("three".into()),
        ]));
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)),
            Ok(AppInput::App(AppEvent::StatusBatchFinished {
                sent: 1,
                failure: Some(wr::StatusSendResult::SendFailed)
            }))
        ));
        assert_eq!(*calls.lock().unwrap(), 2);
    }

    #[test]
    fn oversized_batch_is_rejected_without_sending_a_prefix() {
        let (tx, rx) = mpsc::channel();
        let sent = Arc::new(Mutex::new(Vec::new()));
        let worker = Worker::new(tx, Box::new(RecordingPort(Arc::clone(&sent))));
        let batch = vec![wr::MessageContent::Text("status".into()); MAX_QUEUED_STATUS_SENDS + 1];
        assert!(!worker.enqueue_batch(batch));
        drop(worker);
        assert!(rx.recv_timeout(Duration::from_secs(1)).is_err());
        assert!(sent.lock().unwrap().is_empty());
    }

    fn file(path: &str, kind: wr::FileKind) -> wr::MessageContent {
        wr::MessageContent::File(wr::FileContent {
            kind,
            path: path.into(),
            ..Default::default()
        })
    }
}
