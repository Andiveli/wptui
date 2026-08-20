use super::App;
use whatsrust as wr;

impl App<'_> {
    pub(crate) fn reanchor_message_selection(&mut self, chat_jid: &wr::JID) {
        let Some(anchor) = self.message_list_state.get_selected_message() else {
            return;
        };
        let Some(messages) = self.chat_messages.get(chat_jid) else {
            return;
        };
        if let Some(index) = messages
            .iter()
            .rev()
            .filter(|id| self.messages.contains_key(*id))
            .position(|id| id == &anchor)
        {
            self.message_list_state.selected = Some(index);
        }
    }

    pub fn sort_chats(&mut self) {
        let selected = self.get_selected_chat();
        let mut entries: Vec<_> = self.chats.values().cloned().collect();
        entries.sort_by(|a, b| {
            let a_time = a.last_message_time.unwrap_or_default();
            let b_time = b.last_message_time.unwrap_or_default();
            b_time.cmp(&a_time)
        });

        self.sorted_chats = entries
            .iter()
            .map(|chat| chat.jid.clone())
            .filter(|jid: &wr::JID| !jid.0.as_ref().ends_with("@broadcast"))
            .collect();
        self.select_chat(selected);
    }

    pub(crate) fn sort_chat_messages(&mut self, chat_jid: wr::JID) {
        let revision = self
            .message_sequence_revisions
            .get(&chat_jid)
            .copied()
            .unwrap_or(0);
        let cache_was_valid = self
            .message_sequence_cache
            .get(&chat_jid)
            .is_some_and(|cache| cache.is_valid_for(revision));
        let before = self
            .chat_messages
            .get(&chat_jid)
            .cloned()
            .unwrap_or_default();
        let before_keys = before
            .iter()
            .map(|id| message_order_key(self.messages.get(id)))
            .collect::<Vec<_>>();
        if let Some(messages) = self.chat_messages.get_mut(&chat_jid) {
            messages.sort_by_cached_key(|msg_id| {
                (
                    self.messages
                        .get(msg_id)
                        .map(|m| m.info.timestamp)
                        .unwrap_or(i64::MIN),
                    msg_id.clone(),
                )
            });
            let after_keys = messages
                .iter()
                .map(|id| message_order_key(self.messages.get(id)))
                .collect::<Vec<_>>();
            if cache_was_valid && (*messages != before || before_keys != after_keys) {
                self.invalidate_message_sequence(&chat_jid);
                self.message_height_cache.mark_layout_changed();
            }
        }
    }
}

fn message_order_key(message: Option<&wr::Message>) -> Option<(i64, wr::JID, bool)> {
    message.map(|message| {
        (
            message.info.timestamp,
            message.info.sender.clone(),
            message.info.is_from_me,
        )
    })
}

#[cfg(test)]
mod tests;
