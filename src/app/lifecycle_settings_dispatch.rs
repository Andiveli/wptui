use crate::app::App;
use crate::app::actions::{AppAction, FocusPane, focus_after_visibility_change};

impl App<'_> {
    pub(crate) fn dispatch_lifecycle_settings_action(
        &mut self,
        action: AppAction,
    ) -> Result<(), AppAction> {
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
            other => return Err(other),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::TestApp;

    #[test]
    fn hidden_pane_toggle_moves_focus_to_visible_pane() {
        let mut app = TestApp::new();
        app.pane_visibility.section_rail = false;
        app.focus_pane = FocusPane::ChatList;

        app.dispatch_action(AppAction::ToggleChatList);

        assert!(!app.pane_visibility.chat_list);
        assert_eq!(app.focus_pane, FocusPane::Conversation);
    }

    #[test]
    fn shortcut_popup_toggle_is_handled_by_lifecycle_settings_family() {
        let mut app = TestApp::new();

        app.dispatch_action(AppAction::ToggleShortcutPopup);

        assert!(app.shortcut_popup);
    }
}
