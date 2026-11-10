use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use ratatui::layout::Size;
use ratatui_image::{Resize, ResizeEncodeRender};
use whatsrust as wr;

use crate::app::events::{AppEvent, AppInput, ViewerPreviewState, ViewerStatus};
use crate::app::media_jobs::MediaJobOwner;
use crate::app::media_support::{
    apply_video_play_marker, generate_video_thumbnail, has_decent_video_thumbnail,
};
use crate::app::{App, FileMeta, Metadata};
use crate::media::MediaRoot;
use crate::ui::message_list::{IMAGE_HEIGHT, IMAGE_WIDTH, VIDEO_HEIGHT, VIDEO_WIDTH};

type DownloadSender = Sender<(wr::MessageId, wr::FileId)>;

impl App<'_> {
    /// Metadata/identity validation is separate from cache visibility. Only the
    /// successful draw plan supplies visibility and exact preparation geometry.
    pub(crate) fn native_thumbnail_is_current(
        &self,
        key: &crate::app::native_link_thumbnails::ThumbnailKey,
    ) -> bool {
        if self.open_chat.as_ref() != Some(&key.chat)
            || !matches!(
                self.selected_section,
                crate::app::actions::Section::Chats | crate::app::actions::Section::Communities
            )
            || self.rail_on_logout
            || self.logout_in_progress
            || self.should_quit
            || !crate::app::native_link_thumbnails::valid_size(key.size)
        {
            return false;
        }
        let Some(message) = self.messages.get(&key.message_id) else {
            return false;
        };
        if message.info.chat != key.chat
            || !matches!(&message.message, wr::MessageContent::Text(source) if source == &key.source)
        {
            return false;
        }
        self.native_link_preview(&key.message_id)
            .is_some_and(|(_, preview)| {
                preview.matched_text == key.matched_url
                    && !preview.jpeg_thumbnail.is_empty()
                    && preview.jpeg_thumbnail.len() <= 256 * 1024
            })
    }

    fn retry_pending_sticker_previews(
        &mut self,
        download_tx: &DownloadSender,
        media_jobs: &mut MediaJobOwner,
    ) {
        while let Some((id, path)) = self.pending_sticker_previews.pop_front() {
            if !self.preview_result_is_current(&id, &path) {
                continue;
            }
            // Only dispatch when a slot can be acquired. UI dispatch is single-threaded;
            // release this probe before LoadFilePreview claims the slot.
            let Some(slot) = self.animation_slots.try_acquire() else {
                self.pending_sticker_previews.push_front((id, path));
                break;
            };
            drop(slot);
            self.metadata
                .insert(id.clone(), Metadata::File(FileMeta::Downloaded));
            self.handle_media_viewer_event(AppEvent::LoadFilePreview(id), download_tx, media_jobs);
        }
    }

    pub(crate) fn handle_media_viewer_event(
        &mut self,
        event: AppEvent,
        download_tx: &DownloadSender,
        media_jobs: &mut MediaJobOwner,
    ) -> bool {
        match event {
            AppEvent::LoadNativeLinkThumbnail(request) => {
                let key = request.0;
                if !self.native_thumbnail_is_current(&key) {
                    return false;
                }
                // Temporarily own the cache so the admission supplier can use the
                // validated accessor without cloning bytes on hits or full slots.
                let mut cache = std::mem::take(&mut self.native_link_thumbnails);
                let job = cache.admit(&key, || {
                    self.native_link_preview(&key.message_id)
                        .map(|(_, preview)| preview.jpeg_thumbnail.clone())
                });
                self.native_link_thumbnails = cache;
                let Some(job) = job else {
                    return false;
                };
                let tx = self.tx.clone();
                let picker = Arc::clone(&self.picker);
                media_jobs.spawn(move |permit| {
                    let ticket = job.ticket();
                    // A panicking decoder/encoder still returns this slot to the event owner.
                    let protocol = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        job.run_with(
                            crate::app::native_link_thumbnails::decode_thumbnail,
                            |image, size| {
                                crate::app::native_link_thumbnails::prepare_image(
                                    image, size, &picker,
                                )
                            },
                        )
                    }))
                    .unwrap_or_else(|_| {
                        log::error!("Native link thumbnail preparation panicked");
                        None
                    });
                    // Cancelled jobs publish an empty completion too; only shutdown
                    // suppresses publication through the existing managed permit.
                    permit.send(
                        &tx,
                        AppInput::App(AppEvent::SetNativeLinkThumbnail(
                            crate::app::events::NativeLinkThumbnailResult { ticket, protocol },
                        )),
                    );
                });
                false
            }
            AppEvent::SetNativeLinkThumbnail(result) => {
                let current = self
                    .native_link_thumbnails
                    .key_for_ticket(&result.ticket)
                    .is_some_and(|key| self.native_thumbnail_is_current(key));
                if !current {
                    self.native_link_thumbnails.cancel(&result.ticket);
                }
                let outstanding = self.native_link_thumbnails.outstanding();
                let accepted = self
                    .native_link_thumbnails
                    .finish(result.ticket, result.protocol);
                // A stale completion can unblock demand without installing a protocol.
                accepted
                    || (self.native_link_thumbnails.outstanding() < outstanding
                        && self
                            .native_link_thumbnails
                            .pending_visible_keys()
                            .any(|key| self.native_thumbnail_is_current(key)))
            }
            AppEvent::SetComposerPreviews(results) => self.finish_composer_previews(results),
            AppEvent::OutboundSendSucceeded { .. } | AppEvent::OutboundSendFailed { .. } => {
                unreachable!("runtime_loop must route Send events to handle_send_event")
            }
            AppEvent::StatusSendSucceeded
            | AppEvent::StatusSendFailed(_)
            | AppEvent::StatusBatchFinished { .. } => {
                unreachable!("runtime_loop must route StatusSend events to handle_send_event")
            }
            AppEvent::ReadReceiptResult(..)
            | AppEvent::ReadReceiptRestored(..)
            | AppEvent::ReadReceiptPersisted(..)
            | AppEvent::ReadReceiptCompleted(..)
            | AppEvent::ReadReceiptRejected(..) => {
                unreachable!(
                    "runtime_loop must route ReadReceipt events to handle_read_receipt_event"
                )
            }
            AppEvent::SetStackCrops(results) => {
                self.stack_previews.finish(results);
                true
            }
            AppEvent::SetFilePreview(message_id, file_path, img, source) => {
                if !self.preview_result_is_current(&message_id, &file_path) {
                    self.retry_pending_sticker_previews(download_tx, media_jobs);
                    return false;
                }
                self.cache_file_preview(message_id.clone(), file_path.clone(), img);
                if let Some((source, dimensions)) = source {
                    self.stack_previews
                        .install_original(file_path, source, dimensions);
                }
                self.retry_pending_sticker_previews(download_tx, media_jobs);
                if let Some(viewer) = self.attachment_viewer.as_mut()
                    && viewer.message_id == message_id
                {
                    viewer.status = ViewerStatus::Ready;
                }
                true
            }
            AppEvent::SetStickerPreview(message_id, path, playback) => {
                if !self.preview_result_is_current(&message_id, &path) {
                    drop(playback);
                    self.retry_pending_sticker_previews(download_tx, media_jobs);
                    return false;
                }
                self.cache_sticker_preview(message_id, path, playback);
                self.retry_pending_sticker_previews(download_tx, media_jobs);
                true
            }
            AppEvent::LoadViewerPreview(key) => {
                if self
                    .viewer_preview
                    .as_ref()
                    .is_none_or(|state| state.key() != &key)
                {
                    false
                } else {
                    let tx = self.tx.clone();
                    let media_path = self.media_path.clone();
                    let picker = Arc::clone(&self.picker);
                    media_jobs.spawn(move |permit| {
                        let protocol = MediaRoot::new(&media_path)
                            .and_then(|root| {
                                root.open_media_file(Path::new(key.preview_path().as_ref()))
                            })
                            .ok()
                            .and_then(decode_media_image)
                            .map(|image| {
                                let mut protocol =
                                    picker.lock().unwrap().new_resize_protocol(image);
                                protocol.resize_encode(
                                    &Resize::Scale(None),
                                    Size::new(key.width, key.height),
                                );
                                protocol
                            });
                        permit.send(
                            &tx,
                            AppInput::App(AppEvent::SetViewerPreview(key, protocol)),
                        );
                    });
                    false
                }
            }
            AppEvent::SetViewerPreview(key, protocol) => {
                if self
                    .viewer_preview
                    .as_ref()
                    .is_some_and(|state| state.key() == &key)
                {
                    self.viewer_preview = Some(match protocol {
                        Some(protocol) => ViewerPreviewState::Ready {
                            key,
                            protocol: Box::new(protocol),
                        },
                        None => ViewerPreviewState::Failed(key),
                    });
                    true
                } else {
                    false
                }
            }
            AppEvent::LoadFilePreview(message_id) => {
                if matches!(
                    self.metadata.get(&message_id),
                    Some(Metadata::File(FileMeta::Loaded))
                ) {
                    // UI queued geometry only. Crop and encode on the existing
                    // managed worker, with bounded jobs/keys and no extra I/O.
                    if let Some(work) = self.stack_previews.take_work() {
                        let picker = Arc::clone(&self.picker);
                        let tx = self.tx.clone();
                        media_jobs.spawn(move |permit| {
                            let results = work.prepare(&mut picker.lock().unwrap());
                            permit.send(&tx, AppInput::App(AppEvent::SetStackCrops(results)));
                        });
                    }
                    return false;
                }
                if !matches!(
                    self.metadata.get(&message_id),
                    Some(Metadata::File(FileMeta::Loading))
                ) {
                    self.metadata
                        .insert(message_id.clone(), Metadata::File(FileMeta::Loading));
                    self.message_height_cache.invalidate(&message_id);
                    let tx = self.tx.clone();
                    let media_path = self.media_path.to_owned();
                    let picker = Arc::clone(&self.picker);
                    let preview_reader = Arc::clone(&self.media_preview_read);
                    let file = match &self.messages.get(&message_id).unwrap().message {
                        wr::MessageContent::File(file) => Some(file.clone()),
                        _ => None,
                    };
                    if let Some(file) = file {
                        // Only animated work needs a slot; still and other media jobs stay unchanged.
                        let animation_slot = if matches!(file.kind, wr::FileKind::Sticker) {
                            match self.animation_slots.try_acquire() {
                                Some(slot) => Some(slot),
                                None if self.pending_sticker_previews.len() >= 256 => {
                                    // At capacity, keep a stable still preview instead of
                                    // repeatedly requesting an unqueued animation.
                                    None
                                }
                                None => {
                                    // Loading stays pending until a result releases a slot.
                                    if !self
                                        .pending_sticker_previews
                                        .iter()
                                        .any(|(id, _)| id == &message_id)
                                    {
                                        self.pending_sticker_previews
                                            .push_back((message_id, file.path));
                                    }
                                    return false;
                                }
                            }
                        } else {
                            None
                        };
                        media_jobs.spawn(move |permit| {
                            let preview_path = match file.kind {
                                wr::FileKind::Video => Path::new(file.path.as_ref())
                                    .with_extension("jpg")
                                    .to_string_lossy()
                                    .to_string(),
                                _ => file.path.to_string(),
                            };
                            let root = MediaRoot::new(&media_path).ok();
                            let preview_file = root.as_ref().and_then(|root| {
                                root.open_media_file(Path::new(&preview_path)).ok()
                            });
                            let generated = if matches!(file.kind, wr::FileKind::Video)
                                && !preview_file
                                    .as_ref()
                                    .is_some_and(has_decent_video_thumbnail)
                            {
                                root.as_ref().and_then(|root| {
                                    generate_video_thumbnail(root, Path::new(file.path.as_ref()))
                                })
                            } else {
                                None
                            };
                            let animation_slot = if let Some(slot) = animation_slot {
                                match crate::app::sticker_preview_worker::try_animated_preview(
                                    preview_reader.as_ref(),
                                    Path::new(&preview_path),
                                    message_id.clone(),
                                    file.path.clone(),
                                    slot,
                                    &picker,
                                    &permit,
                                    &tx,
                                ) {
                                    Ok(()) => return,
                                    Err(slot) => Some(slot),
                                }
                            } else {
                                None
                            };
                            let image_res =
                                generated.or_else(|| preview_file.and_then(decode_media_image));
                            if let Some(mut image_src) = image_res {
                                if matches!(file.kind, wr::FileKind::Video) {
                                    apply_video_play_marker(&mut image_src);
                                }
                                let dimensions = (image_src.width(), image_src.height());
                                let source =
                                    matches!(file.kind, wr::FileKind::Image | wr::FileKind::Video)
                                        .then(|| {
                                            crate::ui::message_list::bounded_stack_source(
                                                &image_src,
                                            )
                                        });
                                // Native preview state also owns only bounded pixels,
                                // not a second retained full-resolution decode.
                                let preview_pixels = source
                                    .as_ref()
                                    .map(|pixels| (**pixels).clone())
                                    .unwrap_or(image_src);
                                let mut img =
                                    picker.lock().unwrap().new_resize_protocol(preview_pixels);
                                let (preview_width, preview_height) =
                                    if matches!(file.kind, wr::FileKind::Video) {
                                        (VIDEO_WIDTH, VIDEO_HEIGHT)
                                    } else {
                                        (IMAGE_WIDTH, IMAGE_HEIGHT)
                                    };
                                img.resize_encode(
                                    &Resize::Scale(None),
                                    Size::new(preview_width as u16, preview_height as u16),
                                );
                                drop(animation_slot);
                                permit.send(
                                    &tx,
                                    AppInput::App(AppEvent::SetFilePreview(
                                        message_id.clone(),
                                        file.path.clone(),
                                        img,
                                        source.map(|pixels| (pixels, dimensions)),
                                    )),
                                );
                            } else if matches!(file.kind, wr::FileKind::Video) {
                                drop(animation_slot);
                                permit.send(
                                    &tx,
                                    AppInput::App(AppEvent::SetFileState(
                                        message_id.clone(),
                                        FileMeta::Loaded,
                                    )),
                                );
                            } else {
                                drop(animation_slot);
                                permit.send(
                                    &tx,
                                    AppInput::App(AppEvent::SetFileState(
                                        message_id.clone(),
                                        FileMeta::LoadFailed,
                                    )),
                                );
                            }
                        });
                    } else {
                        log::error!("Expected a file message for preview");
                    }
                }
                false
            }
            AppEvent::SetFileState(message_id, state) => {
                self.retry_pending_sticker_previews(download_tx, media_jobs);
                if let Some(viewer) = self.attachment_viewer.as_mut()
                    && viewer.message_id == message_id
                {
                    viewer.status = match &state {
                        FileMeta::Loaded | FileMeta::Downloaded => ViewerStatus::Ready,
                        FileMeta::Loading | FileMeta::Downloading => ViewerStatus::Downloading,
                        FileMeta::LoadFailed | FileMeta::DownloadFailed => ViewerStatus::Failed,
                    };
                }
                self.metadata
                    .insert(message_id.clone(), Metadata::File(state));
                self.message_height_cache.invalidate(&message_id);
                if matches!(
                    self.metadata.get(&message_id),
                    Some(Metadata::File(FileMeta::Downloaded | FileMeta::Loaded))
                ) {
                    self.spawn_audio_duration_probe_if_missing(&message_id, media_jobs);
                }
                true
            }
            AppEvent::SetAudioDuration(_message_id, path, duration) => {
                if let Some(duration) = duration {
                    self.audio_durations.insert(path, duration);
                }
                true
            }
            AppEvent::DownloadFile(message_id, file_id) => {
                if matches!(
                    self.metadata.get(&message_id),
                    Some(Metadata::File(FileMeta::Downloading))
                ) {
                    false
                } else {
                    self.metadata
                        .insert(message_id.clone(), Metadata::File(FileMeta::Downloading));
                    self.message_height_cache.invalidate(&message_id);
                    download_tx.send((message_id, file_id)).unwrap();
                    false
                }
            }
            AppEvent::DownloadFileDone(message_id, state) => {
                self.metadata
                    .insert(message_id.clone(), Metadata::File(state));
                self.message_height_cache.invalidate(&message_id);
                true
            }
            _ => unreachable!(
                "runtime_loop must route only MediaViewer events to handle_media_viewer_event"
            ),
        }
    }
}

fn decode_media_image(file: File) -> Option<image::DynamicImage> {
    image::ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()
}
