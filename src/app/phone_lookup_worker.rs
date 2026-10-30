use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender, SyncSender};
use std::thread::{self, JoinHandle};

use super::events::{AppEvent, AppInput};
use super::phone_lookup_port::PhoneLookupPort;

struct Request {
    generation: u64,
    phone: String,
}

/// A bounded queue keeps the network lookup off the input/draw loop.
pub struct Worker {
    tx: Option<SyncSender<Request>>,
    join: Option<JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
}

impl Worker {
    pub fn new(app_tx: Sender<AppInput>, mut port: Box<dyn PhoneLookupPort>) -> Self {
        let (tx, rx) = mpsc::sync_channel::<Request>(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let join = thread::spawn(move || {
            while let Ok(request) = rx.recv() {
                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }
                let result = port.lookup(&request.phone);
                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }
                if app_tx
                    .send(AppInput::App(AppEvent::PhoneLookupFinished {
                        generation: request.generation,
                        phone: request.phone,
                        result,
                    }))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            tx: Some(tx),
            join: Some(join),
            cancelled,
        }
    }

    pub fn enqueue(&self, generation: u64, phone: String) -> bool {
        self.tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(Request { generation, phone }).is_ok())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.tx.take();
        // The Go call has its own deadline; never block UI shutdown on it.
        self.join.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use whatsrust::{JID, PhoneLookupError};

    struct Registered;

    impl PhoneLookupPort for Registered {
        fn lookup(&mut self, phone: &str) -> Result<Option<JID>, PhoneLookupError> {
            assert_eq!(phone, "+15551234567");
            Ok(Some(JID::from("15551234567@s.whatsapp.net".to_owned())))
        }
    }

    #[test]
    fn worker_returns_tagged_lookup_without_blocking_the_caller() {
        let (tx, rx) = mpsc::channel();
        let worker = Worker::new(tx, Box::new(Registered));
        assert!(worker.enqueue(7, "+15551234567".into()));
        let AppInput::App(AppEvent::PhoneLookupFinished {
            generation,
            phone,
            result,
        }) = rx.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("unexpected worker event");
        };
        assert_eq!(generation, 7);
        assert_eq!(phone, "+15551234567");
        assert_eq!(
            result,
            Ok(Some(JID::from("15551234567@s.whatsapp.net".to_owned())))
        );
    }
}
