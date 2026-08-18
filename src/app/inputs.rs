use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::app::actions::{
    AppAction, COMMON_REACTIONS, ComposerAction, ConversationMode, FocusPane, Section,
    SequenceResolution, focus_after, focus_after_visibility_change,
};
use std::path::Path;

use crate::app::App;
use crate::app::composer::{Composer, ComposerOutcome};
use crate::app::input_mapping::{
    attachment_viewer_action, file_picker_navigation_action, file_picker_search_action,
    message_menu_action, reaction_picker_action, share_picker_action, url_picker_action,
};
use crate::clipboard::{self, ClipboardError, ClipboardPaste};
use crate::file_picker::FilePickerState;
use crate::key_handler::Key;
use whatsrust as wr;

impl App<'_> {
    pub fn on_terminal_event(&mut self, event: Event) {
        if let Event::Key(key_event) = event
            && key_event.kind == KeyEventKind::Press
        {
            let key = Key {
                code: key_event.code,
                modifiers: key_event.modifiers,
            };

            if self.pending_logout {
                if self.logout_in_progress {
                    // The async logout is running (off the event loop). Ignore
                    // further input until Event::LogoutResult resolves.
                    return;
                }
                self.handle_logout_input(key);
                return;
            }

            if is_toggle_logs_key(&key) {
                self.dispatch_action(AppAction::ToggleLogs);
            } else if self.focus_pane == FocusPane::Conversation
                && self.selected_section == Section::Chats
                && matches!(
                    self.conversation_mode,
                    ConversationMode::ComposerEditing | ConversationMode::EditingMessage
                )
            {
                if key == Key::k(KeyCode::Esc) {
                    if self.conversation_mode == ConversationMode::EditingMessage {
                        self.cancel_message_edit();
                    } else {
                        self.composer.apply(ComposerAction::CancelReply);
                        self.composer.pending.clear();
                        self.conversation_mode = ConversationMode::MessageNavigation;
                    }
                } else if is_attach_file_key(&key) {
                    self.dispatch_action(AppAction::AttachFile);
                } else if self.composer_blocked() {
                    return;
                } else {
                    self.dispatch_composer_action(composer_action_for_editing_key(&key));
                }
            } else if self.attachment_viewer.is_some() {
                let Some(action) = attachment_viewer_action(&key) else {
                    return;
                };
                self.dispatch_action(action);
            } else if self.url_picker.is_some() {
                let Some(action) = url_picker_action(&key) else {
                    return;
                };
                self.dispatch_action(action);
            } else if self.file_picker.is_some() && self.file_picker.as_ref().unwrap().searching {
                // Search mode: the keyboard buffer owns every key except the
                // explicit exits, so `h/j/k/l` build the filter instead of
                // moving. Esc leaves search mode back to navigation.
                let Some(action) = file_picker_search_action(&key) else {
                    return;
                };
                self.dispatch_action(action);
            } else if self.file_picker.is_some() {
                // Navigation mode. `h/l` move across the tree, `Enter` commits
                // (files under the cursor, or every `Space`-selected file).
                let Some(action) = file_picker_navigation_action(&key) else {
                    return;
                };
                self.dispatch_action(action);
            } else if self.share_picker.is_some() {
                let Some(action) = share_picker_action(&key) else {
                    return;
                };
                self.dispatch_action(action);
            } else if self.reaction_picker.is_some() {
                let Some(action) = reaction_picker_action(&key) else {
                    return;
                };
                self.dispatch_action(action);
            } else if self.message_menu.is_some() {
                let Some(action) = message_menu_action(&key) else {
                    return;
                };
                self.dispatch_action(action);
            } else if self.focus_pane == FocusPane::Conversation
                && self.selected_section == Section::Status
            {
                // Read-only status view: Esc returns to the contact list and
                // Enter opens the fullscreen viewer on a media status.
                if key == Key::k(KeyCode::Esc) {
                    self.dispatch_action(AppAction::CloseStatusPane);
                } else if key == Key::k(KeyCode::Enter) {
                    self.dispatch_action(AppAction::ViewMessage);
                } else {
                    match self.kh.resolve(key.clone()) {
                        SequenceResolution::Complete(action) => self.dispatch_action(action),
                        SequenceResolution::Partial => {}
                        SequenceResolution::Cancelled => {}
                    }
                }
            } else if self.focus_pane == FocusPane::ChatList && self.contact_search_active {
                self.handle_chat_search_key(key);
            } else if self.focus_pane == FocusPane::ChatList && key == Key::k(KeyCode::Enter) {
                self.dispatch_action(AppAction::OpenChat);
            } else if self.focus_pane == FocusPane::SectionRail
                && self.rail_on_logout
                && key == Key::k(KeyCode::Enter)
            {
                self.begin_logout_confirmation();
            } else {
                match self.kh.resolve(key.clone()) {
                    SequenceResolution::Complete(action) => self.dispatch_action(action),
                    SequenceResolution::Partial => {}
                    SequenceResolution::Cancelled => self.handle_unbound_key(key),
                }
            }
        }
    }

    fn handle_unbound_key(&mut self, key: Key) {
        // Chat search is a Chats-section feature; the status list has its
        // own single-pane navigation.
        if self.focus_pane == FocusPane::ChatList
            && self.selected_section == Section::Chats
            && key.code == KeyCode::Char('/')
        {
            self.contact_search_active = true;
        }
    }

    pub fn dispatch_action(&mut self, action: AppAction) {
        self.focus_pane = focus_after(self.focus_pane, &action, self.pane_visibility);

        // The status view is read-only: chat actions (react, reply, edit,
        // delete, menu, composer) must never target the status@broadcast
        // chat, so they are rejected while a contact's statuses are shown.
        if self.selected_section == Section::Status
            && self.focus_pane == FocusPane::Conversation
            && !status_view_allows(&action)
        {
            return;
        }

        match action {
            AppAction::Quit => {
                self.db_handler.stop();
                self.should_quit = true;
            }
            AppAction::Logout => self.begin_logout_confirmation(),
            AppAction::ToggleLogs => {
                self.show_logs = !self.show_logs;
                tui_logger::set_default_level(log_level_for_logs(self.show_logs));
            }
            AppAction::ToggleSectionRail => {
                self.pane_visibility.section_rail = !self.pane_visibility.section_rail;
                self.focus_pane =
                    focus_after_visibility_change(self.focus_pane, self.pane_visibility);
            }
            AppAction::ToggleChatList => {
                self.pane_visibility.chat_list = !self.pane_visibility.chat_list;
                self.focus_pane =
                    focus_after_visibility_change(self.focus_pane, self.pane_visibility);
            }
            AppAction::FocusNext | AppAction::FocusPrevious => {}
            AppAction::SelectNext => self.select_next(),
            AppAction::SelectPrevious => self.select_previous(),
            AppAction::JumpTop => self.jump_top(),
            AppAction::JumpBottom => self.jump_bottom(),
            AppAction::HalfPageDown => self.half_page_down(),
            AppAction::HalfPageUp => self.half_page_up(),
            AppAction::InsertMode => {
                if self.focus_pane == FocusPane::Conversation && !self.composer_blocked() {
                    self.conversation_mode = ConversationMode::ComposerEditing;
                }
            }
            AppAction::CopyMessage => self.copy_selected_text(),
            AppAction::ReplyMessage => {
                if self.selected_section == Section::Status {
                    self.reply_to_status();
                } else {
                    self.reply_to_selected();
                }
            }
            AppAction::ReplyPrivately => self.reply_privately(),
            AppAction::ShareMessage => self.open_share_picker(),
            AppAction::ReactMessage => {
                if self.selected_section == Section::Status {
                    self.heart_selected_status();
                } else {
                    self.open_reaction_picker();
                }
            }
            AppAction::DeleteMessage => self.delete_selected_message(),
            AppAction::EditMessage => self.start_message_edit(),
            AppAction::OpenChat => {
                if self.selected_section == Section::Status {
                    let opened = self.selected_status_contact().is_some();
                    if opened {
                        self.open_selected_status();
                        self.focus_pane = FocusPane::Conversation;
                        if self.status_message_count() > 0 {
                            self.message_list_state.select(Some(0));
                        }
                    }
                } else if self.selected_section == Section::Communities {
                    if let Some(jid) = self.get_selected_community() {
                        self.open_chat_by_jid(jid);
                        self.focus_pane = FocusPane::Conversation;
                    }
                } else {
                    let opened = self.get_selected_chat().is_some();
                    self.open_selected_chat();
                    if opened {
                        self.focus_pane = FocusPane::Conversation;
                        if self.message_count() > 0 {
                            self.message_list_state.select(Some(0));
                        }
                    }
                }
            }
            AppAction::OpenMessage => self.open_selected_url(),
            AppAction::DownloadMessage => self.plan_selected_media_launch(),
            AppAction::ViewMessage => self.open_attachment_viewer(),
            AppAction::ViewerNext => self.navigate_viewer(1),
            AppAction::ViewerPrevious => self.navigate_viewer(-1),
            AppAction::ViewerZoomIn => {
                self.viewer_zoom = self.viewer_zoom.saturating_add(25).min(400);
                self.viewer_preview = None;
            }
            AppAction::ViewerZoomOut => {
                self.viewer_zoom = self.viewer_zoom.saturating_sub(25).max(25);
                self.viewer_preview = None;
            }
            AppAction::ViewerOpenExternal => self.plan_viewer_media_launch(),
            AppAction::CloseAttachmentViewer => {
                self.attachment_viewer = None;
                self.viewer_preview = None;
            }
            AppAction::CloseStatusPane => {
                self.focus_pane = FocusPane::ChatList;
            }
            AppAction::OpenMessageMenu => self.open_message_menu(),
            AppAction::MenuNext => self.move_menu(1),
            AppAction::MenuPrevious => self.move_menu(-1),
            AppAction::ConfirmMessageMenu => self.confirm_message_menu(),
            AppAction::CancelMessageMenu => {
                self.message_menu = None;
                self.action_notice = Some(crate::app::actions::ActionNotice::Cancelled);
            }
            AppAction::SharePickerPrevious => self.move_share_picker(-1),
            AppAction::SharePickerNext => self.move_share_picker(1),
            AppAction::ToggleShareRecipient => self.toggle_share_recipient(),
            AppAction::ConfirmShare => self.confirm_share(),
            AppAction::CancelShare => {
                self.share_picker = None;
                self.action_notice = Some(crate::app::actions::ActionNotice::Cancelled);
            }
            AppAction::ShareSearchBackspace => self.share_search_backspace(),
            AppAction::ShareSearchCharacter(character) => self.share_search_character(character),
            AppAction::ReactionPrev => self.move_reaction(-1),
            AppAction::ReactionNext => self.move_reaction(1),
            AppAction::ConfirmReaction => self.confirm_reaction(),
            AppAction::CancelReaction => {
                self.reaction_picker = None;
                self.action_notice = Some(crate::app::actions::ActionNotice::Cancelled);
            }
            AppAction::UrlPickerPrevious => self.move_url_picker(-1),
            AppAction::UrlPickerNext => self.move_url_picker(1),
            AppAction::ConfirmUrlPicker => self.confirm_url_picker(),
            AppAction::CancelUrlPicker => {
                self.url_picker = None;
                self.action_notice = Some(crate::app::actions::ActionNotice::Cancelled);
            }
            AppAction::AttachFile => {
                if !self.composer_blocked() {
                    self.open_file_picker()
                }
            }
            AppAction::FilePickerPrevious => self.move_file_picker(-1),
            AppAction::FilePickerNext => self.move_file_picker(1),
            AppAction::FilePickerParent => {
                if !self.file_picker_up() {
                    self.unavailable("Already at the top of the filesystem");
                }
            }
            AppAction::FilePickerDescend => {
                if !self.file_picker_down() {
                    self.unavailable("Cursor is not on a folder");
                }
            }
            AppAction::FilePickerToggle => self.file_picker_toggle(),
            AppAction::FilePickerConfirm => self.confirm_file_picker(),
            AppAction::FilePickerEnterSearch => self.file_picker_enter_search(),
            AppAction::FilePickerEndSearch => self.file_picker_end_search(),
            AppAction::FilePickerBackspace => self.file_picker_backspace(),
            AppAction::FilePickerCharacter(character) => self.file_picker_character(character),
            AppAction::CancelFilePicker => self.cancel_file_picker(),
            AppAction::GoToReference => {
                if !self.follow_selected_reference() {
                    self.unavailable("Reference is not available");
                }
            }
            AppAction::Composer(action) => self.dispatch_composer_action(action),
        }
    }

    pub fn unavailable(&mut self, action: &str) {
        self.action_notice = Some(crate::app::actions::ActionNotice::Unavailable(
            action.into(),
        ));
    }

    fn selected_message_is_deleted(&self) -> bool {
        self.selected_message()
            .is_some_and(|message| self.message_status(&message.info.id).deleted)
    }

    fn open_selected_url(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let urls = self
            .selected_message()
            .map(message_urls)
            .unwrap_or_default();
        match urls.len() {
            0 => self.open_selected_document(),
            1 => self.launch_url(&urls[0]),
            _ => self.url_picker = Some((urls, 0)),
        }
    }

    fn open_selected_document(&mut self) {
        let Some(message) = self.selected_message() else {
            return self.unavailable("Open is not available");
        };
        let wr::MessageContent::File(file) = &message.message else {
            return self.unavailable("Open is not available");
        };
        if !matches!(file.kind, wr::FileKind::Document) {
            return self.unavailable("Open is not available");
        }
        if !matches!(
            self.metadata.get(&message.info.id),
            Some(crate::app::Metadata::File(
                crate::app::FileMeta::Downloaded | crate::app::FileMeta::Loaded
            ))
        ) {
            return self.unavailable("Media is not downloaded");
        }
        let opener =
            match crate::media::document_opener_from_environment(Path::new(file.path.as_ref())) {
                Ok(opener) => opener,
                Err(_) => {
                    return self.unavailable(
                        "Set WPTUI_PDF_VIEWER or WPTUI_DOCUMENT_VIEWER to one executable name",
                    );
                }
            };
        match crate::media::plan_media_launch(
            Some(&self.media_path),
            Some(&opener),
            Some(Path::new(file.path.as_ref())),
        ) {
            Ok(Some(plan)) => {
                match crate::media::execute_launch(&plan, &mut crate::media::CommandLaunchExecutor)
                {
                    Ok(()) => {
                        self.action_notice = Some(crate::app::actions::ActionNotice::Unavailable(
                            "Document viewer started".into(),
                        ))
                    }
                    Err(error) => {
                        self.unavailable(&format!("Could not start document viewer: {error:?}"))
                    }
                }
            }
            Ok(None) | Err(_) => self.unavailable("Media launch is unavailable"),
        }
    }

    fn move_url_picker(&mut self, delta: isize) {
        if let Some((urls, selected)) = &mut self.url_picker {
            *selected = selected
                .saturating_add_signed(delta)
                .min(urls.len().saturating_sub(1));
        }
    }

    fn confirm_url_picker(&mut self) {
        let Some((urls, selected)) = self.url_picker.take() else {
            return;
        };
        if let Some(url) = urls.get(selected) {
            self.launch_url(url);
        } else {
            self.unavailable("Open is not available");
        }
    }

    fn launch_url(&mut self, url: &str) {
        let plan = crate::url::url_launch_plan(url);
        if self.url_opener.open(&plan).is_err() {
            self.unavailable("Could not open URL");
        }
    }

    fn open_file_picker(&mut self) {
        if self.file_picker.is_some() {
            return;
        }
        // If a workspace/project root is known, start there; otherwise fall
        // back to the user's home directory or the current directory.
        let start = std::env::var("PROJECT_ROOT")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| home_dir())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        match FilePickerState::open(&start) {
            Ok(picker) => self.file_picker = Some(picker),
            Err(_) => self.unavailable("Could not open the file picker"),
        }
    }

    fn move_file_picker(&mut self, delta: isize) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.move_selection(delta);
        }
    }

    fn file_picker_up(&mut self) -> bool {
        self.file_picker
            .as_mut()
            .is_some_and(|picker| picker.go_parent())
    }

    fn file_picker_down(&mut self) -> bool {
        self.file_picker
            .as_mut()
            .is_some_and(|picker| picker.descend_current())
    }

    fn file_picker_toggle(&mut self) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.toggle_selected();
        }
    }

    fn file_picker_enter_search(&mut self) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.enter_search();
        }
    }

    fn file_picker_end_search(&mut self) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.end_search();
        }
    }

    fn file_picker_backspace(&mut self) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.backspace_query();
        }
    }

    fn file_picker_character(&mut self, character: char) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.push_query(character);
        }
    }

    fn confirm_file_picker(&mut self) {
        if self.composer_blocked() {
            self.file_picker = None;
            return;
        }
        let Some(paths) = self
            .file_picker
            .as_ref()
            .map(FilePickerState::pending_paths)
        else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        self.file_picker = None;
        for path in paths {
            let kind = crate::clipboard::file_kind(&path);
            self.composer
                .queue_attachment(path.to_string_lossy().into_owned().into(), kind);
        }
        // Focus lands straight in the composer so the user can type on top of
        // the just-attached files instead of pressing `i` again.
        self.conversation_mode = ConversationMode::ComposerEditing;
        self.focus_pane = FocusPane::Conversation;
    }

    fn cancel_file_picker(&mut self) {
        self.file_picker = None;
        self.action_notice = Some(crate::app::actions::ActionNotice::Cancelled);
    }

    fn open_attachment_viewer(&mut self) {
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
            self.action_notice = Some(crate::app::actions::ActionNotice::Unsupported(
                "Sticker viewer is not supported".into(),
            ));
            return;
        }
        if !matches!(file.kind, wr::FileKind::Image | wr::FileKind::Video) {
            return self.unavailable("View is not available");
        }
        // In the Status section the viewer navigates only the opened
        // contact's statuses; everywhere else it uses the open chat's media.
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
                    Some(crate::app::events::ViewerAttachment {
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
            attachments.push(crate::app::events::ViewerAttachment {
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
        self.attachment_viewer = Some(crate::app::events::AttachmentViewerState::from_attachments(
            attachments,
            index,
        ));
        self.viewer_preview = None;
    }

    fn viewer_status(&self, message_id: &wr::MessageId) -> crate::app::events::ViewerStatus {
        match self.metadata.get(message_id) {
            Some(crate::app::Metadata::File(
                crate::app::FileMeta::Downloaded | crate::app::FileMeta::Loaded,
            )) => crate::app::events::ViewerStatus::Ready,
            Some(crate::app::Metadata::File(
                crate::app::FileMeta::Downloading | crate::app::FileMeta::Loading,
            )) => crate::app::events::ViewerStatus::Downloading,
            Some(crate::app::Metadata::File(
                crate::app::FileMeta::DownloadFailed | crate::app::FileMeta::LoadFailed,
            )) => crate::app::events::ViewerStatus::Failed,
            _ => crate::app::events::ViewerStatus::Missing,
        }
    }

    fn navigate_viewer(&mut self, delta: isize) {
        if let Some(viewer) = &mut self.attachment_viewer {
            viewer.navigate(delta);
            self.viewer_preview = None;
        }
    }

    fn plan_viewer_media_launch(&mut self) {
        let Some(viewer) = self.attachment_viewer.as_ref().cloned() else {
            return self.unavailable("Media is not downloaded");
        };
        if viewer.status != crate::app::events::ViewerStatus::Ready {
            return self.unavailable("Media is not downloaded");
        }
        self.plan_media_launch(&viewer.kind, Path::new(viewer.path.as_ref()));
    }

    fn plan_selected_media_launch(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let Some(message) = self.selected_message() else {
            return self.unavailable("Media is not downloaded");
        };
        let wr::MessageContent::File(file) = &message.message else {
            return self.unavailable("Media is not downloaded");
        };
        if !matches!(
            self.metadata.get(&message.info.id),
            Some(crate::app::Metadata::File(
                crate::app::FileMeta::Downloaded | crate::app::FileMeta::Loaded
            ))
        ) {
            return self.unavailable("Media is not downloaded");
        }
        let player = match crate::media::media_opener_from_environment(&file.kind) {
            Ok(player) => player,
            Err(_) => {
                return self.unavailable(
                    "Set WPTUI_IMAGE_VIEWER or WPTUI_MEDIA_PLAYER to one executable name",
                );
            }
        };
        match crate::media::plan_media_launch(
            Some(&self.media_path),
            Some(&player),
            Some(Path::new(file.path.as_ref())),
        ) {
            Ok(Some(plan)) => {
                match crate::media::execute_launch(&plan, &mut crate::media::CommandLaunchExecutor)
                {
                    Ok(()) => {
                        self.action_notice = Some(crate::app::actions::ActionNotice::Unavailable(
                            "Media player started".into(),
                        ))
                    }
                    Err(error) => {
                        self.unavailable(&format!("Could not start media player: {error:?}"))
                    }
                }
            }
            Ok(None) | Err(_) => self.unavailable("Media launch is unavailable"),
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
                        self.action_notice = Some(crate::app::actions::ActionNotice::Unavailable(
                            "Media player started".into(),
                        ))
                    }
                    Err(error) => {
                        self.unavailable(&format!("Could not start media player: {error:?}"))
                    }
                }
            }
            Ok(None) | Err(_) => self.unavailable("Media launch is unavailable"),
        }
    }

    fn copy_selected_text(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let text = self
            .selected_message()
            .and_then(|message| match &message.message {
                wr::MessageContent::Text(text) => Some(text.to_string()),
                _ => None,
            });
        self.action_notice = Some(match text {
            Some(text) if self.clipboard_writer.write_text(&text).is_ok() => {
                crate::app::actions::ActionNotice::CopiedText(text)
            }
            Some(_) => {
                crate::app::actions::ActionNotice::Unavailable("Could not copy message".into())
            }
            None => crate::app::actions::ActionNotice::Unavailable("Copy is not available".into()),
        });
    }

    fn open_share_picker(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let Some(message) = self.selected_message() else {
            return self.unavailable("Forward is not available");
        };
        if !forwardable_jid(&message.info.chat) {
            return self.unavailable("Forward is not available");
        }
        let contacts = self
            .contacts
            .keys()
            .filter(|jid| forwardable_jid(jid))
            .cloned()
            .collect();
        let labels = self
            .contacts
            .iter()
            .map(|(jid, name)| (jid.clone(), name.to_string()))
            .collect();
        let recency = self
            .chats
            .iter()
            .filter_map(|(jid, chat)| chat.last_message_time.map(|time| (jid.clone(), time)))
            .collect();
        self.share_picker = Some(crate::app::SharePicker::new(contacts, labels, recency));
    }

    fn move_share_picker(&mut self, delta: isize) {
        if let Some(picker) = self.share_picker.as_mut() {
            picker.move_selection(delta);
        }
    }

    fn toggle_share_recipient(&mut self) {
        let Some(picker) = self.share_picker.as_mut() else {
            return;
        };
        picker.toggle_selected();
    }

    fn share_search_backspace(&mut self) {
        if let Some(picker) = self.share_picker.as_mut() {
            picker.search_backspace();
        }
    }

    fn share_search_character(&mut self, character: char) {
        if let Some(picker) = self.share_picker.as_mut() {
            picker.search_character(character);
        }
    }

    fn confirm_share(&mut self) {
        let Some(picker) = self.share_picker.as_ref() else {
            return;
        };
        let destinations = picker.destinations();
        if destinations.is_empty() {
            return self.unavailable("Select at least one contact");
        }
        let Some(message) = self.selected_message().cloned() else {
            self.share_picker = None;
            return self.unavailable("Forward is not available");
        };
        self.share_picker = None;
        let report = self
            .message_forwarder
            .forward_message(&message, &destinations);
        self.action_notice = Some(crate::app::actions::ActionNotice::Forwarded {
            succeeded: report.succeeded,
            failed: report.failed,
            failure: report.failure,
        });
    }

    fn open_reaction_picker(&mut self) {
        if self.selected_message().is_none() {
            return self.unavailable("Reaction is not available");
        }
        self.reaction_picker = Some((
            COMMON_REACTIONS
                .iter()
                .map(|reaction| (*reaction).into())
                .collect(),
            0,
        ));
    }

    fn move_reaction(&mut self, delta: isize) {
        if let Some((reactions, selected)) = &mut self.reaction_picker {
            *selected = selected
                .saturating_add_signed(delta)
                .min(reactions.len() - 1);
        }
    }

    fn confirm_reaction(&mut self) {
        let Some((reactions, selected)) = self.reaction_picker.take() else {
            return;
        };
        let Some(reaction) = reactions.get(selected).cloned() else {
            return;
        };
        let Some(message) = self.selected_message().cloned() else {
            return self.unavailable("Reaction is not available");
        };
        if self
            .message_reactor
            .react_to_message(
                &message.info.chat,
                &message.info.sender,
                &message.info.id,
                &reaction,
            )
            .is_ok()
        {
            self.action_notice = Some(crate::app::actions::ActionNotice::Reacted);
        } else {
            self.unavailable("Could not react to message");
        }
    }

    fn delete_selected_message(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let Some(message) = self.selected_message().cloned() else {
            return self.unavailable("Delete is not available");
        };
        if !message.info.is_from_me {
            return self.action_notice = Some(crate::app::actions::ActionNotice::Unauthorized(
                "Only your messages can be changed".into(),
            ));
        }
        if !matches!(message.message, wr::MessageContent::Text(_)) {
            return self.unavailable("Delete is not available");
        }
        if self
            .message_revoker
            .revoke_message(&message.info.chat, &message.info.sender, &message.info.id)
            .is_ok()
        {
            self.record_local_message_delete(&message);
            self.action_notice = Some(crate::app::actions::ActionNotice::DeletedMessage);
        } else {
            self.unavailable("Could not delete message");
        }
    }

    fn start_message_edit(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let Some(message) = self.selected_message().cloned() else {
            return self.unavailable("Edit is not available");
        };
        if !message.info.is_from_me {
            return self.action_notice = Some(crate::app::actions::ActionNotice::Unauthorized(
                "Only your messages can be changed".into(),
            ));
        }
        if let wr::MessageContent::Text(text) = &message.message {
            self.composer.replace_text(text);
            self.edit_message = Some(message);
            self.conversation_mode = ConversationMode::EditingMessage;
        } else {
            self.unavailable("Edit is not available");
        }
    }

    pub fn cancel_message_edit(&mut self) {
        self.edit_message = None;
        self.composer.clear_text();
        self.conversation_mode = ConversationMode::MessageNavigation;
    }

    fn submit_message_edit(&mut self) {
        let replacement = self.composer.text();
        let replacement = replacement.trim();
        if replacement.is_empty() {
            return self.unavailable("Replacement cannot be empty");
        }
        let Some(message) = self.edit_message.as_ref().cloned() else {
            return self.unavailable("Edit is not available");
        };
        if self
            .message_editor
            .edit_message(&message.info.chat, &message.info.id, replacement)
            .is_ok()
        {
            self.record_local_message_edit(&message, replacement.into());
            self.cancel_message_edit();
            self.action_notice = Some(crate::app::actions::ActionNotice::EditedMessage);
        } else {
            self.unavailable("Could not edit message");
        }
    }

    fn reply_to_selected(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        if let Some(message) = self.selected_message().cloned() {
            self.composer.quote = Some(message);
            self.conversation_mode = ConversationMode::ComposerEditing;
        } else {
            self.unavailable("Reply is not available");
        }
    }

    /// Reply from a status: switches to the contact's private chat with
    /// the status quoted, so the answer lands in the inbox (the same flow
    /// as replying to a status in the WhatsApp mobile app).
    fn reply_to_status(&mut self) {
        let Some(message) = self.selected_message().cloned() else {
            return self.unavailable("Reply is not available");
        };
        let contact = message.info.sender.clone();
        self.selected_section = Section::Chats;
        self.open_chat = Some(contact.clone());
        self.sort_chat_messages(contact);
        self.message_list_state.reset();
        self.composer.quote = Some(message);
        self.conversation_mode = ConversationMode::ComposerEditing;
        self.focus_pane = FocusPane::Conversation;
    }

    /// Reacts to the selected status with a heart directly, skipping the
    /// reaction picker (WhatsApp only allows the heart on statuses).
    fn heart_selected_status(&mut self) {
        let Some(message) = self.selected_message().cloned() else {
            return self.unavailable("Reaction is not available");
        };
        if self
            .message_reactor
            .react_to_message_in_chat(
                &message.info.chat,
                &message.info.chat,
                &message.info.sender,
                &message.info.id,
                crate::app::actions::STATUS_REACTION,
            )
            .is_ok()
        {
            self.action_notice = Some(crate::app::actions::ActionNotice::Reacted);
        } else {
            self.unavailable("Could not react to message");
        }
    }

    fn open_message_menu(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let Some(message) = self.selected_message() else {
            return self.unavailable("Menu is not available");
        };
        let mut actions = vec![crate::app::actions::MessageMenuAction::Reply];
        if matches!(message.message, wr::MessageContent::Text(_)) {
            actions.insert(0, crate::app::actions::MessageMenuAction::CopyText);
        }
        if message.info.quote_id.is_some() {
            actions.push(crate::app::actions::MessageMenuAction::GoToReference);
        }
        if self.contacts.contains_key(&message.info.sender) {
            actions.push(crate::app::actions::MessageMenuAction::SenderDetails);
        }
        if self
            .reactions
            .get(&message.info.id)
            .is_some_and(|items| !items.is_empty())
        {
            actions.push(crate::app::actions::MessageMenuAction::ReactedUsers);
        }
        self.message_menu = Some((actions, 0));
    }

    fn move_menu(&mut self, delta: isize) {
        if let Some((actions, selected)) = &mut self.message_menu {
            *selected = selected.saturating_add_signed(delta).min(actions.len() - 1);
        }
    }

    fn confirm_message_menu(&mut self) {
        let action = self
            .message_menu
            .take()
            .and_then(|(actions, selected)| actions.get(selected).copied());
        match action {
            Some(crate::app::actions::MessageMenuAction::CopyText) => self.copy_selected_text(),
            Some(crate::app::actions::MessageMenuAction::Reply) => self.reply_to_selected(),
            Some(crate::app::actions::MessageMenuAction::GoToReference)
                if !self.follow_selected_reference() =>
            {
                self.unavailable("Reference is not available")
            }
            Some(crate::app::actions::MessageMenuAction::GoToReference) | None => {}
            Some(crate::app::actions::MessageMenuAction::SenderDetails) => {
                if let Some(message) = self.selected_message()
                    && let Some(name) = self.contacts.get(&message.info.sender)
                {
                    self.action_notice = Some(crate::app::actions::ActionNotice::SenderDetails(
                        name.to_string(),
                    ));
                }
            }
            Some(crate::app::actions::MessageMenuAction::ReactedUsers) => {
                if let Some(message) = self.selected_message() {
                    let users = self
                        .reactions
                        .get(&message.info.id)
                        .into_iter()
                        .flat_map(|items| items.keys())
                        .map(|jid| self.contact_name(jid).to_string())
                        .collect();
                    self.action_notice =
                        Some(crate::app::actions::ActionNotice::ReactedUsers(users));
                }
            }
        }
    }

    fn dispatch_composer_action(&mut self, action: ComposerAction) {
        if self.composer_blocked() {
            return;
        }
        if self.conversation_mode == ConversationMode::EditingMessage
            && matches!(action, ComposerAction::Submit)
        {
            return self.submit_message_edit();
        }
        match action {
            ComposerAction::StartEdit => {
                // InsertMode is now the canonical way; StartEdit is unused.
            }
            ComposerAction::Paste => {
                let paste = self.clipboard_reader.read_paste();
                if let Err(error) =
                    apply_clipboard_paste(&mut self.composer, &self.media_path, paste)
                {
                    self.unavailable(&format!("Could not paste clipboard content: {error:?}"));
                }
            }
            action => match self.composer.apply(action) {
                ComposerOutcome::Idle => {}
                ComposerOutcome::Submit { messages, quote } => {
                    if let Some(chat) = self.open_chat() {
                        for message in messages {
                            wr::send_message(&chat, &message, quote.as_ref());
                        }
                    }
                }
            },
        }
    }
}

fn forwardable_jid(jid: &wr::JID) -> bool {
    !jid.0.ends_with("@broadcast") && !jid.0.ends_with("@newsletter")
}

fn is_toggle_logs_key(key: &Key) -> bool {
    matches!(key.code, KeyCode::Char('l' | 'L'))
        && key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT
}

fn is_attach_file_key(key: &Key) -> bool {
    key == &Key::ctrl('o')
}

/// Best-effort home directory lookup for the file picker's default start.
/// Falls back to the current directory when no home is available (e.g. in a
/// headless or minimal environment).
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_dir())
}

/// Actions allowed while a contact's status view is focused (read-only).
/// Chat-specific actions are rejected so nothing ever targets the
/// `status@broadcast` chat from the status pane.
fn status_view_allows(action: &AppAction) -> bool {
    matches!(
        action,
        AppAction::Quit
            | AppAction::Logout
            | AppAction::ToggleLogs
            | AppAction::ToggleSectionRail
            | AppAction::ToggleChatList
            | AppAction::FocusNext
            | AppAction::FocusPrevious
            | AppAction::SelectNext
            | AppAction::SelectPrevious
            | AppAction::JumpTop
            | AppAction::JumpBottom
            | AppAction::HalfPageDown
            | AppAction::HalfPageUp
            | AppAction::CopyMessage
            | AppAction::ReplyMessage
            | AppAction::ReactMessage
            | AppAction::DownloadMessage
            | AppAction::ViewMessage
            | AppAction::ViewerNext
            | AppAction::ViewerPrevious
            | AppAction::ViewerZoomIn
            | AppAction::ViewerZoomOut
            | AppAction::CloseAttachmentViewer
            | AppAction::ViewerOpenExternal
            | AppAction::CloseStatusPane
    )
}

fn log_level_for_logs(show_logs: bool) -> tui_logger::LevelFilter {
    if show_logs {
        tui_logger::LevelFilter::Info
    } else {
        tui_logger::LevelFilter::Warn
    }
}

#[cfg(test)]
mod tests {
    use super::{log_level_for_logs, status_view_allows};
    use tui_logger::LevelFilter;

    #[test]
    fn log_panel_uses_info_and_restores_warn_when_closed() {
        assert_eq!(log_level_for_logs(true), LevelFilter::Info);
        assert_eq!(log_level_for_logs(false), LevelFilter::Warn);
    }

    #[test]
    fn status_view_allows_status_interactions_and_media_actions() {
        use crate::app::actions::AppAction;

        for allowed in [
            AppAction::Quit,
            AppAction::Logout,
            AppAction::ToggleLogs,
            AppAction::ToggleSectionRail,
            AppAction::ToggleChatList,
            AppAction::FocusNext,
            AppAction::FocusPrevious,
            AppAction::SelectNext,
            AppAction::SelectPrevious,
            AppAction::JumpTop,
            AppAction::JumpBottom,
            AppAction::HalfPageDown,
            AppAction::HalfPageUp,
            AppAction::CopyMessage,
            AppAction::ReactMessage,
            AppAction::ReplyMessage,
            AppAction::DownloadMessage,
            AppAction::ViewMessage,
            AppAction::ViewerNext,
            AppAction::ViewerPrevious,
            AppAction::ViewerZoomIn,
            AppAction::ViewerZoomOut,
            AppAction::CloseAttachmentViewer,
            AppAction::ViewerOpenExternal,
            AppAction::CloseStatusPane,
        ] {
            assert!(status_view_allows(&allowed), "{allowed:?} must be allowed");
        }

        for blocked in [
            AppAction::OpenChat,
            AppAction::OpenMessage,
            AppAction::InsertMode,
            AppAction::DeleteMessage,
            AppAction::EditMessage,
            AppAction::OpenMessageMenu,
            AppAction::GoToReference,
        ] {
            assert!(!status_view_allows(&blocked), "{blocked:?} must be blocked");
        }
    }
}

pub fn apply_clipboard_paste(
    composer: &mut Composer<'_>,
    media_path: &Path,
    paste: Result<ClipboardPaste, ClipboardError>,
) -> Result<(), ClipboardError> {
    match paste? {
        ClipboardPaste::Text(text) => composer.insert_text(&text),
        ClipboardPaste::Paths(paths) => {
            for path in paths {
                let kind = clipboard::file_kind(&path);
                composer.queue_attachment(path.to_string_lossy().into_owned().into(), kind);
            }
        }
        ClipboardPaste::Png(png) => {
            let path = clipboard::persist_png(media_path, &png)?;
            composer.queue_attachment(
                path.to_string_lossy().into_owned().into(),
                whatsrust::FileKind::Image,
            );
        }
    }
    Ok(())
}

fn message_urls(message: &wr::Message) -> Vec<String> {
    let text = match &message.message {
        wr::MessageContent::Text(text) => Some(text.as_ref()),
        wr::MessageContent::File(file) => file.caption.as_deref(),
    };
    text.map(crate::url::extract_openable_urls)
        .unwrap_or_default()
}

pub fn composer_action_for_editing_key(key: &Key) -> ComposerAction {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, KeyModifiers::NONE) => ComposerAction::Submit,
        (KeyCode::Char('v'), KeyModifiers::CONTROL) => ComposerAction::Paste,
        _ => ComposerAction::Edit(key.clone()),
    }
}
