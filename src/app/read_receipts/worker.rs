use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::repository_port::{PendingReceiptRepository, RepositoryError};
use super::{
    PersistResult, ReadReceiptDispatcher, ReadReceiptPort, ReceiptCandidate, ReceiptKey,
    ReceiptSendStatus,
};
use crate::app::events::{AppEvent, AppInput};

enum Command {
    Send(ReceiptCandidate),
    Load,
    Persist(ReceiptCandidate),
    Complete(ReceiptKey),
    Reject(ReceiptKey),
    Shutdown(mpsc::Sender<()>),
}

pub struct Worker {
    tx: SyncSender<Command>,
    join: Option<JoinHandle<()>>,
    enabled: Arc<AtomicBool>,
}

impl Worker {
    pub fn new(
        app_tx: mpsc::Sender<AppInput>,
        port: Box<dyn ReadReceiptPort + Send>,
        repository: Box<dyn PendingReceiptRepository + Send>,
    ) -> Self {
        let (tx, rx) = mpsc::sync_channel::<Command>(64);
        let enabled = Arc::new(AtomicBool::new(true));
        let worker_enabled = Arc::clone(&enabled);
        let join = thread::spawn(move || run(rx, app_tx, port, repository, worker_enabled));
        let worker = Self {
            tx,
            join: Some(join),
            enabled,
        };
        worker
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }
    pub fn load(&self) -> bool {
        self.tx.try_send(Command::Load).is_ok()
    }
    pub fn persist(&self, candidate: ReceiptCandidate) -> bool {
        self.tx.try_send(Command::Persist(candidate)).is_ok()
    }
    pub fn complete(&self, key: ReceiptKey) -> bool {
        self.tx.try_send(Command::Complete(key)).is_ok()
    }
    pub fn reject(&self, key: ReceiptKey) -> bool {
        self.tx.try_send(Command::Reject(key)).is_ok()
    }

    pub fn shutdown(&mut self) {
        if self.join.is_none() {
            return;
        }
        let (ack, acknowledged) = mpsc::channel();
        let _ = self.tx.send(Command::Shutdown(ack));
        let _ = acknowledged.recv_timeout(Duration::from_secs(7));
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run(
    rx: Receiver<Command>,
    app_tx: mpsc::Sender<AppInput>,
    port: Box<dyn ReadReceiptPort + Send>,
    repository: Box<dyn PendingReceiptRepository + Send>,
    enabled: Arc<AtomicBool>,
) {
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Command::Send(candidate)) => {
                let key: ReceiptKey = candidate.key();
                let status = if enabled.load(Ordering::Acquire) {
                    port.send(&candidate)
                } else {
                    ReceiptSendStatus::Disconnected
                };
                if app_tx
                    .send(AppInput::App(AppEvent::ReadReceiptResult(key, status)))
                    .is_err()
                {
                    break;
                }
            }
            Ok(Command::Load) => {
                if enabled.load(Ordering::Acquire) {
                    let _ = app_tx.send(AppInput::App(AppEvent::ReadReceiptRestored(
                        repository.load(),
                    )));
                }
            }
            Ok(Command::Persist(candidate)) => {
                let result = if enabled.load(Ordering::Acquire) {
                    match repository.was_sent(&candidate.key()) {
                        Ok(true) => PersistResult::AlreadySent,
                        Ok(false) => match repository.save(&candidate) {
                            Ok(()) => PersistResult::Saved,
                            Err(error) => PersistResult::Failed(error),
                        },
                        Err(error) => PersistResult::Failed(error),
                    }
                } else {
                    PersistResult::Failed(RepositoryError::Unavailable)
                };
                let _ = app_tx.send(AppInput::App(AppEvent::ReadReceiptPersisted(
                    candidate, result,
                )));
            }
            Ok(Command::Complete(key)) => {
                let result = if enabled.load(Ordering::Acquire) {
                    repository.complete_success(&key)
                } else {
                    Err(RepositoryError::Unavailable)
                };
                let _ = app_tx.send(AppInput::App(AppEvent::ReadReceiptCompleted(key, result)));
            }
            Ok(Command::Reject(key)) => {
                let result = if enabled.load(Ordering::Acquire) {
                    repository.reject(&key)
                } else {
                    Err(RepositoryError::Unavailable)
                };
                let _ = app_tx.send(AppInput::App(AppEvent::ReadReceiptRejected(key, result)));
            }
            Ok(Command::Shutdown(ack)) => {
                let _ = ack.send(());
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

impl ReadReceiptDispatcher for Worker {
    fn dispatch(&self, candidate: ReceiptCandidate) -> bool {
        self.tx.try_send(Command::Send(candidate)).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Repo;
    impl PendingReceiptRepository for Repo {
        fn load(&self) -> Result<Vec<ReceiptCandidate>, RepositoryError> {
            Ok(Vec::new())
        }
        fn save(&self, _: &ReceiptCandidate) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn was_sent(&self, _: &ReceiptKey) -> Result<bool, RepositoryError> {
            Ok(false)
        }
        fn complete_success(&self, _: &ReceiptKey) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn reject(&self, _: &ReceiptKey) -> Result<(), RepositoryError> {
            Ok(())
        }
    }
    struct Port;
    impl ReadReceiptPort for Port {
        fn send(&self, _: &ReceiptCandidate) -> ReceiptSendStatus {
            ReceiptSendStatus::Success
        }
    }
    #[test]
    fn shutdown_acknowledges_and_joins_worker() {
        let (tx, _rx) = mpsc::channel();
        let mut worker = Worker::new(tx, Box::new(Port), Box::new(Repo));
        worker.shutdown();
        assert!(worker.join.is_none());
    }
}
