use std::{
    path::Path,
    sync::{Arc, Mutex, mpsc::Sender},
};

use ratatui::layout::Size;
use ratatui_image::picker::Picker;
use whatsrust as wr;

use crate::app::{
    events::{AppEvent, AppInput},
    media_jobs::MediaJobPermit,
    media_preview_port::MediaPreviewRead,
    sticker_animation::{AnimationSlot, StickerPlayback, decode_animated_webp},
};
use crate::ui::message_list::{IMAGE_HEIGHT, IMAGE_WIDTH};

/// Returns false when still-image fallback should handle this file.
pub(crate) fn try_animated_preview(
    reader: &dyn MediaPreviewRead,
    path: &Path,
    message_id: wr::MessageId,
    file_path: Arc<str>,
    slot: AnimationSlot,
    picker: &Arc<Mutex<Picker>>,
    permit: &MediaJobPermit,
    tx: &Sender<AppInput>,
) -> Result<(), AnimationSlot> {
    let bytes = reader.read_preview(path, 8 * 1024 * 1024).ok();
    if let Some(animation) = bytes
        .as_deref()
        .and_then(|bytes| decode_animated_webp(bytes).ok().flatten())
        .filter(|animation| animation.frames.len() > 1)
    {
        let mut playback = StickerPlayback::prepare(
            animation,
            &mut picker.lock().unwrap(),
            Size::new(IMAGE_WIDTH as u16, IMAGE_HEIGHT as u16),
        );
        playback.slot = Some(slot);
        permit.send(
            tx,
            AppInput::App(AppEvent::SetStickerPreview(message_id, file_path, playback)),
        );
        Ok(())
    } else {
        Err(slot)
    }
}
