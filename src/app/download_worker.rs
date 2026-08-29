use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use whatsrust as wr;

use crate::app::FileMeta;
use crate::app::events::{AppEvent, AppInput};

/// Owns the single long-lived worker used for all CGo media downloads.
///
/// The worker serializes CGo calls while keeping cancellation and joining under
/// the runtime's control, so shutdown completes before media cleanup begins.
pub struct Worker {
    tx: Sender<(wr::MessageId, wr::FileId)>,
    cancel_tx: Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn sender(&self) -> Sender<(wr::MessageId, wr::FileId)> {
        self.tx.clone()
    }

    pub fn shutdown(&mut self) {
        if self.join.is_none() {
            return;
        }
        let _ = self.cancel_tx.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Starts the single long-lived worker used for all CGo media downloads.
pub fn spawn(media_path: PathBuf, app_tx: Sender<AppInput>) -> Worker {
    let (tx, rx) = mpsc::channel();
    let (cancel_tx, cancel_rx) = mpsc::channel();
    let join = thread::spawn(move || {
        while let Some((message_id, file_id)) = next_request(&cancel_rx, &rx) {
            let result = wr::download_file(&file_id, &media_path);
            let state = if result.is_err() {
                FileMeta::DownloadFailed
            } else {
                FileMeta::Downloaded
            };
            if app_tx
                .send(AppInput::App(AppEvent::SetFileState(message_id, state)))
                .is_err()
            {
                break;
            }
        }
    });
    Worker {
        tx,
        cancel_tx,
        join: Some(join),
    }
}

fn next_request<T>(cancel_rx: &mpsc::Receiver<()>, download_rx: &mpsc::Receiver<T>) -> Option<T> {
    loop {
        if matches!(
            cancel_rx.try_recv(),
            Ok(()) | Err(mpsc::TryRecvError::Disconnected)
        ) {
            return None;
        }

        let request = match download_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(request) => request,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        };

        if matches!(
            cancel_rx.try_recv(),
            Ok(()) | Err(mpsc::TryRecvError::Disconnected)
        ) {
            return None;
        }
        return Some(request);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_joins_the_worker_thread() {
        let (app_tx, _app_rx) = mpsc::channel();
        let mut worker = spawn(PathBuf::new(), app_tx);

        worker.shutdown();

        assert!(worker.join.is_none());
    }

    #[test]
    fn cancellation_wins_over_an_already_queued_download() {
        let (download_tx, download_rx) = mpsc::channel();
        let (cancel_tx, cancel_rx) = mpsc::channel();
        download_tx.send(()).unwrap();
        cancel_tx.send(()).unwrap();

        assert_eq!(next_request(&cancel_rx, &download_rx), None);
    }
}
