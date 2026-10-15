use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// Owns media work that may outlive an individual viewer interaction.
///
/// Jobs receive a permit rather than the application sender directly. Shutdown
/// revokes every permit before joining the workers, so a completed filesystem
/// operation cannot enqueue a result into a terminal or logged-out runtime.
pub(crate) struct MediaJobOwner {
    active: Arc<Mutex<bool>>,
    handles: Vec<JoinHandle<()>>,
}

#[derive(Clone)]
pub(crate) struct MediaJobPermit(Arc<Mutex<bool>>);

impl MediaJobOwner {
    pub(crate) fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(true)),
            handles: Vec::new(),
        }
    }

    pub(crate) fn permit(&self) -> MediaJobPermit {
        MediaJobPermit(Arc::clone(&self.active))
    }

    pub(crate) fn spawn(&mut self, job: impl FnOnce(MediaJobPermit) + Send + 'static) {
        let permit = self.permit();
        self.handles.push(thread::spawn(move || job(permit)));
    }

    pub(crate) fn shutdown(&mut self) {
        self.cancel();
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }

    pub(crate) fn cancel(&self) {
        *self.active.lock().unwrap() = false;
    }
}

impl MediaJobPermit {
    pub(crate) fn send<T>(&self, tx: &Sender<T>, value: T) {
        self.send_if_active(tx, value, || {});
    }

    fn send_if_active<T>(&self, tx: &Sender<T>, value: T, before_send: impl FnOnce()) {
        // The standard channel is unbounded, so send does not wait for receiver capacity.
        // Holding this guard through send serializes publication with cancellation: once
        // cancellation returns, no worker can enqueue another event.
        let active = self.0.lock().unwrap();
        if *active {
            before_send();
            let _ = tx.send(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::Duration;

    use super::MediaJobOwner;

    #[test]
    fn cancelled_jobs_cannot_publish_results() {
        let mut owner = MediaJobOwner::new();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();

        owner.spawn(move |permit| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            permit.send(&result_tx, "stale result");
        });
        started_rx.recv().unwrap();

        owner.cancel();
        release_tx.send(()).unwrap();
        owner.shutdown();
        owner.shutdown();

        assert!(result_rx.try_recv().is_err());
    }

    #[test]
    fn cancellation_waits_for_an_in_progress_publication() {
        let owner = Arc::new(MediaJobOwner::new());
        let permit = owner.permit();
        let (publication_ready_tx, publication_ready_rx) = mpsc::channel();
        let (release_publication_tx, release_publication_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let sender = thread::spawn(move || {
            permit.send_if_active(&result_tx, "result", || {
                publication_ready_tx.send(()).unwrap();
                release_publication_rx.recv().unwrap();
            });
        });

        publication_ready_rx.recv().unwrap();
        let cancel_owner = Arc::clone(&owner);
        let (cancelled_tx, cancelled_rx) = mpsc::channel();
        let canceller = thread::spawn(move || {
            cancel_owner.cancel();
            cancelled_tx.send(()).unwrap();
        });

        assert!(
            cancelled_rx
                .recv_timeout(Duration::from_millis(50))
                .is_err()
        );
        release_publication_tx.send(()).unwrap();
        sender.join().unwrap();
        cancelled_rx.recv().unwrap();
        canceller.join().unwrap();

        assert_eq!(result_rx.recv().unwrap(), "result");
    }
}
