use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread;

use whatsrust as wr;

use crate::app::FileMeta;
use crate::app::events::{AppEvent, AppInput};

/// Starts the single long-lived worker used for all CGo media downloads.
///
/// Keeping one receiver-owned thread preserves the CGo serialization constraint,
/// while returning the sender lets `App::run` retain ownership of request
/// submission and channel shutdown semantics.
pub fn spawn(media_path: PathBuf, app_tx: Sender<AppInput>) -> Sender<(wr::MessageId, wr::FileId)> {
    let (download_tx, download_rx) = mpsc::channel::<(wr::MessageId, wr::FileId)>();
    thread::spawn(move || {
        for (message_id, file_id) in download_rx {
            let result = wr::download_file(&file_id, &media_path);
            let state = if result.is_err() {
                FileMeta::DownloadFailed
            } else {
                FileMeta::Downloaded
            };
            app_tx
                .send(AppInput::App(AppEvent::SetFileState(message_id, state)))
                .unwrap();
        }
    });
    download_tx
}
