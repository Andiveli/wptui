use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;

use ratatui::layout::Size;
use ratatui_image::{Resize, ResizeEncodeRender};
use whatsrust as wr;

use crate::app::events::{AppEvent, AppInput, ViewerPreviewState, ViewerStatus};
use crate::app::media_support::{
    apply_video_play_marker, generate_video_thumbnail, has_decent_video_thumbnail,
};
use crate::app::{App, FileMeta, Metadata};
use crate::media::MediaRoot;
use crate::ui::message_list::{IMAGE_HEIGHT, IMAGE_WIDTH, VIDEO_HEIGHT, VIDEO_WIDTH};

type DownloadSender = Sender<(wr::MessageId, wr::FileId)>;

impl App<'_> {
    pub(crate) fn handle_media_viewer_event(
        &mut self,
        event: AppEvent,
        download_tx: &DownloadSender,
    ) -> bool {
        match event {
            AppEvent::OutboundSendSucceeded { .. } | AppEvent::OutboundSendFailed { .. } => {
                unreachable!("runtime_loop must route Send events to handle_send_event")
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
            AppEvent::SetFilePreview(message_id, file_path, img) => {
                self.cache_file_preview(message_id.clone(), file_path, img);
                if let Some(viewer) = self.attachment_viewer.as_mut()
                    && viewer.message_id == message_id
                {
                    viewer.status = ViewerStatus::Ready;
                }
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
                    thread::spawn(move || {
                        let protocol = MediaRoot::new(&media_path)
                            .and_then(|root| {
                                root.media_file(Path::new(key.preview_path().as_ref()))
                            })
                            .ok()
                            .and_then(|path| image::ImageReader::open(path).ok())
                            .and_then(|reader| reader.decode().ok())
                            .map(|image| {
                                let mut protocol =
                                    picker.lock().unwrap().new_resize_protocol(image);
                                protocol.resize_encode(
                                    &Resize::Scale(None),
                                    Size::new(key.width, key.height),
                                );
                                protocol
                            });
                        let _ = tx.send(AppInput::App(AppEvent::SetViewerPreview(key, protocol)));
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
                    let file = match &self.messages.get(&message_id).unwrap().message {
                        wr::MessageContent::File(file) => Some(file.clone()),
                        _ => None,
                    };
                    if let Some(file) = file {
                        thread::spawn(move || {
                            let preview_path = match file.kind {
                                wr::FileKind::Video => {
                                    let video_rel = Path::new(file.path.as_ref());
                                    let sidecar_rel = video_rel.with_extension("jpg");
                                    let sidecar_abs = media_path.join(&sidecar_rel);
                                    if !has_decent_video_thumbnail(&sidecar_abs) {
                                        let video_abs = media_path.join(video_rel);
                                        generate_video_thumbnail(&video_abs, &sidecar_abs);
                                    }
                                    sidecar_rel.to_string_lossy().to_string()
                                }
                                _ => file.path.to_string(),
                            };
                            let image_res = MediaRoot::new(&media_path)
                                .and_then(|root| root.media_file(Path::new(&preview_path)))
                                .ok()
                                .and_then(|path| image::ImageReader::open(path).ok())
                                .and_then(|reader| reader.decode().ok());
                            if let Some(mut image_src) = image_res {
                                if matches!(file.kind, wr::FileKind::Video) {
                                    apply_video_play_marker(&mut image_src);
                                }
                                let mut img = picker.lock().unwrap().new_resize_protocol(image_src);
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
                                tx.send(AppInput::App(AppEvent::SetFilePreview(
                                    message_id.clone(),
                                    file.path.clone(),
                                    img,
                                )))
                                .unwrap();
                            } else if matches!(file.kind, wr::FileKind::Video) {
                                tx.send(AppInput::App(AppEvent::SetFileState(
                                    message_id.clone(),
                                    FileMeta::Loaded,
                                )))
                                .unwrap();
                            } else {
                                tx.send(AppInput::App(AppEvent::SetFileState(
                                    message_id.clone(),
                                    FileMeta::LoadFailed,
                                )))
                                .unwrap();
                            }
                        });
                    } else {
                        log::error!("Expected a file message for preview");
                    }
                }
                false
            }
            AppEvent::SetFileState(message_id, state) => {
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
                    self.spawn_audio_duration_probe_if_missing(&message_id);
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
