use crate::app::App;
use crate::app::actions::AppAction;

impl App<'_> {
    pub(crate) fn dispatch_picker_viewer_action(
        &mut self,
        action: AppAction,
    ) -> Result<(), AppAction> {
        match action {
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
                self.focus_pane = crate::app::actions::FocusPane::ChatList
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
            other => return Err(other),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::FocusPane;
    use crate::app::test_support::TestApp;

    #[test]
    fn viewer_zoom_actions_are_handled_by_the_picker_viewer_family() {
        let mut app = TestApp::new();

        app.dispatch_action(AppAction::ViewerZoomIn);
        assert_eq!(app.viewer_zoom, 125);
        app.dispatch_action(AppAction::ViewerZoomOut);
        assert_eq!(app.viewer_zoom, 100);
    }

    #[test]
    fn closing_status_pane_returns_focus_to_chat_list() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;

        app.dispatch_action(AppAction::CloseStatusPane);

        assert_eq!(app.focus_pane, FocusPane::ChatList);
    }
}
