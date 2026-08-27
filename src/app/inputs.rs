use ratatui::crossterm::event::Event;

use crate::app::App;
use crate::app::actions::{
    AppAction, ConversationMode, FocusPane, Section, focus_after,
};
pub use crate::app::composer_input_mapping::composer_action_for_editing_key;
pub use crate::app::composer_input_paste::apply_clipboard_paste;
use crate::app::input_router::{InputRoute, ModalContext, route_context_key, route_modal_key};
use crate::app::leader_menu::{LeaderMenuContext, build_leader_menu};
use crate::app::log_toggle::is_toggle_logs_key;
use crate::app::status_input::status_view_allows;
use crate::app::terminal_input_translation::translate_terminal_event;
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
        if let Some(key) = translate_terminal_event(&event) {
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
                if matches!(
                    route_modal_key(&key, ModalContext::ShortcutPopup),
                    InputRoute::DismissLeader
                ) {
                    self.shortcut_popup = false;
                }
            } else if self.leader_menu.is_some() {
                let route = route_modal_key(
                    &key,
                    ModalContext::Leader(&self.leader_menu.as_ref().unwrap().0),
                );
                match route {
                    InputRoute::DismissLeader => {
                        self.leader_menu = None;
                        self.kh.key_buffer.clear();
                        self.kh.key_sequence_active = false;
                    }
                    InputRoute::ActivateLeader => {
                        if key == Key::k(KeyCode::Enter) {
                            self.activate_leader_selection();
                        } else {
                            self.activate_leader_key(key);
                        }
                    }
                    InputRoute::MoveLeader(delta) => self.move_leader_menu(delta),
                    _ => {}
                }
            } else if self.contextual_menu.is_some() {
                let route = route_modal_key(
                    &key,
                    ModalContext::Contextual(&self.contextual_menu.as_ref().unwrap().0),
                );
                match route {
                    InputRoute::DismissContextual => self.contextual_menu = None,
                    InputRoute::ActivateContextualSelection => self.activate_contextual_action(),
                    InputRoute::ActivateContextualShortcut(action) => {
                        self.activate_contextual_shortcut(action)
                    }
                    InputRoute::MoveContextual(delta) => self.move_contextual_menu(delta),
                    _ => {}
                }
            } else if self.attachment_viewer.is_some() {
                if let InputRoute::Action(action) =
                    route_modal_key(&key, ModalContext::AttachmentViewer)
                {
                    self.dispatch_action(action);
                }
            } else if self.url_picker.is_some() {
                if let InputRoute::Action(action) = route_modal_key(&key, ModalContext::UrlPicker) {
                    self.dispatch_action(action);
                }
            } else if self.file_picker.is_some() && self.file_picker.as_ref().unwrap().searching {
                // Search mode: the keyboard buffer owns every key except the
                // explicit exits, so `h/j/k/l` build the filter instead of
                // moving. Esc leaves search mode back to navigation.
                if let InputRoute::Action(action) =
                    route_modal_key(&key, ModalContext::FilePickerSearch)
                {
                    self.dispatch_action(action);
                }
            } else if self.file_picker.is_some() {
                // Navigation mode. `h/l` move across the tree, `Enter` commits
                // (files under the cursor, or every `Space`-selected file).
                if let InputRoute::Action(action) =
                    route_modal_key(&key, ModalContext::FilePickerNavigation)
                {
                    self.dispatch_action(action);
                }
            } else if self.share_picker.is_some() {
                if let InputRoute::Action(action) = route_modal_key(&key, ModalContext::SharePicker)
                {
                    self.dispatch_action(action);
                }
            } else if self.reaction_picker.is_some() {
                if let InputRoute::Action(action) =
                    route_modal_key(&key, ModalContext::ReactionPicker)
                {
                    self.dispatch_action(action);
                }
            } else if self.message_menu.is_some() {
                if let InputRoute::Action(action) = route_modal_key(&key, ModalContext::MessageMenu)
                {
                    self.dispatch_action(action);
                }
            } else if self.focus_pane == FocusPane::Conversation
                && self.selected_section == Section::Status
            {
                // Read-only status view: Esc returns to the contact list and
                // Enter opens the fullscreen viewer on a media status.
                if let Some(action) =
                    route_context_key(&key, self.focus_pane, self.selected_section, false, false)
                {
                    self.dispatch_action(action);
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
            } else if let Some(action) = route_context_key(
                &key,
                self.focus_pane,
                self.selected_section,
                self.community_detail.is_some(),
                self.rail_on_logout,
            ) {
                self.dispatch_action(action);
            } else if key == Key::k(KeyCode::Enter) {
                if self.focus_pane == FocusPane::ChatList
                    && self.community_detail.is_some()
                    && key == Key::k(KeyCode::Enter)
                {
                    self.dispatch_action(AppAction::OpenChat);
                } else if self.focus_pane == FocusPane::SectionRail && self.rail_on_logout {
                    self.begin_logout_confirmation();
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
        if self.selected_message_is_informational()
            && matches!(
                action,
                AppAction::OpenMessageMenu
                    | AppAction::CopyMessage
                    | AppAction::ReplyMessage
                    | AppAction::ReplyPrivately
                    | AppAction::ShareMessage
                    | AppAction::ReactMessage
                    | AppAction::DeleteMessage
                    | AppAction::EditMessage
                    | AppAction::OpenMessage
                    | AppAction::DownloadMessage
                    | AppAction::ViewMessage
                    | AppAction::GoToReference
            )
        {
            self.unavailable("Action is not available for this informational item");
            return;
        }

        match action {
            action @ (AppAction::DownloadMessage
            | AppAction::ViewMessage
            | AppAction::ViewerNext
            | AppAction::ViewerPrevious
            | AppAction::ViewerZoomIn
            | AppAction::ViewerZoomOut
            | AppAction::ViewerOpenExternal
            | AppAction::CloseAttachmentViewer
            | AppAction::CloseStatusPane
            | AppAction::OpenMessageMenu
            | AppAction::MenuNext
            | AppAction::MenuPrevious
            | AppAction::ConfirmMessageMenu
            | AppAction::CancelMessageMenu
            | AppAction::SharePickerPrevious
            | AppAction::SharePickerNext
            | AppAction::ToggleShareRecipient
            | AppAction::ConfirmShare
            | AppAction::CancelShare
            | AppAction::ShareSearchBackspace
            | AppAction::ShareSearchCharacter(_)
            | AppAction::ReactionPrev
            | AppAction::ReactionNext
            | AppAction::ConfirmReaction
            | AppAction::CancelReaction
            | AppAction::UrlPickerPrevious
            | AppAction::UrlPickerNext
            | AppAction::ConfirmUrlPicker
            | AppAction::CancelUrlPicker
            | AppAction::AttachFile
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
            | AppAction::CancelFilePicker) => {
                self.dispatch_picker_viewer_action(action)
                    .expect("picker/viewer action family must be handled by its dispatcher");
            }
            action @ (AppAction::Quit
            | AppAction::Logout
            | AppAction::ToggleLogs
            | AppAction::ToggleSectionRail
            | AppAction::ToggleChatList
            | AppAction::FocusPane(_)
            | AppAction::ToggleShortcutPopup
            | AppAction::ToggleComposerDirection
            | AppAction::PlannedLeaderAction(_)) => {
                self.dispatch_lifecycle_settings_action(action)
                    .expect("lifecycle/settings action family must be handled by its dispatcher");
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
    use crate::app::contextual_actions::ContextualAction;
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
                    .filter(|row| {
                        row.action_token != crate::app::contextual_actions::ContextualAction::Quit
                    })
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
                    .filter(|row| {
                        row.action_token != crate::app::contextual_actions::ContextualAction::Quit
                    })
                    .all(|row| row.row_style == crate::app::contextual_actions::RowStyle::Disabled),
                "character: {character:?}"
            );
        }
    }

    #[test]
    fn space_then_q_quits_through_terminal_input_routing() {
        let mut app = TestApp::new();

        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Char(' '),
            KeyModifiers::NONE,
        )));
        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Char('q'),
            KeyModifiers::NONE,
        )));

        assert!(app.should_quit);
        assert!(app.leader_menu.is_none());
    }

    #[test]
    fn contextual_q_quits_through_terminal_input_routing() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.selected_section = Section::Chats;

        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(app.contextual_menu.is_some());

        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Char('q'),
            KeyModifiers::NONE,
        )));

        assert!(app.should_quit);
        assert!(app.contextual_menu.is_none());
    }

    #[test]
    fn enter_activates_selected_quit_row_in_leader_menu() {
        let mut app = TestApp::new();

        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Char(' '),
            KeyModifiers::NONE,
        )));
        let quit_index = app
            .leader_menu
            .as_ref()
            .unwrap()
            .0
            .iter()
            .position(|row| row.action_token == AppAction::Quit)
            .unwrap();
        for _ in 0..quit_index {
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Down,
                KeyModifiers::NONE,
            )));
        }
        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert!(app.should_quit);
        assert!(app.leader_menu.is_none());
    }

    #[test]
    fn enter_activates_selected_quit_row_in_contextual_menu() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.selected_section = Section::Chats;

        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Enter,
            KeyModifiers::NONE,
        )));
        let quit_index = app
            .contextual_menu
            .as_ref()
            .unwrap()
            .0
            .iter()
            .position(|row| row.action_token == ContextualAction::Quit)
            .unwrap();
        for _ in 0..quit_index {
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Down,
                KeyModifiers::NONE,
            )));
        }
        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert!(app.should_quit);
        assert!(app.contextual_menu.is_none());
    }
}
