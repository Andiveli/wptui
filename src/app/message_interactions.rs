use super::App;
use super::actions::ConversationMode;
use whatsrust as wr;

impl App<'_> {
    pub(crate) fn selected_message_is_deleted(&self) -> bool {
        self.selected_message()
            .is_some_and(|message| self.message_status(&message.info.id).deleted)
    }

    pub(crate) fn selected_message_is_informational(&self) -> bool {
        self.selected_message().is_some_and(|message| {
            matches!(message.message, wr::MessageContent::ViewOnceUnavailable)
        })
    }

    pub(crate) fn copy_selected_text(&mut self) {
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

    pub(crate) fn delete_selected_message(&mut self) {
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

    pub(crate) fn start_message_edit(&mut self) {
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

    pub(crate) fn submit_message_edit(&mut self) {
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

    pub(crate) fn reply_to_selected(&mut self) {
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
}
