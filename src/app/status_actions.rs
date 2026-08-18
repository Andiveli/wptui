use crate::app::App;
use crate::app::actions::{ActionNotice, ConversationMode, FocusPane, STATUS_REACTION, Section};

impl App<'_> {
    /// Reply from a status: switches to the contact's private chat with
    /// the status quoted, so the answer lands in the inbox.
    pub(crate) fn reply_to_status(&mut self) {
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

    /// Reacts to the selected status with a heart directly. Statuses do not
    /// open the general reaction picker because WhatsApp only allows the heart.
    pub(crate) fn heart_selected_status(&mut self) {
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
                STATUS_REACTION,
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
    use crate::app::actions::{ActionNotice, ConversationMode};
    use crate::app::test_support::TestApp;
    use whatsrust as wr;

    fn status_message(id: &str) -> wr::Message {
        wr::Message {
            info: wr::MessageInfo {
                id: id.into(),
                chat: "status@broadcast".to_owned().into(),
                sender: "alice@s.whatsapp.net".to_owned().into(),
                timestamp: 100,
                forwarding: Default::default(),
                is_from_me: false,
                quote_id: None,
                read_by: 0,
            },
            message: wr::MessageContent::Text("status".into()),
        }
    }

    #[test]
    fn reply_moves_status_into_the_contact_chat_with_a_quote() {
        let mut app = TestApp::new();
        let message = status_message("status-1");
        let sender = message.info.sender.clone();
        app.add_message(message);
        app.message_list_state
            .set_selected_message("status-1".into());

        app.reply_to_status();

        assert_eq!(app.selected_section, Section::Chats);
        assert_eq!(app.open_chat(), Some(sender));
        assert_eq!(app.focus_pane, FocusPane::Conversation);
        assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
        assert_eq!(
            app.composer
                .quote
                .as_ref()
                .map(|item| item.info.id.as_ref()),
            Some("status-1")
        );
    }

    #[test]
    fn reaction_without_a_selected_status_reports_unavailability() {
        let mut app = TestApp::new();

        app.heart_selected_status();

        assert!(matches!(
            &app.action_notice,
            Some(ActionNotice::Unavailable(message)) if message == "Reaction is not available"
        ));
    }
}
