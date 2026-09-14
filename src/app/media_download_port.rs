use std::path::Path;

use whatsrust as wr;

/// App-level failure for a media download bridge request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaDownloadError;

/// Worker-bound bridge for the blocking WhatsRust media download call.
pub trait MediaDownloadPort: Send + 'static {
    fn download(
        &mut self,
        file_id: &wr::FileId,
        media_path: &Path,
    ) -> Result<(), MediaDownloadError>;
}
