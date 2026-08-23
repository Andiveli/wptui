use ratatui::crossterm::event::{Event, KeyEventKind};

use crate::app::App;
use crate::app::actions::{
    AppAction, ConversationMode, FocusPane, Section, focus_after, focus_after_visibility_change,
};
pub use crate::app::composer_input_mapping::composer_action_for_editing_key;
pub use crate::app::composer_input_paste::apply_clipboard_paste;
use crate::app::contextual_routing::route_contextual_key;
use crate::app::input_mapping::{
    attachment_viewer_action, file_picker_navigation_action, file_picker_search_action,
    message_menu_action, reaction_picker_action, share_picker_action, url_picker_action,
};
use crate::app::leader_menu::{LeaderMenuContext, build_leader_menu};
use crate::app::log_toggle::is_toggle_logs_key;
use crate::app::status_input::status_view_allows;
use crate::input_key::{Key, KeyCode};
use crate::keybindings::SequenceResolution;
use crate::keybindings::matches_leader_binding;

impl App<'_> {
    pub(crate) fn open_leader_menu(&mut self) {
        self.leader_menu = Some((
            build_leader_menu(LeaderMenuContext {
                composer_direction: self.composer_direction,
            }),
            0,
        ));
    }

    pub(crate) fn move_leader_menu(&mut self, delta: isize) {
        if let Some((rows, selected)) = &mut self.leader_menu {
            *selected = selected
                .saturating_add_signed(delta)
                .min(rows.len().saturating_sub(1));
        }
    }

    pub(crate) fn activate_leader_key(&mut self, key: Key) {
        let Some((rows, _)) = &self.leader_menu else {
            return;
        };
        let Some(row) = rows
            .iter()
            .find(|row| matches_leader_binding(&key, &row.key))
            .cloned()
        else {
            return;
        };
        if row.row_style != crate::app::contextual_actions::RowStyle::Enabled {
            return;
        }
        self.leader_menu = None;
        self.kh.key_buffer.clear();
        self.kh.key_sequence_active = false;
        self.dispatch_action(row.action_token);
    }

    pub(crate) fn activate_leader_selection(&mut self) {
        let Some((rows, selected)) = &self.leader_menu else {
            return;
        };
        let Some(row) = rows.get(*selected).cloned() else {
            return;
        };
        self.activate_leader_key(row.key);
    }

    pub fn on_terminal_event(&mut self, event: Event) {
        if let Event::Key(key_event) = event
            && key_event.kind == KeyEventKind::Press
        {
            let key = Key {
                code: match key_event.code {
                    ratatui::crossterm::event::KeyCode::Backspace => KeyCode::Backspace,
                    ratatui::crossterm::event::KeyCode::Enter => KeyCode::Enter,
                    ratatui::crossterm::event::KeyCode::Esc => KeyCode::Esc,
                    ratatui::crossterm::event::KeyCode::Left => KeyCode::Left,
                    ratatui::crossterm::event::KeyCode::Right => KeyCode::Right,
                    ratatui::crossterm::event::KeyCode::Up => KeyCode::Up,
                    ratatui::crossterm::event::KeyCode::Down => KeyCode::Down,
                    ratatui::crossterm::event::KeyCode::Tab => KeyCode::Tab,
                    ratatui::crossterm::event::KeyCode::Char(c) => KeyCode::Char(c),
                    _ => return,
                },
                modifiers: {
                    let mut modifiers = crate::input_key::KeyModifiers::NONE;
                    if key_event
                        .modifiers
                        .contains(ratatui::crossterm::event::KeyModifiers::SHIFT)
                    {
                        modifiers = modifiers | crate::input_key::KeyModifiers::SHIFT;
                    }
                    if key_event
                        .modifiers
                        .contains(ratatui::crossterm::event::KeyModifiers::CONTROL)
                    {
                        modifiers = modifiers | crate::input_key::KeyModifiers::CONTROL;
                    }
                    if key_event
                        .modifiers
                        .contains(ratatui::crossterm::event::KeyModifiers::ALT)
                    {
                        modifiers = modifiers | crate::input_key::KeyModifiers::ALT;
                    }
                    modifiers
                },
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
            } else if self.handle_composer_input(key.clone()) {
            } else if self.shortcut_popup {
                if key == Key::k(KeyCode::Esc) || key == Key::c('?') {
                    self.shortcut_popup = false;
                }
            } else if self.leader_menu.is_some() {
                if key == Key::k(KeyCode::Esc) {
                    self.leader_menu = None;
                    self.kh.key_buffer.clear();
                    self.kh.key_sequence_active = false;
                } else if key == Key::k(KeyCode::Enter) {
                    self.activate_leader_selection();
                } else if key == Key::k(KeyCode::Char('j')) || key == Key::k(KeyCode::Down) {
                    self.move_leader_menu(1);
                } else if key == Key::k(KeyCode::Char('k')) || key == Key::k(KeyCode::Up) {
                    self.move_leader_menu(-1);
                } else {
                    self.activate_leader_key(key);
                }
            } else if self.contextual_menu.is_some() {
                if key == Key::k(KeyCode::Esc) {
                    self.contextual_menu = None;
                } else if key == Key::k(KeyCode::Enter) {
                    self.activate_contextual_action();
                } else if key == Key::k(KeyCode::Char('j')) || key == Key::k(KeyCode::Down) {
                    self.move_contextual_menu(1);
                } else if key == Key::k(KeyCode::Char('k')) || key == Key::k(KeyCode::Up) {
                    self.move_contextual_menu(-1);
                } else if let Some(action) = self
                    .contextual_menu
                    .as_ref()
                    .and_then(|(rows, _)| route_contextual_key(rows, &key))
                {
                    self.activate_contextual_shortcut(action);
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
                        SequenceResolution::Partial => self.open_leader_menu_if_space(),
                        SequenceResolution::Cancelled => {}
                    }
                }
            } else if self.handle_chat_search_input(key.clone()) {
            } else if self.focus_pane == FocusPane::ChatList
                && self.community_detail.is_some()
                && key == Key::k(KeyCode::Esc)
            {
                self.close_community_detail();
            } else if self.focus_pane == FocusPane::SectionRail
                && self.rail_on_logout
                && key == Key::k(KeyCode::Enter)
            {
                self.begin_logout_confirmation();
            } else if key == Key::k(KeyCode::Enter) {
                match self.focus_pane {
                    FocusPane::SectionRail => {
                        self.dispatch_action(AppAction::FocusPane(FocusPane::ChatList))
                    }
                    FocusPane::ChatList => self.dispatch_action(AppAction::OpenChat),
                    FocusPane::Conversation => {
                        self.dispatch_action(AppAction::OpenContextualActions)
                    }
                }
            } else {
                match self.kh.resolve(key.clone()) {
                    SequenceResolution::Complete(action) => self.dispatch_action(action),
                    SequenceResolution::Partial => self.open_leader_menu_if_space(),
                    SequenceResolution::Cancelled => self.handle_unbound_key(key),
                }
            }
        }
    }

    fn activate_contextual_shortcut(
        &mut self,
        action: crate::app::contextual_actions::ContextualAction,
    ) {
        let Some((rows, _)) = &self.contextual_menu else {
            return;
        };
        if let Some(index) = rows.iter().position(|row| row.action_token == action) {
            if let Some(menu) = &mut self.contextual_menu {
                menu.1 = index;
            }
            self.activate_contextual_action();
        }
    }

    fn handle_unbound_key(&mut self, key: Key) {
        // Chat search is a Chats-section feature; the status list has its
        // own single-pane navigation.
        self.start_chat_search(&key);
    }

    fn open_leader_menu_if_space(&mut self) {
        if self.kh.buffered_keys() == [Key::c(' ')] {
            self.open_leader_menu();
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
            AppAction::ToggleLogs => self.toggle_logs(),
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
            AppAction::FocusPane(pane) => {
                let visible = match pane {
                    FocusPane::SectionRail => self.pane_visibility.section_rail,
                    FocusPane::ChatList => self.pane_visibility.chat_list,
                    FocusPane::Conversation => true,
                };
                if visible {
                    self.focus_pane = pane;
                }
            }
            AppAction::OpenContextualActions => self.open_contextual_actions(),
            AppAction::ToggleShortcutPopup => self.shortcut_popup = !self.shortcut_popup,
            AppAction::ToggleComposerDirection => self.toggle_composer_direction(),
            AppAction::PlannedLeaderAction(label) => {
                self.unavailable(&format!("{label}: not implemented"))
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
                } else if self.selected_section == Section::Communities
                    && self.community_detail.is_none()
                {
                    match self
                        .community_navigation_rows()
                        .into_iter()
                        .filter(|row| !matches!(row, crate::app::CommunityNavigationRow::Separator))
                        .nth(self.chat_list_state.selected().unwrap_or_default())
                    {
                        Some(crate::app::CommunityNavigationRow::Root(jid))
                        | Some(crate::app::CommunityNavigationRow::ViewAll(jid)) => {
                            self.open_community_detail(jid);
                        }
                        Some(crate::app::CommunityNavigationRow::Group(jid))
                        | Some(crate::app::CommunityNavigationRow::Announcement(jid)) => {
                            self.open_chat_by_jid(jid);
                            self.focus_pane = FocusPane::Conversation;
                        }
                        Some(crate::app::CommunityNavigationRow::Separator) => {}
                        None => {}
                    }
                } else if let Some(root) = self.selected_community_contact() {
                    let unread = self
                        .communities
                        .iter()
                        .find(|node| node.jid == root)
                        .map(|node| {
                            node.linked_groups
                                .iter()
                                .filter(|jid| self.pending_new_messages(jid) > 0)
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if unread.len() == 1 {
                        self.open_chat_by_jid(unread[0].clone());
                        self.focus_pane = FocusPane::Conversation;
                        if self.message_count() > 0 {
                            self.message_list_state.select(Some(0));
                        }
                    } else {
                        self.open_community_detail(root);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::Section;
    use crate::app::test_support::TestApp;
    use ratatui::crossterm::event::{Event, KeyCode as TerminalKeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn status_enter_keeps_status_precedence_over_generic_pane_enter() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.selected_section = Section::Status;

        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert!(app.contextual_menu.is_none());
    }

    #[test]
    fn status_view_reaches_composer_direction_toggle_through_leader_settings() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.selected_section = Section::Status;

        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Char(' '),
            KeyModifiers::NONE,
        )));
        assert!(app.leader_menu.is_some());

        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Char('s'),
            KeyModifiers::NONE,
        )));

        assert_eq!(
            app.composer_direction,
            crate::app::preferences::ComposerDirection::Rtl
        );
        assert!(app.leader_menu.is_none());
    }

    #[test]
    fn space_a_opens_contextual_menu_in_section_rail_with_only_disabled_children() {
        for character in ['A', 'a'] {
            let mut app = TestApp::new();
            app.focus_pane = FocusPane::SectionRail;
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char(' '),
                KeyModifiers::NONE,
            )));
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char(character),
                KeyModifiers::SHIFT,
            )));
            assert!(app.contextual_menu.is_some(), "character: {character:?}");
            assert!(
                app.contextual_menu
                    .as_ref()
                    .unwrap()
                    .0
                    .iter()
                    .all(|row| row.row_style == crate::app::contextual_actions::RowStyle::Disabled),
                "character: {character:?}"
            );
        }
    }

    #[test]
    fn space_a_opens_contextual_menu_in_chat_list_with_only_disabled_children() {
        for character in ['A', 'a'] {
            let mut app = TestApp::new();
            app.focus_pane = FocusPane::ChatList;
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char(' '),
                KeyModifiers::NONE,
            )));
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char(character),
                KeyModifiers::SHIFT,
            )));
            assert!(app.contextual_menu.is_some(), "character: {character:?}");
            assert!(
                app.contextual_menu
                    .as_ref()
                    .unwrap()
                    .0
                    .iter()
                    .all(|row| row.row_style == crate::app::contextual_actions::RowStyle::Disabled),
                "character: {character:?}"
            );
        }
    }
}
