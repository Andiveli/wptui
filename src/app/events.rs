use core::fmt;
use std::sync::Arc;

use ratatui::crossterm::event::Event;

use ratatui_image::protocol::StatefulProtocol;

use whatsrust as wr;

use crate::app::contact_avatars::{AvatarResult, AvatarTarget};
use crate::app::read_receipts::{
    PersistResult, ReceiptCandidate, ReceiptKey, ReceiptSendStatus, RepositoryError,
};
use crate::app::{App, FileMeta};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ViewerStatus {
    Ready,
    Downloading,
    Missing,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewerPreviewKey {
    pub path: Arc<str>,
    pub is_video: bool,
    pub width: u16,
    pub height: u16,
}

impl ViewerPreviewKey {
    pub fn new(path: impl Into<Arc<str>>, width: u16, height: u16) -> Self {
        Self::for_attachment(path, wr::FileKind::Image, width, height)
    }

    pub fn for_attachment(
        path: impl Into<Arc<str>>,
        kind: wr::FileKind,
        width: u16,
        height: u16,
    ) -> Self {
        Self {
            path: path.into(),
            is_video: matches!(kind, wr::FileKind::Video),
            width,
            height,
        }
    }

    pub fn preview_path(&self) -> Arc<str> {
        if self.is_video {
            std::path::Path::new(self.path.as_ref())
                .with_extension("jpg")
                .to_string_lossy()
                .into_owned()
                .into()
        } else {
            self.path.clone()
        }
    }
}

pub enum ViewerPreviewState {
    Loading(ViewerPreviewKey),
    Ready {
        key: ViewerPreviewKey,
        protocol: Box<StatefulProtocol>,
    },
    Failed(ViewerPreviewKey),
}

impl ViewerPreviewState {
    pub fn key(&self) -> &ViewerPreviewKey {
        match self {
            Self::Loading(key) | Self::Failed(key) | Self::Ready { key, .. } => key,
        }
    }
}

pub fn viewer_preview_request(
    state: &mut Option<ViewerPreviewState>,
    key: ViewerPreviewKey,
) -> Option<ViewerPreviewKey> {
    if state.as_ref().is_some_and(|current| current.key() == &key) {
        None
    } else {
        *state = Some(ViewerPreviewState::Loading(key.clone()));
        Some(key)
    }
}

pub(crate) fn viewer_preview_needs_load(
    state: &Option<ViewerPreviewState>,
    key: &ViewerPreviewKey,
) -> bool {
    state.as_ref().is_none_or(|current| current.key() != key)
}

#[derive(Clone, Debug)]
pub struct ViewerAttachment {
    pub message_id: wr::MessageId,
    pub kind: wr::FileKind,
    pub path: Arc<str>,
    pub status: ViewerStatus,
}

#[derive(Clone, Debug)]
pub struct AttachmentViewerState {
    pub message_id: wr::MessageId,
    pub index: usize,
    pub attachment_count: usize,
    pub kind: wr::FileKind,
    pub path: Arc<str>,
    pub status: ViewerStatus,
    pub attachments: Vec<ViewerAttachment>,
}

impl AttachmentViewerState {
    pub fn from_attachments(attachments: Vec<ViewerAttachment>, index: usize) -> Self {
        let attachment_count = attachments.len().max(1);
        let index = index.min(attachment_count - 1);
        let active = attachments.get(index);
        Self {
            message_id: active.map_or_else(|| "".into(), |item| item.message_id.clone()),
            index,
            attachment_count,
            kind: active.map_or(wr::FileKind::Image, |item| item.kind.clone()),
            path: active.map_or_else(|| "".into(), |item| item.path.clone()),
            status: active.map_or(ViewerStatus::Missing, |item| item.status.clone()),
            attachments,
        }
    }

    pub fn navigate(&mut self, delta: isize) {
        self.index = self
            .index
            .saturating_add_signed(delta)
            .min(self.attachment_count - 1);
        if let Some(active) = self.attachments.get(self.index) {
            self.message_id = active.message_id.clone();
            self.kind = active.kind.clone();
            self.path = active.path.clone();
            self.status = active.status.clone();
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum MediaRenderEffect {
    DownloadFile(wr::MessageId, wr::FileId),
    LoadFilePreview(wr::MessageId),
    LoadViewerPreview(ViewerPreviewKey),
}

#[derive(Default)]
pub struct MediaRenderPlan {
    effects: Vec<MediaRenderEffect>,
}

impl MediaRenderPlan {
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    pub(crate) fn append(&mut self, effect: MediaRenderEffect) {
        self.effects.push(effect);
    }

    pub(crate) fn into_effects(self) -> Vec<MediaRenderEffect> {
        self.effects
    }
}

pub enum AppEvent {
    UpdateAvailable(String),
    OutboundSendSucceeded {
        local_send_id: u64,
        message: wr::Message,
    },
    OutboundSendFailed {
        local_send_id: u64,
    },
    ReadReceiptResult(ReceiptKey, ReceiptSendStatus),
    ReadReceiptRestored(Result<Vec<ReceiptCandidate>, RepositoryError>),
    ReadReceiptPersisted(ReceiptCandidate, PersistResult),
    ReadReceiptCompleted(ReceiptKey, Result<(), RepositoryError>),
    ReadReceiptRejected(ReceiptKey, Result<(), RepositoryError>),
    DownloadFile(wr::MessageId, wr::FileId),
    DownloadFileDone(wr::MessageId, FileMeta),
    LoadFilePreview(wr::MessageId),
    SetFilePreview(wr::MessageId, Arc<str>, StatefulProtocol),
    LoadViewerPreview(ViewerPreviewKey),
    SetViewerPreview(ViewerPreviewKey, Option<StatefulProtocol>),
    SetFileState(wr::MessageId, FileMeta),
    SetAudioDuration(wr::MessageId, Arc<str>, Option<u64>),
    ContactAvatar(AvatarResult),
    ContactAvatarRefreshed {
        generation: u64,
        target: AvatarTarget,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppEventFamily {
    Updater,
    Send,
    ReadReceipt,
    MediaViewer,
    Avatar,
}

impl AppEvent {
    pub const fn family(&self) -> AppEventFamily {
        match self {
            Self::UpdateAvailable(_) => AppEventFamily::Updater,
            Self::OutboundSendSucceeded { .. } | Self::OutboundSendFailed { .. } => {
                AppEventFamily::Send
            }
            Self::ReadReceiptResult(_, _)
            | Self::ReadReceiptRestored(_)
            | Self::ReadReceiptPersisted(_, _)
            | Self::ReadReceiptCompleted(_, _)
            | Self::ReadReceiptRejected(_, _) => AppEventFamily::ReadReceipt,
            Self::DownloadFile(_, _)
            | Self::DownloadFileDone(_, _)
            | Self::LoadFilePreview(_)
            | Self::SetFilePreview(_, _, _)
            | Self::LoadViewerPreview(_)
            | Self::SetViewerPreview(_, _)
            | Self::SetFileState(_, _)
            | Self::SetAudioDuration(_, _, _) => AppEventFamily::MediaViewer,
            Self::ContactAvatar(_) | Self::ContactAvatarRefreshed { .. } => AppEventFamily::Avatar,
        }
    }
}

#[derive(Debug)]
pub enum AppInput {
    Draw(DrawSource),
    App(AppEvent),
    Message { message: wr::Message, is_sync: bool },
    Presence(wr::PresenceUpdate),
    WhatsApp(wr::Event),
    Terminal(Event),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrawSource {
    Ordinary,
    GoLog,
}

impl fmt::Debug for AppEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppEvent::UpdateAvailable(version) => {
                f.debug_tuple("UpdateAvailable").field(version).finish()
            }
            AppEvent::OutboundSendSucceeded {
                local_send_id,
                message,
            } => f
                .debug_struct("OutboundSendSucceeded")
                .field("local_send_id", local_send_id)
                .field("server_message_id", &message.info.id)
                .finish(),
            AppEvent::OutboundSendFailed { local_send_id } => f
                .debug_struct("OutboundSendFailed")
                .field("local_send_id", local_send_id)
                .finish(),
            AppEvent::ReadReceiptResult(key, status) => f
                .debug_tuple("ReadReceiptResult")
                .field(key)
                .field(status)
                .finish(),
            AppEvent::ReadReceiptRestored(result) => f
                .debug_tuple("ReadReceiptRestored")
                .field(&result.as_ref().map(|items| items.len()))
                .finish(),
            AppEvent::ReadReceiptPersisted(candidate, result) => f
                .debug_tuple("ReadReceiptPersisted")
                .field(&candidate.key())
                .field(result)
                .finish(),
            AppEvent::ReadReceiptCompleted(key, result) => f
                .debug_tuple("ReadReceiptCompleted")
                .field(key)
                .field(result)
                .finish(),
            AppEvent::ReadReceiptRejected(key, result) => f
                .debug_tuple("ReadReceiptRejected")
                .field(key)
                .field(result)
                .finish(),
            AppEvent::DownloadFile(message_id, file_id) => f
                .debug_tuple("DownloadFile")
                .field(message_id)
                .field(file_id)
                .finish(),
            AppEvent::DownloadFileDone(message_id, state) => f
                .debug_tuple("DownloadFileDone")
                .field(message_id)
                .field(state)
                .finish(),
            AppEvent::LoadFilePreview(message_id) => {
                f.debug_tuple("LoadFilePreview").field(message_id).finish()
            }
            AppEvent::SetFilePreview(message_id, path, _) => f
                .debug_tuple("SetFilePreview")
                .field(message_id)
                .field(path)
                .finish(),
            AppEvent::LoadViewerPreview(key) => {
                f.debug_tuple("LoadViewerPreview").field(key).finish()
            }
            AppEvent::SetViewerPreview(key, protocol) => f
                .debug_tuple("SetViewerPreview")
                .field(key)
                .field(&protocol.is_some())
                .finish(),
            AppEvent::SetFileState(message_id, state) => f
                .debug_tuple("SetFileState")
                .field(message_id)
                .field(state)
                .finish(),
            AppEvent::SetAudioDuration(message_id, path, duration) => f
                .debug_struct("SetAudioDuration")
                .field("message_id", message_id)
                .field("path", path)
                .field("duration_secs", duration)
                .finish(),
            AppEvent::ContactAvatar(result) => f
                .debug_tuple("ContactAvatar")
                .field(&result.generation())
                .field(result.target())
                .finish(),
            AppEvent::ContactAvatarRefreshed { generation, target } => f
                .debug_struct("ContactAvatarRefreshed")
                .field("generation", generation)
                .field("target", target)
                .finish(),
        }
    }
}

impl App<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_families_classify_every_runtime_owner() {
        assert_eq!(
            AppEvent::UpdateAvailable("1.2.3".to_owned()).family(),
            AppEventFamily::Updater
        );
        assert_eq!(
            AppEvent::OutboundSendFailed { local_send_id: 1 }.family(),
            AppEventFamily::Send
        );
        assert_eq!(
            AppEvent::ReadReceiptRestored(Ok(vec![])).family(),
            AppEventFamily::ReadReceipt
        );
        assert_eq!(
            AppEvent::LoadFilePreview("message-1".into()).family(),
            AppEventFamily::MediaViewer
        );
        assert_eq!(
            AppEvent::ContactAvatarRefreshed {
                generation: 1,
                target: AvatarTarget::Contact("contact@s.whatsapp.net".to_owned().into()),
            }
            .family(),
            AppEventFamily::Avatar
        );
    }
}
