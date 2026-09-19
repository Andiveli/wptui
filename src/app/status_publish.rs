use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;

use whatsrust as wr;

use crate::app::events::{AppEvent, AppInput};

const MAX_QUEUED_STATUS_SENDS: usize = 64;

enum Command {
    Send(wr::MessageContent),
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
}

fn run(rx: Receiver<Command>, app_tx: mpsc::Sender<AppInput>, mut port: Box<dyn StatusSendPort>) {
    while let Ok(Command::Send(content)) = rx.recv() {
        let event = match port.send(&content) {
            wr::StatusSendResult::Sent => AppEvent::StatusSendSucceeded,
            result => AppEvent::StatusSendFailed(result),
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

    fn file(path: &str, kind: wr::FileKind) -> wr::MessageContent {
        wr::MessageContent::File(wr::FileContent {
            kind,
            path: path.into(),
            ..Default::default()
        })
    }
}
