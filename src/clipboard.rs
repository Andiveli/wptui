use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use arboard::Clipboard;
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use whatsrust::FileKind;

#[derive(Debug, PartialEq)]
pub enum ClipboardPaste {
    Text(String),
    Paths(Vec<PathBuf>),
    Png(Vec<u8>),
}

#[derive(Debug, PartialEq)]
pub enum ClipboardError {
    ClipboardUnavailable,
    EmptyText,
    MissingPath(PathBuf),
    InvalidImageData,
    ImageConversion,
    PersistFailed,
}

pub fn classify_text(text: &str) -> Result<ClipboardPaste, ClipboardError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(ClipboardError::EmptyText);
    }

    let paths = text.lines().map(PathBuf::from).collect::<Vec<_>>();
    if paths.iter().all(|path| path.is_file()) {
        return Ok(ClipboardPaste::Paths(paths));
    }

    if let Some(path) = paths
        .iter()
        .find(|path| path.is_absolute() && !path.exists())
    {
        return Err(ClipboardError::MissingPath(path.clone()));
    }

    Ok(ClipboardPaste::Text(text.to_owned()))
}

pub fn encode_rgba_png(
    width: usize,
    height: usize,
    rgba: &[u8],
) -> Result<Vec<u8>, ClipboardError> {
    let expected_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(ClipboardError::InvalidImageData)?;
    if rgba.len() != expected_len {
        return Err(ClipboardError::InvalidImageData);
    }

    let width = u32::try_from(width).map_err(|_| ClipboardError::InvalidImageData)?;
    let height = u32::try_from(height).map_err(|_| ClipboardError::InvalidImageData)?;
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|_| ClipboardError::ImageConversion)?;
    Ok(png)
}

pub fn read_paste(clipboard: &mut Clipboard) -> Result<ClipboardPaste, ClipboardError> {
    match clipboard.get_text() {
        Ok(text) => classify_text(&text),
        Err(_) => {
            let image = clipboard
                .get_image()
                .map_err(|_| ClipboardError::ClipboardUnavailable)?;
            encode_rgba_png(image.width, image.height, image.bytes.as_ref())
                .map(ClipboardPaste::Png)
        }
    }
}

pub fn persist_png(media_path: &Path, png: &[u8]) -> Result<PathBuf, ClipboardError> {
    fs::create_dir_all(media_path).map_err(|_| ClipboardError::PersistFailed)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ClipboardError::PersistFailed)?
        .as_nanos();
    let path = media_path.join(format!("clipboard-{timestamp}.png"));
    fs::write(&path, png).map_err(|_| ClipboardError::PersistFailed)?;
    Ok(path)
}

pub fn file_kind(path: &Path) -> FileKind {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("png" | "jpg" | "jpeg" | "gif" | "webp") => FileKind::Image,
        Some("mp4" | "mkv" | "mov") => FileKind::Video,
        Some("mp3" | "wav" | "ogg") => FileKind::Audio,
        _ => FileKind::Document,
    }
}
