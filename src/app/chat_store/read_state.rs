use super::super::App;
use whatsrust as wr;

impl App<'_> {
    pub(super) fn is_viewing_latest_message(&self, chat: &wr::JID) -> bool {
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
}
