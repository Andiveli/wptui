use super::{App, ConversationMode, FocusPane, Section, actions::ActionNotice};

#[cfg(test)]
mod tests;

impl App<'_> {
    /// Jumps to a private conversation with the sender of the selected
    /// incoming message. The chat-opening mechanics remain owned by
    /// `chat_opening`.
    pub fn reply_privately(&mut self) {
        let Some(message) = self.selected_message().cloned() else {
            return self.unavailable("Reply in private is not available");
        };
        if self.message_status(&message.info.id).deleted {
            return self.unavailable("This message was deleted.");
        }
        if message.info.is_from_me {
            return self.unavailable("Reply in private is not available for your own messages");
        }
        let Some(chat) = self.open_chat() else {
            return self.unavailable("Reply in private is not available");
        };
        if self.selected_section == Section::Status || !Self::is_group_chat(&chat) {
            return self.unavailable("Reply in private is only available in groups");
        }
        if message.info.chat != chat {
            return self.unavailable("Reply in private is not available");
        }

        // Group participants can be a LID while the real direct chat lives
        // under its phone number; resolve so we open/send to the stored chat
        // instead of an empty LID-keyed thread.
        let sender = message.info.sender.clone();
        let target = self.dm_resolver.resolve_dm_chat(&sender).unwrap_or(sender);
        let name = self.contact_name(&target).to_string();
        self.open_chat_by_jid(target);
        self.selected_section = Section::Chats;
        self.composer.quote = Some(message);
        self.conversation_mode = ConversationMode::ComposerEditing;
        self.focus_pane = FocusPane::Conversation;
        self.action_notice = Some(ActionNotice::ReplyPrivatelyNamed(name));
    }
}
