use super::super::{App, StoreChatReadCursor};
use whatsrust as wr;

impl App<'_> {
    pub(super) fn restore_read_cursors(&mut self) {
        for (chat, message_id, timestamp) in self.chat_read_cursor.load() {
            self.timeline.insert(
                chat,
                super::super::unread_messages::ChatTimelineState {
                    manually_unread: false,
                    pending_new_messages: 0,
                    last_read_message: Some(message_id),
                    last_read_at: Some(timestamp),
                },
            );
        }
    }

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
        self.mark_chat_read_at_latest_inner(chat, true)
    }

    fn mark_chat_read_at_latest_inner(&mut self, chat: &wr::JID, sync: bool) -> bool {
        let manual = self.chat_is_manually_unread(chat);
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
        if let Some(message_id) = latest.as_ref() {
            self.chat_read_cursor.store(StoreChatReadCursor {
                chat: chat.clone(),
                message_id: Some(message_id.clone()),
                timestamp,
            });
        }
        if sync
            && (!unchanged || manual)
            && (latest.is_some() || manual)
            && !Self::is_status_chat(chat)
        {
            scheduled = self.schedule_chat_read_state(chat, latest.as_ref(), true);
            if !scheduled {
                log::warn!("chat read sync skipped: worker queue unavailable");
            }
        }
        if !sync || scheduled {
            self.set_manual_unread(chat, false);
        }
        if changed {
            self.invalidate_chat_list();
        }
        scheduled
    }

    pub(crate) fn chat_is_manually_unread(&self, chat: &wr::JID) -> bool {
        self.timeline
            .get(chat)
            .is_some_and(|state| state.manually_unread)
    }

    pub(crate) fn chat_is_unread(&self, chat: &wr::JID) -> bool {
        self.chat_is_manually_unread(chat) || self.pending_chat_activity(chat).0 > 0
    }

    pub(crate) fn set_manual_unread(&mut self, chat: &wr::JID, unread: bool) {
        if self.chat_is_manually_unread(chat) != unread {
            self.timeline
                .entry(chat.clone())
                .or_default()
                .manually_unread = unread;
            self.invalidate_chat_list();
        }
    }

    fn schedule_chat_read_state(
        &mut self,
        chat: &wr::JID,
        id: Option<&wr::MessageId>,
        read: bool,
    ) -> bool {
        if Self::is_status_chat(chat) {
            return false;
        }
        let message = id.and_then(|id| self.messages.get(id));
        if id.is_some() && message.is_none() {
            return false;
        }
        let accepted = self.chat_read_sync.schedule_state(
            chat,
            id,
            read,
            message.map_or(0, |message| message.info.timestamp),
            message.is_some_and(|message| message.info.is_from_me),
            message
                .filter(|_| Self::is_group_chat(chat))
                .map(|message| &message.info.sender),
        );
        self.message_action_diagnostics.trace_queue(accepted, read);
        accepted
    }

    pub(crate) fn toggle_chat_read(&mut self, chat: &wr::JID) {
        use super::super::message_action_diagnostics::ReadSyncStage;
        self.trace_read_sync_snapshot(chat, ReadSyncStage::ToggleBefore);
        let read = self.chat_is_unread(chat);
        let latest = self
            .chat_messages
            .get(chat)
            .and_then(|messages| messages.last())
            .cloned();
        if !self.schedule_chat_read_state(chat, latest.as_ref(), read) {
            self.unavailable("Could not schedule chat read/unread sync");
            self.trace_read_sync_snapshot(chat, ReadSyncStage::ToggleAfter);
            return;
        }
        if read {
            self.mark_chat_read_at_latest_inner(chat, false);
        } else {
            self.set_manual_unread(chat, true);
        }
        self.trace_read_sync_snapshot(chat, ReadSyncStage::ToggleAfter);
    }

    /// Opening acknowledges only the manual flag, at the existing read boundary.
    /// It neither consumes later messages nor changes the restored selection anchor.
    pub(crate) fn clear_manual_unread_on_open(&mut self) {
        let Some(chat) = self
            .open_chat()
            .filter(|chat| self.chat_is_manually_unread(chat))
        else {
            return;
        };
        let cursor = self
            .timeline
            .get(&chat)
            .and_then(|state| state.last_read_message.clone());
        if self.schedule_chat_read_state(&chat, cursor.as_ref(), true) {
            self.set_manual_unread(&chat, false);
        } else {
            self.unavailable("Could not schedule chat read/unread sync");
        }
    }

    pub(crate) fn shutdown_read_sync_worker(&mut self) {
        self.chat_read_sync.shutdown();
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
        if chat.0.is_empty() {
            record("rejected reason=missing_chat");
            return;
        }
        if !read {
            self.set_manual_unread(&chat, true);
            record("applied manual_unread");
            return;
        }
        if message_id.is_empty()
            && self
                .chat_messages
                .get(&chat)
                .is_none_or(|messages| messages.is_empty())
        {
            self.set_manual_unread(&chat, false);
            record("applied empty_chat_read");
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
        if current.as_ref() == Some(&cursor) {
            self.set_manual_unread(&chat, false);
            record("applied unchanged_cursor");
            return;
        }
        if current.as_ref().is_some_and(|current| *current > cursor) {
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
        self.set_manual_unread(&chat, false);
        let timeline = self.timeline.entry(chat.clone()).or_default();
        timeline.last_read_message = Some(message_id.clone());
        timeline.last_read_at = Some(timestamp);
        timeline.pending_new_messages = pending;
        self.chat_read_cursor.store(StoreChatReadCursor {
            chat: chat.clone(),
            message_id: Some(message_id.clone()),
            timestamp,
        });
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
