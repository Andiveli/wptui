use crate::app::App;
use crate::app::actions::{ActionNotice, AppAction, COMMON_REACTIONS};

impl App<'_> {
    pub(crate) fn dispatch_reaction_picker_action(&mut self, action: AppAction) {
        match action {
            AppAction::ReactionPrev => self.move_reaction(-1),
            AppAction::ReactionNext => self.move_reaction(1),
            AppAction::ConfirmReaction => self.confirm_reaction(),
            AppAction::CancelReaction => {
                self.reaction_picker = None;
                self.action_notice = Some(ActionNotice::Cancelled);
            }
            _ => unreachable!("non-reaction-picker action dispatched to reaction picker"),
        }
    }

    pub(crate) fn open_reaction_picker(&mut self) {
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
            self.action_notice = Some(ActionNotice::Reacted);
        } else {
            self.unavailable("Could not react to message");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::ActionNotice;
    use crate::app::test_support::{TestApp, message};
    use whatsrust as wr;

    fn selected_app() -> TestApp {
        let mut app = TestApp::new();
        let chat = wr::JID::from("chat@example.test".to_owned());
        let selected = message(&chat, "message", 1);
        app.add_message(selected);
        app.message_list_state
            .set_selected_message("message".into());
        app
    }

    #[test]
    fn opens_with_common_reactions_and_moves_within_bounds() {
        let mut app = selected_app();

        app.open_reaction_picker();
        assert_eq!(
            app.reaction_picker.as_ref().map(|(items, selected)| (
                items.iter().map(String::as_str).collect::<Vec<_>>(),
                *selected,
            )),
            Some((COMMON_REACTIONS.to_vec(), 0))
        );

        app.dispatch_reaction_picker_action(AppAction::ReactionNext);
        app.dispatch_reaction_picker_action(AppAction::ReactionPrev);
        app.dispatch_reaction_picker_action(AppAction::ReactionPrev);
        assert_eq!(
            app.reaction_picker.as_ref().map(|(_, selected)| *selected),
            Some(0)
        );
    }

    #[test]
    fn cancel_clears_picker_and_reports_cancellation() {
        let mut app = selected_app();
        app.open_reaction_picker();

        app.dispatch_reaction_picker_action(AppAction::CancelReaction);

        assert!(app.reaction_picker.is_none());
        assert_eq!(app.action_notice, Some(ActionNotice::Cancelled));
    }

    #[test]
    fn opening_without_selection_reports_unavailable_action() {
        let mut app = TestApp::new();

        app.open_reaction_picker();

        assert!(app.reaction_picker.is_none());
        assert!(matches!(
            app.action_notice,
            Some(ActionNotice::Unavailable(_))
        ));
    }
}
