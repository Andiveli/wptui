use std::sync::Arc;

use log::{info, warn};
use whatsrust as wr;

use super::{App, Chat};

impl App<'_> {
    pub fn load_data_from_db(&mut self) {
        info!("Reading database");
        for chat in self.db_handler.get_chats() {
            self.chats.insert(chat.jid.clone(), chat);
        }
        for (jid, name) in self.db_handler.get_contacts() {
            self.contacts.insert(jid, name);
        }

        for action in self.db_handler.get_message_actions() {
            self.local_action_sequence = self.local_action_sequence.max(
                action
                    .action_id
                    .rsplit_once(':')
                    .and_then(|(_, sequence)| sequence.parse().ok())
                    .unwrap_or_default(),
            );
            self.message_actions
                .entry(action.target_message_id.clone())
                .or_default()
                .push(action);
        }

        for message in self.db_handler.get_messages() {
            self.add_message_without_sort(message);
        }
        let chat_ids = self.chat_messages.keys().cloned().collect::<Vec<_>>();
        for chat_id in chat_ids {
            self.sort_chat_messages(chat_id);
        }
        for (message_id, participant, emoji) in self.db_handler.get_reactions() {
            self.reactions
                .entry(message_id)
                .or_default()
                .insert(participant, emoji);
        }
        warn!(
            "Finished reading database with {} chats and {} messages",
            self.chats.len(),
            self.messages.len()
        );
    }

    /// Display name for a JID (chat or sender). Falls back to the JID string if not in contacts.
    pub fn contact_name(&self, jid: &wr::JID) -> Arc<str> {
        self.contacts
            .get(jid)
            .cloned()
            .unwrap_or_else(|| jid.0.clone())
    }

    pub fn add_message(&mut self, message: wr::Message) {
        let chat_jid = message.info.chat.clone();
        let is_open_chat = self.open_chat.as_ref() == Some(&chat_jid);
        self.add_message_without_sort(message);
        self.sort_chat_messages(chat_jid.clone());
        if is_open_chat {
            self.reanchor_message_selection(&chat_jid);
        }
    }

    fn add_message_without_sort(&mut self, message: wr::Message) {
        let chat_jid = message.info.chat.clone();
        self.add_or_update_chat(
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
        self.refresh_message_projection(&id);
        self.refresh_status_contacts();
    }

    pub(crate) fn add_or_update_chat<F: FnOnce(&mut Chat)>(&mut self, chat: Chat, callback: F) {
        if let Some(existing_chat) = self.chats.get_mut(&chat.jid) {
            callback(existing_chat);
            self.db_handler.add_chat(existing_chat);
        } else {
            self.db_handler.add_chat(&chat);
            self.chats.insert(chat.jid.clone(), chat);
        }
    }

    pub(crate) fn get_contacts(&mut self) {
        for (jid, name) in wr::get_contacts() {
            self.contacts.insert(jid.clone(), name.clone());
            self.db_handler.add_contact(&jid, name.as_ref());
        }
    }
}

#[cfg(test)]
mod tests;
