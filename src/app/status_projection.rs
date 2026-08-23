use std::collections::HashMap;

use super::App;
use whatsrust as wr;

/// The synthetic chat that carries WhatsApp status broadcasts. Each message's
/// `info.sender` is the contact who posted the status.
pub const STATUS_BROADCAST_CHAT: &str = "status@broadcast";

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod tests;

impl App<'_> {
    /// Re-derives `status_contacts` from the `status@broadcast` chat and
    /// keeps the list selection valid. Runs on every message arrival.
    pub(crate) fn refresh_status_contacts(&mut self) {
        self.status_contacts = self.derive_status_contacts();
        self.clamp_status_selection();
    }

    fn derive_status_contacts(&self) -> Vec<wr::JID> {
        let mut latest: HashMap<wr::JID, i64> = HashMap::new();
        for id in self
            .chat_messages
            .get(&wr::JID::from(STATUS_BROADCAST_CHAT.to_owned()))
            .into_iter()
            .flatten()
        {
            if let Some(message) = self.messages.get(id)
                && message.info.timestamp
                    > latest
                        .get(&message.info.sender)
                        .copied()
                        .unwrap_or(i64::MIN)
            {
                latest.insert(message.info.sender.clone(), message.info.timestamp);
            }
        }
        let mut senders = latest.into_iter().collect::<Vec<_>>();
        senders.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| left.0.0.as_ref().cmp(right.0.0.as_ref()))
        });
        senders.into_iter().map(|(jid, _)| jid).collect()
    }

    /// Keeps the status-list highlight valid: always selects a row when the
    /// list is non-empty and never selects past the end.
    pub(crate) fn clamp_status_selection(&mut self) {
        match (self.status_contacts.len(), self.status_selection.selected()) {
            (0, _) => self.status_selection.select(None),
            (_, None) => self.status_selection.select(Some(0)),
            (len, Some(selected)) if selected >= len => self.status_selection.select(Some(len - 1)),
            _ => {}
        }
    }

    pub fn selected_status_contact(&self) -> Option<wr::JID> {
        self.status_selection
            .selected()
            .map(|index| self.status_contacts[index].clone())
    }

    /// The statuses of `contact` from the `status@broadcast` chat in
    /// ascending order (newest last, as `sort_chat_messages` leaves it).
    pub fn status_messages(&self, contact: &wr::JID) -> Vec<wr::MessageId> {
        self.chat_messages
            .get(&wr::JID::from(STATUS_BROADCAST_CHAT.to_owned()))
            .into_iter()
            .flatten()
            .filter(|id| {
                self.messages
                    .get(*id)
                    .is_some_and(|message| &message.info.sender == contact)
            })
            .cloned()
            .collect()
    }

    pub fn status_latest_time(&self, contact: &wr::JID) -> Option<i64> {
        self.chat_messages
            .get(&wr::JID::from(STATUS_BROADCAST_CHAT.to_owned()))
            .into_iter()
            .flatten()
            .filter_map(|id| self.messages.get(id))
            .filter(|message| &message.info.sender == contact)
            .map(|message| message.info.timestamp)
            .max()
    }

    pub fn has_unseen_statuses(&self, contact: &wr::JID) -> bool {
        self.status_latest_time(contact).is_some_and(|latest| {
            latest
                > self
                    .status_last_seen
                    .get(contact)
                    .copied()
                    .unwrap_or_default()
        })
    }

    /// Marks the selected contact's statuses as viewed by recording the
    /// latest status timestamp, and resets the message-list scroll state.
    pub fn open_selected_status(&mut self) {
        let Some(contact) = self.selected_status_contact() else {
            return;
        };
        self.open_status_contact = Some(contact.clone());
        if let Some(latest) = self.status_latest_time(&contact) {
            let current = self
                .status_last_seen
                .get(&contact)
                .copied()
                .unwrap_or_default();
            if latest > current {
                match self.db_handler.set_status_last_seen(&contact, latest) {
                    Ok(rows) => {
                        let contact_id =
                            crate::app::message_action_diagnostics::identifier_for_log(&contact.0);
                        self.message_action_diagnostics.record_read_sync(|| {
                            format!(
                                "source=rust event=status_cursor_write rows={rows} contact={contact_id} timestamp={latest}"
                            )
                        });
                    }
                    Err(error) => log::error!(
                        "status cursor write failed contact={} timestamp={latest}: {error}",
                        crate::app::message_action_diagnostics::identifier_for_log(&contact.0)
                    ),
                }
                self.status_last_seen.insert(contact, latest);
            }
        }
        self.message_list_state.reset();
    }
}
