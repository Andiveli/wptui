use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use whatsrust as wr;

use crate::app::FileMeta;
use crate::app::events::{AppEvent, AppInput};
use crate::app::media_download_port::MediaDownloadPort;

/// Owns the single long-lived worker used for all blocking media downloads.
///
/// Cancellation is checked every 50ms while idle and before a queued request.
/// It cannot interrupt an active Go download; shutdown therefore joins only after
/// that call returns, before the runtime can begin media cleanup.
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

impl Drop for Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Starts the single long-lived worker used for all media downloads.
pub fn spawn_with_port(
    media_path: PathBuf,
    app_tx: Sender<AppInput>,
    mut port: Box<dyn MediaDownloadPort>,
) -> Worker {
    let (tx, rx) = mpsc::channel();
    let (cancel_tx, cancel_rx) = mpsc::channel();
    let join = thread::spawn(move || {
        while let Some((message_id, file_id)) = next_request(&cancel_rx, &rx) {
            let state = match port.download(&file_id, &media_path) {
                Ok(()) => FileMeta::Downloaded,
                Err(_) => FileMeta::DownloadFailed,
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

        let request = match download_rx.recv_timeout(Duration::from_millis(50)) {
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
    use crate::app::media_download_port::{MediaDownloadError, MediaDownloadPort};
    use std::path::Path;
    use std::sync::{Arc, Condvar, Mutex};

    #[derive(Clone, Default)]
    struct FakePort {
        results: Arc<Mutex<Vec<Result<(), MediaDownloadError>>>>,
        calls: Arc<Mutex<Vec<wr::FileId>>>,
    }

    impl MediaDownloadPort for FakePort {
        fn download(&mut self, file_id: &wr::FileId, _: &Path) -> Result<(), MediaDownloadError> {
            self.calls.lock().unwrap().push(file_id.clone());
            self.results.lock().unwrap().remove(0)
        }
    }

    #[test]
    fn worker_maps_port_results_to_file_metadata() {
        let (app_tx, app_rx) = mpsc::channel();
        let port = FakePort {
            results: Arc::new(Mutex::new(vec![Ok(()), Err(MediaDownloadError)])),
            ..Default::default()
        };
        let mut worker = spawn_with_port(PathBuf::new(), app_tx, Box::new(port));

        worker
            .sender()
            .send(("one".into(), "file-one".into()))
            .unwrap();
        worker
            .sender()
            .send(("two".into(), "file-two".into()))
            .unwrap();

        assert!(matches!(
            app_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            AppInput::App(AppEvent::SetFileState(message_id, FileMeta::Downloaded)) if message_id.as_ref() == "one"
        ));
        assert!(matches!(
            app_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            AppInput::App(AppEvent::SetFileState(message_id, FileMeta::DownloadFailed)) if message_id.as_ref() == "two"
        ));
        worker.shutdown();
    }

    #[test]
    fn shutdown_joins_the_worker_thread() {
        let (app_tx, _app_rx) = mpsc::channel();
        let mut worker = spawn_with_port(PathBuf::new(), app_tx, Box::new(FakePort::default()));

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

    #[test]
    fn closed_channels_stop_the_worker() {
        let (download_tx, download_rx) = mpsc::channel::<()>();
        let (_cancel_tx, cancel_rx) = mpsc::channel::<()>();
        drop(download_tx);
        assert_eq!(next_request(&cancel_rx, &download_rx), None);

        let (_download_tx, download_rx) = mpsc::channel::<()>();
        let (cancel_tx, cancel_rx) = mpsc::channel::<()>();
        drop(cancel_tx);
        assert_eq!(next_request(&cancel_rx, &download_rx), None);
    }

    struct BlockingPort {
        started: Sender<()>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }

    impl MediaDownloadPort for BlockingPort {
        fn download(&mut self, _: &wr::FileId, _: &Path) -> Result<(), MediaDownloadError> {
            self.started.send(()).unwrap();
            let (lock, wake) = &*self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = wake.wait(released).unwrap();
            }
            Ok(())
        }
    }

    #[test]
    fn shutdown_waits_for_active_go_work_to_return() {
        let (app_tx, _app_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let mut worker = spawn_with_port(
            PathBuf::new(),
            app_tx,
            Box::new(BlockingPort {
                started: started_tx,
                release: Arc::clone(&release),
            }),
        );
        worker
            .sender()
            .send(("one".into(), "file-one".into()))
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let (finished_tx, finished_rx) = mpsc::channel();
        let shutdown = thread::spawn(move || {
            worker.shutdown();
            finished_tx.send(()).unwrap();
        });
        assert!(
            finished_rx
                .recv_timeout(Duration::from_millis(100))
                .is_err()
        );
        let (lock, wake) = &*release;
        *lock.lock().unwrap() = true;
        wake.notify_all();
        finished_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        shutdown.join().unwrap();
    }
}
