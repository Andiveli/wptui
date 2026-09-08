use std::path::Path;

use whatsrust as wr;

use crate::app::media_download_port::{MediaDownloadError, MediaDownloadPort};

/// The sole bridge from the Rust application to WhatsRust media downloads.
pub struct WhatsRustMediaDownload;

impl MediaDownloadPort for WhatsRustMediaDownload {
    fn download(
        &mut self,
        file_id: &wr::FileId,
        media_path: &Path,
    ) -> Result<(), MediaDownloadError> {
        wr::download_file(file_id, media_path).map_err(|_| MediaDownloadError)
    }
}
