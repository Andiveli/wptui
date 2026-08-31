use super::super::{App, StoreStatusCursor};
use whatsrust as wr;

impl App<'_> {
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
                match self.status_cursor.store(StoreStatusCursor {
                    contact: sender.clone(),
                    timestamp,
                }) {
                    Ok(()) => {
                        self.status_last_seen.insert(sender, timestamp);
                    }
                    Err(error) => {
                        log::error!("status receipt cursor write failed: {error}");
                    }
                }
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
}
