use std::{io, path::Path};

/// Bounded source bytes for a media preview; decoding remains worker-owned.
pub trait MediaPreviewRead: Send + Sync {
    fn read_preview(&self, relative_path: &Path, max_bytes: u64) -> io::Result<Vec<u8>>;
}
