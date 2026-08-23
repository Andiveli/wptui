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
        let diagnostics = self.message_action_diagnostics.clone();
        match self.db_handler.status_last_seen() {
            Ok(cursors) => {
                diagnostics.record_read_sync(|| {
                    format!(
                        "source=rust event=status_cursor_read rows={}",
                        cursors.len()
                    )
                });
                for (contact, timestamp) in cursors {
                    let contact_id =
                        crate::app::message_action_diagnostics::identifier_for_log(&contact.0);
                    diagnostics.record_read_sync(|| {
                        format!(
                            "source=rust event=status_cursor_restored contact={contact_id} timestamp={timestamp}"
                        )
                    });
                    self.status_last_seen.insert(contact, timestamp);
                }
            }
            Err(error) => log::error!("status cursor read failed: {error}"),
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
        self.invalidate_chat_list();
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
        self.with_chat_list_mutation(|app| app.add_message_inner(message));
    }

    fn add_message_inner(&mut self, message: wr::Message) {
        let chat_jid = message.info.chat.clone();
        let is_open_chat = self.open_chat.as_ref() == Some(&chat_jid);
        let is_inbound = !message.info.is_from_me && !App::is_status_chat(&chat_jid);
        let is_confirmed_own_message = message.info.is_from_me && !App::is_status_chat(&chat_jid);
        let was_at_latest = self.is_viewing_latest_message(&chat_jid);
        let projection_changed = self.add_message_without_sort(message);
        self.sort_chat_messages(chat_jid.clone());
        if !projection_changed {
            return;
        }
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
        if projection_changed {
            self.invalidate_chat_list();
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

    pub fn mark_chat_read_at_latest(&mut self, chat: &wr::JID) -> bool {
        let latest = self
            .chat_messages
            .get(chat)
            .and_then(|messages| messages.last())
            .cloned();
        let timestamp = latest
            .as_ref()
            .and_then(|id| self.messages.get(id))
            .map_or(0, |message| message.info.timestamp);
        let unchanged = self.timeline.get(chat).is_some_and(|timeline| {
            timeline.last_read_message == latest && timeline.last_read_at == Some(timestamp)
        });
        let timeline = self.timeline.entry(chat.clone()).or_default();
        let changed = timeline.pending_new_messages != 0
            || timeline.last_read_message != latest
            || timeline.last_read_at != Some(timestamp);
        let mut scheduled = false;
        timeline.pending_new_messages = 0;
        timeline.last_read_message = latest.clone();
        timeline.last_read_at = Some(timestamp);
        if let Some(message_id) = latest {
            self.db_handler
                .set_last_read_cursor(chat, Some(message_id.clone()), timestamp);
            if !unchanged && !Self::is_status_chat(chat) {
                if let Some(message) = self.messages.get(&message_id) {
                    let participant = Self::is_group_chat(chat).then_some(&message.info.sender);
                    wr::sync_chat_read(
                        chat,
                        &message.info.id,
                        timestamp,
                        message.info.is_from_me,
                        participant,
                    );
                    scheduled = true;
                }
            }
        }
        if changed {
            self.invalidate_chat_list();
        }
        scheduled
    }

    pub fn pending_new_messages(&self, chat: &wr::JID) -> usize {
        self.timeline
            .get(chat)
            .map_or(0, |state| state.pending_new_messages)
    }

    pub(crate) fn apply_remote_chat_read(
        &mut self,
        chat: wr::JID,
        message_id: wr::MessageId,
        read: bool,
        timestamp: i64,
        from_me: bool,
        participant: Option<wr::JID>,
    ) {
        let diagnostics = self.message_action_diagnostics.clone();
        let record = |outcome: &str| {
            diagnostics.record_read_sync(|| {
                format!(
                    "source=rust event={outcome} chat={} message={} timestamp={timestamp} from_me={from_me} participant={}",
                    crate::app::message_action_diagnostics::identifier_for_log(&chat.0),
                    if message_id.is_empty() {
                        "<missing>".to_owned()
                    } else {
                        crate::app::message_action_diagnostics::identifier_for_log(&message_id)
                    },
                    participant.as_ref().map_or_else(
                        || "<missing>".to_owned(),
                        |jid| crate::app::message_action_diagnostics::identifier_for_log(&jid.0),
                    ),
                )
            });
        };
        record("received");
        if Self::is_status_chat(&chat) {
            record("rejected reason=status_exclusion");
            return;
        }
        if !read {
            record("rejected reason=read_false");
            return;
        }
        if message_id.is_empty() || (Self::is_group_chat(&chat) && participant.is_none()) {
            record("rejected reason=missing_identity");
            return;
        }
        let Some(message) = self.messages.get(&message_id) else {
            record("rejected reason=message_unavailable");
            return;
        };
        if message.info.chat != chat {
            record("rejected reason=chat_mismatch");
            return;
        }
        if message.info.timestamp != timestamp {
            record("rejected reason=timestamp_mismatch");
            return;
        }
        if message.info.is_from_me != from_me {
            record("rejected reason=from_me_mismatch");
            return;
        }
        if participant
            .as_ref()
            .is_some_and(|jid| &message.info.sender != jid)
        {
            record("rejected reason=participant_mismatch");
            return;
        }
        let cursor = (timestamp, message_id.clone());
        let current = self.timeline.get(&chat).and_then(|timeline| {
            timeline
                .last_read_at
                .zip(timeline.last_read_message.clone())
        });
        if current.as_ref().is_some_and(|current| *current >= cursor) {
            record("rejected reason=stale_cursor");
            return;
        }
        let pending = self
            .chat_messages
            .get(&chat)
            .into_iter()
            .flatten()
            .filter_map(|id| self.messages.get(id))
            .filter(|item| {
                !item.info.is_from_me && (item.info.timestamp, item.info.id.clone()) > cursor
            })
            .count();
        let timeline = self.timeline.entry(chat.clone()).or_default();
        timeline.last_read_message = Some(message_id.clone());
        timeline.last_read_at = Some(timestamp);
        timeline.pending_new_messages = pending;
        self.db_handler
            .set_last_read_cursor(&chat, Some(message_id.clone()), timestamp);
        self.invalidate_chat_list();
        record("applied");
    }

    pub(crate) fn apply_receipt(
        &mut self,
        kind: wr::ReceiptKind,
        chat: wr::JID,
        message_ids: Vec<wr::MessageId>,
    ) {
        if kind == wr::ReceiptKind::Read {
            let mut linked_device_reads = Vec::new();
            for message_id in message_ids {
                let Some(message) = self.messages.get(&message_id) else {
                    self.record_receipt_classification(
                        kind,
                        &chat,
                        &message_id,
                        "rejected_unknown",
                    );
                    continue;
                };
                if message.info.chat != chat {
                    self.record_receipt_classification(
                        kind,
                        &chat,
                        &message_id,
                        "rejected_chat_mismatch",
                    );
                    continue;
                }
                if message.info.is_from_me {
                    let message = self
                        .messages
                        .get_mut(&message_id)
                        .expect("message was validated");
                    message.info.read_by = message.info.read_by.saturating_add(1);
                    self.db_handler.add_message(message);
                    self.record_receipt_classification(kind, &chat, &message_id, "peer_read");
                } else {
                    linked_device_reads.push(message_id.clone());
                    self.record_receipt_classification(
                        kind,
                        &chat,
                        &message_id,
                        "linked_device_read",
                    );
                }
            }
            if !linked_device_reads.is_empty() {
                self.apply_local_read_cursor(kind, chat, linked_device_reads);
            }
            return;
        }

        self.apply_local_read_cursor(kind, chat, message_ids);
    }

    fn record_receipt_classification(
        &self,
        kind: wr::ReceiptKind,
        chat: &wr::JID,
        message_id: &wr::MessageId,
        outcome: &str,
    ) {
        self.message_action_diagnostics.record_read_sync(|| {
            format!(
                "source=rust event=receipt_classification kind={kind:?} outcome={outcome} chat={} message={}",
                crate::app::message_action_diagnostics::identifier_for_log(&chat.0),
                if message_id.is_empty() {
                    "<missing>".to_owned()
                } else {
                    crate::app::message_action_diagnostics::identifier_for_log(message_id)
                },
            )
        });
    }

    fn apply_local_read_cursor(
        &mut self,
        kind: wr::ReceiptKind,
        chat: wr::JID,
        message_ids: Vec<wr::MessageId>,
    ) {
        let mut valid_messages = Vec::new();
        for message_id in message_ids {
            let Some(message) = self.messages.get(&message_id) else {
                self.record_receipt_classification(kind, &chat, &message_id, "rejected_unknown");
                continue;
            };
            if message.info.chat != chat {
                self.record_receipt_classification(
                    kind,
                    &chat,
                    &message_id,
                    "rejected_chat_mismatch",
                );
                continue;
            }
            if kind == wr::ReceiptKind::ReadSelf {
                self.record_receipt_classification(kind, &chat, &message_id, "linked_device_read");
            }
            valid_messages.push((
                message_id,
                message.info.sender.clone(),
                message.info.timestamp,
            ));
        }

        if Self::is_status_chat(&chat) {
            let mut latest_by_sender = std::collections::HashMap::<wr::JID, i64>::new();
            for (_, sender, timestamp) in valid_messages {
                latest_by_sender
                    .entry(sender)
                    .and_modify(|latest| *latest = (*latest).max(timestamp))
                    .or_insert(timestamp);
            }
            for (sender, timestamp) in latest_by_sender {
                let current = self
                    .status_last_seen
                    .get(&sender)
                    .copied()
                    .unwrap_or_default();
                if timestamp <= current {
                    continue;
                }
                if let Err(error) = self.db_handler.set_status_last_seen(&sender, timestamp) {
                    log::error!("status receipt cursor write failed: {error}");
                    continue;
                }
                self.status_last_seen.insert(sender, timestamp);
            }
            return;
        }

        let Some((target_id, target_timestamp)) = valid_messages
            .into_iter()
            .max_by_key(|(message_id, _, timestamp)| (*timestamp, message_id.clone()))
            .map(|(message_id, _, timestamp)| (message_id, timestamp))
        else {
            return;
        };
        let target_cursor = (target_timestamp, target_id.clone());
        let current = self.timeline.get(&chat).and_then(|timeline| {
            timeline
                .last_read_at
                .zip(timeline.last_read_message.clone())
        });
        if current
            .as_ref()
            .is_some_and(|cursor| *cursor >= target_cursor)
        {
            self.record_receipt_classification(kind, &chat, &target_id, "rejected_stale_cursor");
            return;
        }
        let pending = self
            .chat_messages
            .get(&chat)
            .into_iter()
            .flatten()
            .filter_map(|id| self.messages.get(id))
            .filter(|message| {
                !message.info.is_from_me
                    && (message.info.timestamp, message.info.id.clone()) > target_cursor
            })
            .count();
        let timeline = self.timeline.entry(chat.clone()).or_default();
        timeline.last_read_message = Some(target_id.clone());
        timeline.last_read_at = Some(target_timestamp);
        timeline.pending_new_messages = pending;
        self.db_handler
            .set_last_read_cursor(&chat, Some(target_id), target_timestamp);
        self.invalidate_chat_list();
        self.record_receipt_classification(
            kind,
            &chat,
            &target_cursor.1,
            "linked_device_cursor_applied",
        );
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

    pub(crate) fn get_contacts(&mut self) {
        let mut changed = false;
        for (jid, name) in wr::get_contacts() {
            changed |= self.contacts.get(&jid) != Some(&name);
            self.contacts.insert(jid.clone(), name.clone());
            self.db_handler.add_contact(&jid, name.as_ref());
        }
        if changed {
            self.invalidate_chat_list();
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
