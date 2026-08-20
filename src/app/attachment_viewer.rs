use std::path::Path;

use super::App;
use crate::app::actions::{ActionNotice, Section};
use crate::app::events::{AttachmentViewerState, ViewerAttachment, ViewerStatus};
use whatsrust as wr;

impl App<'_> {
    pub(crate) fn open_attachment_viewer(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let Some(selected) = self.selected_message().cloned() else {
            return self.unavailable("View is not available");
        };
        let wr::MessageContent::File(file) = &selected.message else {
            return self.unavailable("View is not available");
        };
        if matches!(file.kind, wr::FileKind::Sticker) {
            self.action_notice = Some(ActionNotice::Unsupported(
                "Sticker viewer is not supported".into(),
            ));
            return;
        }
        if !matches!(file.kind, wr::FileKind::Image | wr::FileKind::Video) {
            return self.unavailable("View is not available");
        }
        let pool = if self.selected_section == Section::Status {
            self.open_status_contact()
                .map(|contact| self.status_messages(&contact))
                .unwrap_or_default()
        } else {
            self.chat_messages
                .get(&selected.info.chat)
                .cloned()
                .unwrap_or_default()
        };
        let mut attachments = pool
            .iter()
            .filter_map(|id| self.messages.get(id))
            .filter_map(|message| match &message.message {
                wr::MessageContent::File(file)
                    if matches!(file.kind, wr::FileKind::Image | wr::FileKind::Video) =>
                {
                    Some(ViewerAttachment {
                        message_id: message.info.id.clone(),
                        kind: file.kind.clone(),
                        path: file.path.clone(),
                        status: self.viewer_status(&message.info.id),
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !attachments
            .iter()
            .any(|item| item.message_id == selected.info.id)
        {
            attachments.push(ViewerAttachment {
                message_id: selected.info.id.clone(),
                kind: file.kind.clone(),
                path: file.path.clone(),
                status: self.viewer_status(&selected.info.id),
            });
        }
        let index = attachments
            .iter()
            .position(|item| item.message_id == selected.info.id)
            .unwrap_or_default();
        self.attachment_viewer = Some(AttachmentViewerState::from_attachments(attachments, index));
        self.viewer_preview = None;
    }

    pub(crate) fn navigate_viewer(&mut self, delta: isize) {
        if let Some(viewer) = &mut self.attachment_viewer {
            viewer.navigate(delta);
            self.viewer_preview = None;
        }
    }

    pub(crate) fn plan_viewer_media_launch(&mut self) {
        let Some(viewer) = self.attachment_viewer.as_ref().cloned() else {
            return self.unavailable("Media is not downloaded");
        };
        if viewer.status != ViewerStatus::Ready {
            return self.unavailable("Media is not downloaded");
        }
        self.plan_media_launch(&viewer.kind, Path::new(viewer.path.as_ref()));
    }

    pub(crate) fn plan_selected_media_launch(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let Some(message) = self.selected_message() else {
            return self.unavailable("Media is not downloaded");
        };
        let wr::MessageContent::File(file) = &message.message else {
            return self.unavailable("Media is not downloaded");
        };
        let kind = file.kind.clone();
        let path = file.path.clone();
        if !matches!(
            self.metadata.get(&message.info.id),
            Some(crate::app::Metadata::File(
                crate::app::FileMeta::Downloaded | crate::app::FileMeta::Loaded
            ))
        ) {
            return self.unavailable("Media is not downloaded");
        }
        self.plan_media_launch(&kind, Path::new(path.as_ref()));
    }

    fn viewer_status(&self, message_id: &wr::MessageId) -> ViewerStatus {
        match self.metadata.get(message_id) {
            Some(crate::app::Metadata::File(
                crate::app::FileMeta::Downloaded | crate::app::FileMeta::Loaded,
            )) => ViewerStatus::Ready,
            Some(crate::app::Metadata::File(
                crate::app::FileMeta::Downloading | crate::app::FileMeta::Loading,
            )) => ViewerStatus::Downloading,
            Some(crate::app::Metadata::File(
                crate::app::FileMeta::DownloadFailed | crate::app::FileMeta::LoadFailed,
            )) => ViewerStatus::Failed,
            _ => ViewerStatus::Missing,
        }
    }

    fn plan_media_launch(&mut self, kind: &wr::FileKind, path: &Path) {
        let player = match crate::media::media_opener_from_environment(kind) {
            Ok(player) => player,
            Err(_) => {
                return self.unavailable(
                    "Set WPTUI_IMAGE_VIEWER or WPTUI_MEDIA_PLAYER to one executable name",
                );
            }
        };
        match crate::media::plan_media_launch(Some(&self.media_path), Some(&player), Some(path)) {
            Ok(Some(plan)) => {
                match crate::media::execute_launch(&plan, &mut crate::media::CommandLaunchExecutor)
                {
                    Ok(()) => {
                        self.action_notice =
                            Some(ActionNotice::Unavailable("Media player started".into()))
                    }
                    Err(error) => {
                        self.unavailable(&format!("Could not start media player: {error:?}"))
                    }
                }
            }
            Ok(None) | Err(_) => self.unavailable("Media launch is unavailable"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::TestApp;

    fn media_message(id: &str, kind: wr::FileKind) -> wr::Message {
        wr::Message {
            info: wr::MessageInfo {
                id: id.into(),
                chat: "chat@g.us".to_owned().into(),
                sender: "chat@g.us".to_owned().into(),
                mentions_self: false,
                timestamp: 1,
                forwarding: Default::default(),
                is_from_me: false,
                quote_id: None,
                read_by: 0,
            },
            message: wr::MessageContent::File(wr::FileContent {
                kind,
                path: format!("{id}.bin").into(),
                ..Default::default()
            }),
        }
    }

    #[test]
    fn viewer_collects_supported_media_and_keeps_selected_index() {
        let mut app = TestApp::new();
        let image = media_message("image", wr::FileKind::Image);
        let selected = media_message("selected", wr::FileKind::Video);
        let audio = media_message("audio", wr::FileKind::Audio);
        let chat = selected.info.chat.clone();
        for message in [image, selected, audio] {
            app.chat_messages
                .entry(chat.clone())
                .or_default()
                .push(message.info.id.clone());
            app.messages.insert(message.info.id.clone(), message);
        }
        app.message_list_state
            .set_selected_message("selected".into());

        app.open_attachment_viewer();

        let viewer = app.attachment_viewer.as_ref().expect("viewer opens");
        assert_eq!((viewer.attachment_count, viewer.index), (2, 1));
        assert_eq!(viewer.message_id.as_ref(), "selected");
    }
}
