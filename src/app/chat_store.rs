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
        for (chat, message_id, timestamp) in self.db_handler.read_cursors() {
            self.timeline.insert(
                chat,
                super::unread_messages::ChatTimelineState {
                    pending_new_messages: 0,
                    last_read_message: Some(message_id),
                    last_read_at: Some(timestamp),
                },
            );
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
            .map(|name| canonical_contact_name(name))
            .unwrap_or_else(|| jid.0.clone())
    }

    pub fn message_sender_name(&self, message: &wr::Message) -> Arc<str> {
        self.contacts
            .get(&message.info.sender)
            .map(|name| canonical_contact_name(name))
            .or_else(|| {
                wr::message_push_name(&message.info.id).map(|name| canonical_contact_name(&name))
            })
            .unwrap_or_else(|| self.contact_name(&message.info.sender))
    }

    pub fn add_message(&mut self, message: wr::Message) {
        let chat_jid = message.info.chat.clone();
        let is_open_chat = self.open_chat.as_ref() == Some(&chat_jid);
        let is_inbound = !message.info.is_from_me && !App::is_status_chat(&chat_jid);
        let is_confirmed_own_message = message.info.is_from_me && !App::is_status_chat(&chat_jid);
        let was_at_latest = self.is_viewing_latest_message(&chat_jid);
        self.add_message_without_sort(message);
        self.sort_chat_messages(chat_jid.clone());
        if is_open_chat {
            if is_confirmed_own_message || (is_inbound && was_at_latest) {
                self.catch_up_to_latest(&chat_jid);
            } else {
                if is_inbound {
                    self.timeline
                        .entry(chat_jid.clone())
                        .or_default()
                        .pending_new_messages += 1;
                }
                self.reanchor_message_selection(&chat_jid);
            }
        } else if is_confirmed_own_message {
            self.mark_chat_read_at_latest(&chat_jid);
        } else if is_inbound {
            self.timeline
                .entry(chat_jid)
                .or_default()
                .pending_new_messages += 1;
        }
    }

    fn is_viewing_latest_message(&self, chat: &wr::JID) -> bool {
        let latest = self
            .chat_messages
            .get(chat)
            .and_then(|messages| messages.last());
        match self.message_list_state.get_selected_message() {
            Some(anchor) => latest == Some(&anchor),
            None => self
                .message_list_state
                .selected
                .is_none_or(|selected| selected == 0),
        }
    }

    pub fn catch_up_to_latest(&mut self, chat: &wr::JID) {
        if let Some(latest) = self
            .chat_messages
            .get(chat)
            .and_then(|messages| messages.last())
            .cloned()
        {
            self.message_list_state.follow_latest(latest);
        }
        self.mark_chat_read_at_latest(chat);
    }

    pub fn mark_chat_read_at_latest(&mut self, chat: &wr::JID) {
        let latest = self
            .chat_messages
            .get(chat)
            .and_then(|messages| messages.last())
            .cloned();
        let timestamp = latest
            .as_ref()
            .and_then(|id| self.messages.get(id))
            .map_or(0, |message| message.info.timestamp);
        let timeline = self.timeline.entry(chat.clone()).or_default();
        timeline.pending_new_messages = 0;
        timeline.last_read_message = latest.clone();
        timeline.last_read_at = Some(timestamp);
        if let Some(message_id) = latest {
            self.db_handler
                .set_last_read_cursor(chat, Some(message_id), timestamp);
        }
    }

    pub fn pending_new_messages(&self, chat: &wr::JID) -> usize {
        self.timeline
            .get(chat)
            .map_or(0, |state| state.pending_new_messages)
    }

    pub fn unread_boundary(&self, chat: &wr::JID) -> Option<(usize, i64)> {
        let state = self.timeline.get(chat)?;
        let cursor = (state.last_read_at?, state.last_read_message.clone()?);
        let unread = self
            .chat_messages
            .get(chat)?
            .iter()
            .filter_map(|id| self.messages.get(id))
            .filter(|message| (message.info.timestamp, message.info.id.clone()) > cursor)
            .collect::<Vec<_>>();
        unread
            .first()
            .map(|message| (unread.len(), message.info.timestamp))
    }

    pub(crate) fn add_message_without_sort(&mut self, message: wr::Message) {
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

fn canonical_contact_name(name: &str) -> Arc<str> {
    let name = name.trim();
    ["~ ", "+ "]
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))
        .unwrap_or(name)
        .trim()
        .into()
}

#[cfg(test)]
mod sender_name_tests {
    use super::*;
    use crate::app::test_support::TestApp;

    fn message(id: &str, sender: &str) -> wr::Message {
        wr::Message {
            info: wr::MessageInfo {
                id: id.into(),
                chat: sender.to_owned().into(),
                sender: sender.to_owned().into(),
                mentions_self: false,
                timestamp: 0,
                is_from_me: false,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text("body".into()),
        }
    }

    #[test]
    fn local_contact_name_wins_over_message_push_name() {
        let mut app = TestApp::new();
        let sender = wr::JID::from("123@s.whatsapp.net".to_owned());
        app.contacts
            .insert(sender.clone(), "Saved Full Name".into());
        let message = message("local-name", sender.0.as_ref());
        wr::store_message_push_name(&message.info.id, "WhatsApp Profile");

        assert_eq!(
            app.message_sender_name(&message).as_ref(),
            "Saved Full Name"
        );
    }

    #[test]
    fn unsaved_message_push_name_is_plain_and_numeric_is_final_fallback() {
        let app = TestApp::new();
        let with_push = message("push-name", "123@s.whatsapp.net");
        wr::store_message_push_name(&with_push.info.id, "WhatsApp Profile");
        assert_eq!(
            app.message_sender_name(&with_push).as_ref(),
            "WhatsApp Profile"
        );

        let without_push = message("numeric-name", "456@s.whatsapp.net");
        assert_eq!(
            app.message_sender_name(&without_push).as_ref(),
            "456@s.whatsapp.net"
        );
    }
}

#[cfg(test)]
mod tests;
