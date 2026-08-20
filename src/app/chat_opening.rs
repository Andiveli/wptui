use whatsrust as wr;

use super::{App, Chat, STATUS_BROADCAST_CHAT};

#[cfg(test)]
mod tests;

impl App<'_> {
    pub fn open_chat(&self) -> Option<wr::JID> {
        self.open_chat.clone()
    }

    /// Opens the currently highlighted chat: it becomes the rendered
    /// conversation, composer target, and presence subscription.
    pub fn open_selected_chat(&mut self) {
        crate::crash_diagnostics::breadcrumb("chat-open", "start");
        if let Some(chat) = self.get_selected_chat() {
            self.open_chat = Some(chat.clone());
            self.refresh_group_permission(&chat);
            self.sort_chat_messages(chat);
            crate::crash_diagnostics::breadcrumb("chat-open", "messages-sorted");
            self.message_list_state.reset();
            self.restore_read_cursor_anchor();
            crate::crash_diagnostics::breadcrumb("chat-open", "ready");
        }
    }

    fn refresh_group_permission(&mut self, chat: &wr::JID) {
        if Self::is_group_chat(chat) {
            self.group_permissions.remove(chat);
            if let Ok(info) = wr::get_group_info(chat) {
                self.group_permissions.insert(chat.clone(), info);
            }
            self.composer
                .set_group_participants(wr::get_group_participants(chat));
        } else {
            self.composer.set_group_participants(Vec::new());
        }
        self.composer.set_blocked(self.composer_blocked());
    }

    pub fn composer_blocked(&self) -> bool {
        self.open_chat().is_some_and(|chat| {
            self.group_permissions
                .get(&chat)
                .is_some_and(|info| info.is_announce && !info.is_admin)
        })
    }

    pub fn is_status_chat(jid: &wr::JID) -> bool {
        jid.0.as_ref() == STATUS_BROADCAST_CHAT
    }

    /// True for group conversations (JIDs of the form `number@g.us`).
    pub fn is_group_chat(jid: &wr::JID) -> bool {
        jid.0.as_ref().ends_with("@g.us")
    }

    /// Opens a direct conversation by JID, regardless of whether it already
    /// appears in the chat list. The in-memory entry is created here so the
    /// recipient shows up as a row; the database row is created on the first
    /// real message, so an empty conversation is never persisted.
    pub fn open_chat_by_jid(&mut self, jid: wr::JID) {
        self.chats.entry(jid.clone()).or_insert_with(|| Chat {
            jid: jid.clone(),
            last_message_time: None,
        });
        self.sort_chats();
        self.open_chat = Some(jid.clone());
        self.composer.set_blocked(self.composer_blocked());
        self.sort_chat_messages(jid);
        self.message_list_state.reset();
        self.restore_read_cursor_anchor();
    }

    fn restore_read_cursor_anchor(&mut self) {
        crate::crash_diagnostics::breadcrumb("unread-cursor", "load-start");
        let Some(chat) = self.open_chat.as_ref() else {
            return;
        };
        if let Some(message_id) = self
            .timeline
            .get(chat)
            .and_then(|state| state.last_read_message.clone())
        {
            self.message_list_state.set_selected_message(message_id);
            crate::crash_diagnostics::breadcrumb("unread-cursor", "restored");
        } else {
            crate::crash_diagnostics::breadcrumb("unread-cursor", "none");
        }
    }
}
