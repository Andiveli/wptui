use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::app::App;
use crate::app::actions::{
    AppAction, ComposerAction, ConversationMode, FocusPane, Section, SequenceResolution,
    focus_after, focus_after_visibility_change,
};
use crate::app::composer::ComposerOutcome;
pub use crate::app::composer_input_mapping::composer_action_for_editing_key;
pub use crate::app::composer_input_paste::apply_clipboard_paste;
use crate::app::input_mapping::{
    attachment_viewer_action, file_picker_navigation_action, file_picker_search_action,
    message_menu_action, reaction_picker_action, share_picker_action, url_picker_action,
};
use crate::app::share_picker::is_forwardable_recipient;
use crate::app::status_input::status_view_allows;
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
            action @ (AppAction::ReactionPrev
            | AppAction::ReactionNext
            | AppAction::ConfirmReaction
            | AppAction::CancelReaction) => self.dispatch_reaction_picker_action(action),
            AppAction::UrlPickerPrevious => self.move_url_picker(-1),
            AppAction::UrlPickerNext => self.move_url_picker(1),
            AppAction::ConfirmUrlPicker => self.confirm_url_picker(),
            AppAction::CancelUrlPicker => {
                self.url_picker = None;
                self.action_notice = Some(crate::app::actions::ActionNotice::Cancelled);
            }
            action @ (AppAction::AttachFile
            | AppAction::FilePickerPrevious
            | AppAction::FilePickerNext
            | AppAction::FilePickerParent
            | AppAction::FilePickerDescend
            | AppAction::FilePickerToggle
            | AppAction::FilePickerConfirm
            | AppAction::FilePickerEnterSearch
            | AppAction::FilePickerEndSearch
            | AppAction::FilePickerBackspace
            | AppAction::FilePickerCharacter(_)
            | AppAction::CancelFilePicker) => self.dispatch_file_picker_action(action),
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

    fn open_share_picker(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let Some(message) = self.selected_message() else {
            return self.unavailable("Forward is not available");
        };
        if !is_forwardable_recipient(&message.info.chat) {
            return self.unavailable("Forward is not available");
        }
        let contacts = self
            .contacts
            .keys()
            .filter(|jid| is_forwardable_recipient(jid))
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

fn is_toggle_logs_key(key: &Key) -> bool {
    matches!(key.code, KeyCode::Char('l' | 'L'))
        && key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT
}

fn is_attach_file_key(key: &Key) -> bool {
    key == &Key::ctrl('o')
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
    use super::log_level_for_logs;
    use tui_logger::LevelFilter;

    #[test]
    fn log_panel_uses_info_and_restores_warn_when_closed() {
        assert_eq!(log_level_for_logs(true), LevelFilter::Info);
        assert_eq!(log_level_for_logs(false), LevelFilter::Warn);
    }
}
