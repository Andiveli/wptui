//! Bounded, still-only previews for the current composer's pending media.
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::Arc;

use ratatui::layout::Size;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, ResizeEncodeRender};
use whatsrust as wr;

use crate::app::App;
use crate::app::actions::Section;
use crate::app::composer::PendingAttachment;
use crate::app::events::{AppEvent, AppInput};
use crate::app::media_jobs::MediaJobOwner;
use crate::media::MediaRoot;

pub(crate) const MAX_PREVIEWS: usize = 3;
pub(crate) const PREVIEW_WIDTH: u16 = 16;
pub(crate) const PREVIEW_HEIGHT: u16 = 4;

pub(crate) struct PreviewRequest {
    generation: u64,
    attachment: PendingAttachment,
}

pub(crate) struct PreviewEntry {
    generation: u64,
    attachment: PendingAttachment,
    pub(crate) protocol: Option<StatefulProtocol>,
    completed: bool,
}

#[derive(Default)]
pub(crate) struct ComposerPreviews {
    pub(crate) entries: Vec<PreviewEntry>,
    context: Option<(Section, Option<wr::JID>)>,
    generation: u64,
    // Keep the batch occupied even when its entries are invalidated. Rapid
    // remove/requeue cannot start an unbounded number of decoder threads.
    in_flight: bool,
}

fn same_attachment(left: &PendingAttachment, right: &PendingAttachment) -> bool {
    Arc::ptr_eq(&left.preview_identity, &right.preview_identity)
        && left.path == right.path
        && std::mem::discriminant(&left.kind) == std::mem::discriminant(&right.kind)
}

pub(crate) fn eligible(attachment: &PendingAttachment) -> bool {
    matches!(attachment.kind, wr::FileKind::Image | wr::FileKind::Video)
}

/// Thumbnail rows are optional; labels and at least one input row take priority.
pub(crate) fn preview_rows(pending: &[PendingAttachment], width: u16, height: u16) -> usize {
    if width >= PREVIEW_WIDTH && height >= 12 && pending.iter().any(eligible) {
        usize::from(PREVIEW_HEIGHT)
    } else {
        0
    }
}

impl ComposerPreviews {
    pub(crate) fn protocol_mut(
        &mut self,
        attachment: &PendingAttachment,
    ) -> Option<&mut StatefulProtocol> {
        self.entries
            .iter_mut()
            .find(|entry| same_attachment(&entry.attachment, attachment))
            .and_then(|entry| entry.protocol.as_mut())
    }
}

impl App<'_> {
    pub(crate) fn reconcile_composer_previews(&mut self) -> Vec<PreviewRequest> {
        let context = (self.selected_section, self.open_chat());
        let previews = &mut self.composer_previews;
        if previews.context.as_ref() != Some(&context) {
            previews.entries.clear();
            previews.context = Some(context);
        }
        let visible = self
            .composer
            .pending
            .iter()
            .filter(|item| eligible(item))
            .take(MAX_PREVIEWS)
            .collect::<Vec<_>>();
        previews.entries.retain(|entry| {
            visible
                .iter()
                .any(|item| same_attachment(&entry.attachment, item))
        });
        for attachment in visible {
            if !previews
                .entries
                .iter()
                .any(|entry| same_attachment(&entry.attachment, attachment))
            {
                previews.generation += 1;
                previews.entries.push(PreviewEntry {
                    generation: previews.generation,
                    attachment: attachment.clone(),
                    protocol: None,
                    completed: false,
                });
            }
        }
        if previews.in_flight {
            return Vec::new();
        }
        let requests = previews
            .entries
            .iter()
            .filter(|entry| !entry.completed)
            .map(|entry| PreviewRequest {
                generation: entry.generation,
                attachment: entry.attachment.clone(),
            })
            .collect::<Vec<_>>();
        previews.in_flight = !requests.is_empty();
        requests
    }

    pub(crate) fn finish_composer_previews(
        &mut self,
        results: Vec<(u64, Option<StatefulProtocol>)>,
    ) -> bool {
        // Reconcile before accepting results, not just before the next draw:
        // input may have removed/sent/replaced the attachment or switched chats.
        self.reconcile_composer_previews();
        self.composer_previews.in_flight = false;
        let mut changed = false;
        for (generation, protocol) in results {
            if let Some(entry) = self
                .composer_previews
                .entries
                .iter_mut()
                .find(|entry| entry.generation == generation)
            {
                entry.protocol = protocol;
                entry.completed = true;
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn schedule_composer_previews(&mut self, jobs: &mut MediaJobOwner) {
        let requests = self.reconcile_composer_previews();
        if requests.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        let media_path = self.media_path.clone();
        let picker = Arc::clone(&self.picker);
        jobs.spawn(move |permit| {
            let results = requests
                .into_iter()
                .map(|request| {
                    let protocol = load_preview(&media_path, &request.attachment).map(|image| {
                        let mut protocol = picker.lock().unwrap().new_resize_protocol(image);
                        protocol.resize_encode(
                            &Resize::Scale(None),
                            Size::new(PREVIEW_WIDTH, PREVIEW_HEIGHT),
                        );
                        protocol
                    });
                    (request.generation, protocol)
                })
                .collect();
            permit.send(&tx, AppInput::App(AppEvent::SetComposerPreviews(results)));
        });
    }
}

fn load_preview(media_path: &Path, attachment: &PendingAttachment) -> Option<image::DynamicImage> {
    let source = Path::new(attachment.path.as_ref());
    // Only pending, user-selected local paths get their own root. Received
    // media continues to use the app's existing confinement rules unchanged.
    let (root, relative) = if source.is_absolute() {
        (
            MediaRoot::new(source.parent()?).ok()?,
            Path::new(source.file_name()?),
        )
    } else {
        (MediaRoot::new(media_path).ok()?, source)
    };
    if matches!(attachment.kind, wr::FileKind::Video) {
        return crate::app::media_support::generate_video_thumbnail(&root, relative);
    }
    if !matches!(attachment.kind, wr::FileKind::Image) {
        return None;
    }
    const MAX_BYTES: u64 = 16 * 1024 * 1024;
    let file = root.open_media_file(relative).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().ok().map(|image| image.thumbnail(512, 256))
}

#[cfg(test)]
#[path = "composer_preview_tests.rs"]
mod tests;
