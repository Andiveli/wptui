use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

use super::MediaRoot;
use crate::app::media_preview_port::MediaPreviewRead;

/// Filesystem adapter for bounded previews under the configured media root.
pub struct FilesystemMediaPreview {
    root: PathBuf,
}

impl FilesystemMediaPreview {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl MediaPreviewRead for FilesystemMediaPreview {
    fn read_preview(&self, relative_path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        let root =
            MediaRoot::new(&self.root).map_err(|error| io::Error::other(format!("{error:?}")))?;
        let path = root
            .media_file(relative_path)
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
        let mut file = File::open(path)?;
        if file.metadata()?.len() > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "preview exceeds byte limit",
            ));
        }
        let mut bytes = Vec::new();
        file.take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "preview exceeds byte limit",
            ));
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_missing_and_unsafe_preview_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tiny.webp"), b"abc").unwrap();
        let reader = FilesystemMediaPreview::new(dir.path().to_path_buf());
        assert_eq!(
            reader.read_preview(Path::new("tiny.webp"), 3).unwrap(),
            b"abc"
        );
        assert!(reader.read_preview(Path::new("tiny.webp"), 2).is_err());
        assert!(reader.read_preview(Path::new("missing.webp"), 8).is_err());
        assert!(reader.read_preview(Path::new("../tiny.webp"), 8).is_err());
        assert!(
            reader
                .read_preview(dir.path().join("tiny.webp").as_path(), 8)
                .is_err()
        );
    }
}
