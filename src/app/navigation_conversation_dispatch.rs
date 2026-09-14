use crate::app::App;
use crate::app::actions::{AppAction, ConversationMode, FocusPane, Section};

impl App<'_> {
    pub(crate) fn dispatch_navigation_conversation_action(
        &mut self,
        action: AppAction,
    ) -> Result<(), AppAction> {
        match action {
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
            action @ (AppAction::Quit
            | AppAction::Logout
            | AppAction::ToggleLogs
            | AppAction::ToggleSectionRail
            | AppAction::ToggleChatList
            | AppAction::FocusPane(_)
            | AppAction::OpenContextualActions
            | AppAction::ToggleShortcutPopup
            | AppAction::ToggleComposerDirection
            | AppAction::PlannedLeaderAction(_)
            | AppAction::ViewMessage
            | AppAction::DownloadMessage
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
            | AppAction::ReactionPrev
            | AppAction::ReactionNext
            | AppAction::ConfirmReaction
            | AppAction::CancelReaction
            | AppAction::SharePickerPrevious
            | AppAction::SharePickerNext
            | AppAction::ToggleShareRecipient
            | AppAction::ConfirmShare
            | AppAction::CancelShare
            | AppAction::ShareSearchBackspace
            | AppAction::ShareSearchCharacter(_)
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
            | AppAction::CancelFilePicker) => return Err(action),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::TestApp;

    #[test]
    fn insert_mode_is_handled_by_navigation_conversation_dispatch() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;

        app.dispatch_action(AppAction::InsertMode);

        assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
    }

    #[test]
    fn lifecycle_actions_are_returned_to_their_dispatcher() {
        let mut app = TestApp::new();

        assert_eq!(
            app.dispatch_navigation_conversation_action(AppAction::ToggleLogs),
            Err(AppAction::ToggleLogs)
        );
    }
}
