use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use super::events::{AppEvent, AppInput};
use super::phone_lookup_port::PhoneLookupPort;

struct Request {
    generation: u64,
    phone: String,
}

/// One in-flight lookup and one replaceable pending request. A stale queued
/// number never blocks the latest user-entered number from being accepted.
pub struct Worker {
    tx: Option<SyncSender<()>>,
    pending: Arc<Mutex<Option<Request>>>,
    join: Option<JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
}

impl Worker {
    pub fn new(app_tx: Sender<AppInput>, mut port: Box<dyn PhoneLookupPort>) -> Self {
        let (tx, rx) = mpsc::sync_channel::<()>(1);
        let pending = Arc::new(Mutex::new(None::<Request>));
        let worker_pending = Arc::clone(&pending);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let join = thread::spawn(move || {
            while rx.recv().is_ok() {
                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }
                let Some(request) = worker_pending.lock().unwrap().take() else {
                    continue;
                };
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
            pending,
            join: Some(join),
            cancelled,
        }
    }

    pub fn enqueue(&self, generation: u64, phone: String) -> bool {
        let Some(tx) = self.tx.as_ref() else {
            return false;
        };
        if self.cancelled.load(Ordering::Acquire) {
            return false;
        }
        let mut pending = self.pending.lock().unwrap();
        *pending = Some(Request { generation, phone });
        match tx.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => true,
            Err(TrySendError::Disconnected(())) => {
                pending.take();
                false
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.pending.lock().unwrap().take();
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

    struct HeldLookup {
        started: Sender<()>,
        release: mpsc::Receiver<()>,
    }

    impl PhoneLookupPort for HeldLookup {
        fn lookup(&mut self, phone: &str) -> Result<Option<JID>, PhoneLookupError> {
            if phone == "+1111111" {
                self.started.send(()).unwrap();
                self.release.recv().unwrap();
            }
            Ok(None)
        }
    }

    #[test]
    fn latest_request_replaces_stale_queued_number() {
        let (app_tx, app_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = Worker::new(
            app_tx,
            Box::new(HeldLookup {
                started: started_tx,
                release: release_rx,
            }),
        );
        assert!(worker.enqueue(1, "+1111111".into()));
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(worker.enqueue(2, "+2222222".into()));
        assert!(worker.enqueue(3, "+3333333".into()));
        release_tx.send(()).unwrap();
        for expected in [1, 3] {
            let AppInput::App(AppEvent::PhoneLookupFinished { generation, .. }) =
                app_rx.recv_timeout(Duration::from_secs(2)).unwrap()
            else {
                panic!("unexpected worker event");
            };
            assert_eq!(generation, expected);
        }
        assert!(app_rx.recv_timeout(Duration::from_millis(50)).is_err());
    }
}
