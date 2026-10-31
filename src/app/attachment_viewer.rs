use std::path::{Component, Path};
use std::{fs, io};

use super::App;
use crate::app::actions::{ActionNotice, Section};
use crate::app::events::{
    AttachmentViewerState, SaveDirectoryPicker, ViewerAttachment, ViewerStatus,
};
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

    pub(crate) fn start_viewer_save(&mut self) {
        let Some(viewer) = self.attachment_viewer.as_mut() else {
            return self.unavailable("Media is not downloaded");
        };
        if viewer.status != ViewerStatus::Ready {
            return self.unavailable("Media is not downloaded");
        }
        let name = Path::new(viewer.path.as_ref())
            .file_name()
            .filter(|name| !name.is_empty())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "attachment.bin".to_owned());
        let Ok(directory) = std::env::current_dir() else {
            return self.unavailable("Could not determine save directory");
        };
        match SaveDirectoryPicker::open(directory, name) {
            Ok(picker) => {
                viewer.saved_path = None;
                viewer.save_as = Some(picker);
            }
            Err(error) => self.unavailable(&format!("Could not open save directory: {error}")),
        }
    }

    pub(crate) fn confirm_viewer_save(&mut self) {
        let Some(viewer) = self.attachment_viewer.as_ref() else {
            return self.unavailable("Media is not downloaded");
        };
        if viewer.status != ViewerStatus::Ready {
            return self.unavailable("Media is not downloaded");
        }
        let Some(picker) = viewer.save_as.as_ref() else {
            return self.unavailable("Save as is not active");
        };
        if !picker.editing_name {
            return self.unavailable("Edit the filename before confirming save");
        }
        let filename = Path::new(&picker.filename);
        if !matches!(filename.components().collect::<Vec<_>>().as_slice(), [Component::Normal(_)])
            || picker.filename.contains('/')
            || picker.filename.contains('\\')
        {
            return self.unavailable("Enter one valid filename (no path separators)");
        }
        let target = picker.directory.join(filename);
        let result = (|| -> io::Result<std::path::PathBuf> {
            let root = crate::media::MediaRoot::new(&self.media_path)
                .map_err(|error| io::Error::other(format!("Invalid media root: {error:?}")))?;
            let source = root
                .media_file(Path::new(viewer.path.as_ref()))
                .map_err(|error| io::Error::other(format!("Invalid source: {error:?}")))?;
            let parent = &picker.directory;
            let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
            let mut input = fs::File::open(source)?;
            io::copy(&mut input, &mut temporary)?;
            temporary
                .persist_noclobber(&target)
                .map_err(|error| error.error)?;
            Ok(target)
        })();
        match result {
            Ok(path) => {
                let path = path.to_string_lossy().into_owned();
                if let Some(viewer) = self.attachment_viewer.as_mut() {
                    viewer.save_as = None;
                    viewer.saved_path = Some(path.clone());
                }
                self.action_notice =
                    Some(ActionNotice::Unavailable(format!("Media saved to {path}")));
            }
            Err(error) => self.unavailable(&format!("Could not save media: {error}")),
        }
    }

    pub(crate) fn navigate_save_directory(&mut self, parent: bool) {
        let result = self
            .attachment_viewer
            .as_mut()
            .and_then(|viewer| viewer.save_as.as_mut())
            .map(|picker| {
                if parent {
                    picker.parent()
                } else {
                    picker.descend()
                }
            });
        if let Some(Err(error)) = result {
            self.unavailable(&format!("Could not open directory: {error}"));
        }
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
                match crate::media::execute_launch(&plan, self.launch_executor.as_mut()) {
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
    use crate::app::actions::AppAction;
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

    fn save_fixture(app: &mut TestApp, directory: &Path) {
        app.dispatch_action(AppAction::ViewerSaveAs);
        // Start at a fixture directory without changing process-wide cwd.
        let picker = app.attachment_viewer.as_mut().unwrap().save_as.as_mut().unwrap();
        let filename = picker.filename.clone();
        *picker = SaveDirectoryPicker::open(directory.to_path_buf(), filename).unwrap();
    }

    #[test]
    fn save_as_navigates_directories_and_copies_image_and_video() {
        let mut app = TestApp::new();
        fs::create_dir_all(&app.media_path).unwrap();
        let destination = tempfile::tempdir().unwrap();
        fs::create_dir(destination.path().join("nested")).unwrap();
        let attachments = [wr::FileKind::Image, wr::FileKind::Video]
            .into_iter()
            .enumerate()
            .map(|(index, kind)| {
                let name = format!("media{index}.bin");
                fs::write(app.media_path.join(&name), name.as_bytes()).unwrap();
                ViewerAttachment {
                    message_id: name.clone().into(),
                    kind,
                    path: name.into(),
                    status: ViewerStatus::Ready,
                }
            })
            .collect();
        app.attachment_viewer = Some(AttachmentViewerState::from_attachments(attachments, 0));
        for index in 0..2 {
            save_fixture(&mut app, destination.path());
            app.dispatch_action(AppAction::ViewerSaveDescend);
            let target = destination.path().join("nested").join(format!("media{index}.bin"));
            app.dispatch_action(AppAction::ViewerSaveConfirm);
            assert!(!target.exists()); // Browsing alone cannot save.
            app.dispatch_action(AppAction::ViewerSaveEditName);
            app.dispatch_action(AppAction::ViewerSaveConfirm);
            assert_eq!(fs::read(&target).unwrap(), format!("media{index}.bin").as_bytes());
            let viewer = app.attachment_viewer.as_ref().unwrap();
            assert!(viewer.save_as.is_none());
            assert_eq!(viewer.saved_path.as_deref(), target.to_str());
            assert_eq!(
                app.action_notice,
                Some(ActionNotice::Unavailable(format!("Media saved to {}", target.display())))
            );
            app.dispatch_action(AppAction::ViewerNext);
            assert!(app.attachment_viewer.as_ref().unwrap().saved_path.is_none());
        }
        assert_eq!(app.attachment_viewer.as_ref().unwrap().index, 1);
    }

    #[test]
    fn save_as_rejects_names_existing_targets_and_invalid_sources() {
        let mut app = TestApp::new();
        fs::create_dir_all(&app.media_path).unwrap();
        fs::write(app.media_path.join("safe.bin"), b"new").unwrap();
        let destination = tempfile::tempdir().unwrap();
        let target = destination.path().join("saved.bin");
        fs::write(&target, b"original").unwrap();
        app.attachment_viewer = Some(AttachmentViewerState::from_attachments(
            vec![ViewerAttachment {
                message_id: "image".into(),
                kind: wr::FileKind::Image,
                path: "safe.bin".into(),
                status: ViewerStatus::Ready,
            }],
            0,
        ));
        save_fixture(&mut app, destination.path());
        app.dispatch_action(AppAction::ViewerSaveEditName);
        for name in [
            "", ".", "..", "../escape", "sub/file", "/absolute", "back\\slash", "saved.bin",
        ] {
            // Clear the proposed name using the same public editing actions as the terminal.
            let length = app.attachment_viewer.as_ref().unwrap()
                .save_as.as_ref().unwrap().filename.chars().count();
            for _ in 0..length {
                app.dispatch_action(AppAction::ViewerSaveBackspace);
            }
            for character in name.chars() {
                app.dispatch_action(AppAction::ViewerSaveCharacter(character));
            }
            app.dispatch_action(AppAction::ViewerSaveConfirm);
            assert_eq!(fs::read(&target).unwrap(), b"original");
            assert!(!destination.path().join("escape").exists());
            assert!(!destination.path().join("sub").exists());
            assert!(app.attachment_viewer.as_ref().unwrap().save_as.is_some());
        }
        for source in ["../outside.bin", "missing.bin"] {
            app.attachment_viewer.as_mut().unwrap().path = source.into();
            let length = app.attachment_viewer.as_ref().unwrap()
                .save_as.as_ref().unwrap().filename.chars().count();
            for _ in 0..length {
                app.dispatch_action(AppAction::ViewerSaveBackspace);
            }
            for character in "absent.bin".chars() {
                app.dispatch_action(AppAction::ViewerSaveCharacter(character));
            }
            app.dispatch_action(AppAction::ViewerSaveConfirm);
            assert!(!destination.path().join("absent.bin").exists());
            assert_eq!(fs::read(&target).unwrap(), b"original");
        }
        app.attachment_viewer.as_mut().unwrap().status = ViewerStatus::Missing;
        app.dispatch_action(AppAction::ViewerSaveAs);
        assert!(app.attachment_viewer.as_ref().unwrap().save_as.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn save_as_can_browse_directory_symlinks_like_the_file_picker() {
        let mut app = TestApp::new();
        fs::create_dir_all(&app.media_path).unwrap();
        fs::write(app.media_path.join("image.jpg"), b"image").unwrap();
        let root = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(destination.path(), root.path().join("linked")).unwrap();
        app.attachment_viewer = Some(AttachmentViewerState::from_attachments(
            vec![ViewerAttachment {
                message_id: "image".into(),
                kind: wr::FileKind::Image,
                path: "image.jpg".into(),
                status: ViewerStatus::Ready,
            }],
            0,
        ));
        save_fixture(&mut app, root.path());
        app.dispatch_action(AppAction::ViewerSaveDescend);
        app.dispatch_action(AppAction::ViewerSaveEditName);
        app.dispatch_action(AppAction::ViewerSaveConfirm);
        assert_eq!(fs::read(destination.path().join("image.jpg")).unwrap(), b"image");
    }

    #[test]
    fn directory_navigation_scrolls_and_cancel_preserves_viewer() {
        let mut app = TestApp::new();
        let root = tempfile::tempdir().unwrap();
        for name in ["alpha", "beta", "gamma"] {
            fs::create_dir(root.path().join(name)).unwrap();
        }
        app.attachment_viewer = Some(AttachmentViewerState::from_attachments(
            vec![ViewerAttachment {
                message_id: "image".into(),
                kind: wr::FileKind::Image,
                path: "image.jpg".into(),
                status: ViewerStatus::Ready,
            }],
            0,
        ));
        save_fixture(&mut app, root.path());
        app.attachment_viewer.as_mut().unwrap().save_as.as_mut().unwrap()
            .set_viewport_height(1);
        app.dispatch_action(AppAction::ViewerSaveNext);
        app.dispatch_action(AppAction::ViewerSaveNext);
        assert_eq!(app.attachment_viewer.as_ref().unwrap().save_as.as_ref().unwrap().offset, 2);
        app.dispatch_action(AppAction::ViewerSaveDescend);
        assert_eq!(
            app.attachment_viewer.as_ref().unwrap().save_as.as_ref().unwrap().directory,
            root.path().join("gamma")
        );
        app.dispatch_action(AppAction::ViewerSaveParent);
        assert_eq!(
            app.attachment_viewer.as_ref().unwrap().save_as.as_ref().unwrap().directory,
            root.path()
        );
        app.dispatch_action(AppAction::ViewerSaveCancel);
        assert!(app.attachment_viewer.as_ref().unwrap().save_as.is_none());
        assert!(app.attachment_viewer.as_ref().unwrap().saved_path.is_none());
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
