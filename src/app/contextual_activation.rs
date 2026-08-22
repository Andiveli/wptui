use super::App;
use crate::app::actions::{AppAction, Section};
use crate::app::contextual_actions::{
    AvailabilityFacts, ContextualAction, ContextualContext, ImplementationStatus,
    contextual_menu_rows, evaluate_availability,
};

impl App<'_> {
    pub(crate) fn contextual_context(&self) -> ContextualContext {
        let selected = self.selected_message();
        ContextualContext {
            focus: self.focus_pane,
            section: self.selected_section,
            has_selected_message: selected.is_some(),
            selected_text: selected.is_some_and(|message| {
                matches!(message.message, whatsrust::MessageContent::Text(_))
            }),
            has_reference: selected.is_some_and(|message| message.info.quote_id.is_some()),
            attach_blocked: self.composer_blocked() || self.selected_section == Section::Status,
        }
    }

    pub fn open_contextual_actions(&mut self) {
        self.contextual_menu = Some((contextual_menu_rows(self.contextual_context()), 0));
    }

    pub fn move_contextual_menu(&mut self, delta: isize) {
        if let Some((rows, selected)) = &mut self.contextual_menu {
            *selected = selected
                .saturating_add_signed(delta)
                .min(rows.len().saturating_sub(1));
        }
    }

    pub fn activate_contextual_action(&mut self) {
        let Some((rows, selected)) = self.contextual_menu.as_ref() else {
            return;
        };
        let Some(row) = rows.get(*selected).copied() else {
            return;
        };
        let context = self.contextual_context();
        let available = evaluate_availability(
            crate::app::contextual_actions::CONTEXTUAL_ACTION_METADATA
                .iter()
                .find(|metadata| metadata.action == row.action_token)
                .map_or(ImplementationStatus::Planned, |metadata| {
                    metadata.implementation
                }),
            Some(row.action_token),
            AvailabilityFacts {
                contextual: Some(context),
                contextual_activatable: false,
            },
        );
        if !available.activatable() {
            return;
        }
        self.contextual_menu = None;
        self.dispatch_action(contextual_action_to_app_action(row.action_token));
    }
}

fn contextual_action_to_app_action(action: ContextualAction) -> AppAction {
    match action {
        ContextualAction::Copy => AppAction::CopyMessage,
        ContextualAction::React => AppAction::ReactMessage,
        ContextualAction::Reply => AppAction::ReplyMessage,
        ContextualAction::Share => AppAction::ShareMessage,
        ContextualAction::ReplyPrivately => AppAction::ReplyPrivately,
        ContextualAction::Open => AppAction::OpenMessage,
        ContextualAction::ViewAttachment => AppAction::ViewMessage,
        ContextualAction::GoToReference => AppAction::GoToReference,
        ContextualAction::DeleteForEveryone => AppAction::DeleteMessage,
        ContextualAction::Attach => AppAction::AttachFile,
        _ => AppAction::PlannedLeaderAction(
            crate::app::contextual_actions::CONTEXTUAL_ACTION_METADATA
                .iter()
                .find(|meta| meta.action == action)
                .map_or("Contextual action", |meta| meta.label),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::{ComposerAction, FocusPane, Section};
    use crate::app::contextual_actions::RowStyle;
    use crate::app::test_support::TestApp;

    #[test]
    fn contextual_attach_dispatches_the_existing_attach_action() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.selected_section = Section::Chats;
        app.open_contextual_actions();
        let attach = app
            .contextual_menu
            .as_ref()
            .unwrap()
            .0
            .iter()
            .position(|row| row.action_token == ContextualAction::Attach)
            .unwrap();
        assert_eq!(
            app.contextual_menu.as_ref().unwrap().0[attach].row_style,
            RowStyle::Enabled
        );
        app.contextual_menu.as_mut().unwrap().1 = attach;
        app.activate_contextual_action();
        assert!(app.file_picker.is_some());
    }

    #[test]
    fn composer_direction_toggle_dispatches_through_app_action() {
        let mut app = TestApp::new();
        assert_eq!(
            app.composer_direction,
            crate::app::preferences::ComposerDirection::Auto
        );
        app.dispatch_action(AppAction::ToggleComposerDirection);
        assert_eq!(
            app.composer_direction,
            crate::app::preferences::ComposerDirection::Rtl
        );
        app.dispatch_action(AppAction::ToggleComposerDirection);
        assert_eq!(
            app.composer_direction,
            crate::app::preferences::ComposerDirection::Auto
        );
    }

    #[test]
    fn toggling_direction_preserves_the_existing_draft_and_cursor() {
        let mut app = TestApp::new();
        app.composer.insert_text("abc אבג");
        let text = app.composer.text();
        let cursor = app.composer.input.cursor();

        app.dispatch_action(AppAction::ToggleComposerDirection);

        assert_eq!(app.composer.text(), text);
        assert_eq!(app.composer.input.cursor(), cursor);
    }

    #[test]
    fn toggling_direction_preserves_mentions_quote_and_pending_attachments() {
        let mut app = TestApp::new();
        app.composer
            .set_group_participants(vec![whatsrust::GroupParticipant {
                jid: "111@s.whatsapp.net".to_owned().into(),
                phone_number: "111@s.whatsapp.net".to_owned().into(),
                name: "Alice".into(),
            }]);
        app.composer.insert_text("@al");
        app.composer.confirm_mention();
        app.composer.quote = Some(whatsrust::Message {
            info: whatsrust::MessageInfo {
                id: "quoted".into(),
                chat: "123@g.us".to_owned().into(),
                sender: "222@s.whatsapp.net".to_owned().into(),
                mentions_self: false,
                timestamp: 0,
                is_from_me: false,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: whatsrust::MessageContent::Text("quoted text".into()),
        });
        app.composer
            .queue_attachment("photo.jpg".into(), whatsrust::FileKind::Image);

        app.dispatch_action(AppAction::ToggleComposerDirection);

        assert_eq!(app.composer.text(), "@Alice ");
        assert!(
            app.composer
                .quote
                .as_ref()
                .is_some_and(|quote| quote.info.id.as_ref() == "quoted")
        );
        assert_eq!(app.composer.pending.len(), 1);
        assert_eq!(app.composer.pending[0].path.as_ref(), "photo.jpg");
        let outcome = app.composer.apply(ComposerAction::Submit);
        assert!(matches!(
            outcome,
            crate::app::composer::ComposerOutcome::Submit {
                quote: Some(quote),
                mentions,
                messages,
                ..
            } if quote.info.id.as_ref() == "quoted"
                && mentions == vec![whatsrust::Mention {
                    jid: "111@s.whatsapp.net".to_owned().into(),
                    numeric_user: "111".into(),
                }]
                && messages.iter().any(|message| matches!(
                    message,
                    whatsrust::MessageContent::File(file)
                        if file.path.as_ref() == "photo.jpg"
                            && file.caption.as_deref() == Some("@111 ")
                ))
        ));
    }

    #[test]
    fn direction_toggle_surfaces_persistence_failure_without_changing_state() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = TestApp::new();
        app.preferences_path = directory.path().to_path_buf();

        app.dispatch_action(AppAction::ToggleComposerDirection);

        assert_eq!(
            app.composer_direction,
            crate::app::preferences::ComposerDirection::Auto
        );
        assert!(matches!(
            &app.action_notice,
            Some(crate::app::actions::ActionNotice::Unavailable(message))
                if message.starts_with("Could not persist composer direction:")
        ));
    }

    #[test]
    fn contextual_attach_does_not_activate_when_composer_is_blocked() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.selected_section = Section::Chats;
        let group = whatsrust::JID("123@g.us".into());
        app.open_chat_by_jid(group.clone());
        app.group_permissions.insert(
            group.clone(),
            whatsrust::GroupInfo {
                jid: group,
                name: "Admins only".into(),
                is_announce: true,
                is_admin: false,
            },
        );
        app.open_contextual_actions();
        let attach = app
            .contextual_menu
            .as_ref()
            .unwrap()
            .0
            .iter()
            .position(|row| row.action_token == ContextualAction::Attach)
            .unwrap();
        assert_eq!(
            app.contextual_menu.as_ref().unwrap().0[attach].row_style,
            RowStyle::Disabled
        );
        app.contextual_menu.as_mut().unwrap().1 = attach;
        app.activate_contextual_action();
        assert!(app.file_picker.is_none());
        assert!(app.contextual_menu.is_some());
    }
}
