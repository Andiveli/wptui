use super::super::{App, Chat};
use whatsrust as wr;

impl App<'_> {
    pub(crate) fn add_message_without_sort(&mut self, message: wr::Message) -> bool {
        let chat_jid = message.info.chat.clone();
        let chat_changed = self.add_or_update_chat(
            Chat {
                jid: chat_jid.clone(),
                last_message_time: Some(message.info.timestamp),
            },
            |chat| {
                if Some(message.info.timestamp) > chat.last_message_time {
                    chat.last_message_time = Some(message.info.timestamp);
                }
            },
        );

        let id = message.info.id.clone();
        let is_new = !self.messages.contains_key(&id);
        // Ownership is monotonic for a protocol message ID. A later echo or
        // history revision may improve content, but it must not erase a
        // previously verified local-owner fact.
        let mut message = message;
        if self
            .messages
            .get(&id)
            .is_some_and(|existing| existing.info.is_from_me)
        {
            message.info.is_from_me = true;
        }
        let old_order_key = self.messages.get(&id).map(|existing| {
            (
                existing.info.timestamp,
                existing.info.sender.clone(),
                existing.info.is_from_me,
            )
        });
        let should_replace = self
            .messages
            .get(&id)
            .is_none_or(|existing| existing.info.timestamp < message.info.timestamp);
        if should_replace {
            self.messages.insert(id.clone(), message);
        }
        if is_new {
            self.chat_messages
                .entry(chat_jid.clone())
                .or_default()
                .push(id.clone());
        }
        let new_order_key = self.messages.get(&id).map(|current| {
            (
                current.info.timestamp,
                current.info.sender.clone(),
                current.info.is_from_me,
            )
        });
        if is_new || old_order_key != new_order_key {
            self.invalidate_message_sequence(&chat_jid);
        }
        self.refresh_message_projection(&id);
        self.refresh_status_contacts();
        chat_changed || is_new || should_replace
    }

    pub(crate) fn add_or_update_chat<F: FnOnce(&mut Chat)>(
        &mut self,
        chat: Chat,
        callback: F,
    ) -> bool {
        if let Some(existing_chat) = self.chats.get_mut(&chat.jid) {
            let before = existing_chat.last_message_time;
            callback(existing_chat);
            self.db_handler.add_chat(existing_chat);
            before != existing_chat.last_message_time
        } else {
            self.db_handler.add_chat(&chat);
            self.chats.insert(chat.jid.clone(), chat);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::test_support::TestApp;
    use whatsrust as wr;

    fn message(chat: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
        wr::Message {
            info: wr::MessageInfo {
                id: id.into(),
                chat: chat.clone(),
                sender: chat.clone(),
                mentions_self: false,
                timestamp,
                forwarding: Default::default(),
                is_from_me: false,
                quote_id: None,
                read_by: 0,
            },
            message: wr::MessageContent::Text(id.into()),
        }
    }

    #[test]
    fn stores_message_without_sorting_its_chat_index() {
        let mut app = TestApp::new();
        let chat = wr::JID::from("chat@example.test".to_owned());

        app.add_message_without_sort(message(&chat, "newest", 30));
        app.add_message_without_sort(message(&chat, "oldest", 10));

        assert_eq!(
            app.chat_messages[&chat]
                .iter()
                .map(|id| id.as_ref())
                .collect::<Vec<_>>(),
            ["newest", "oldest"]
        );
    }
}
