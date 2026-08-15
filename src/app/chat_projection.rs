use std::collections::HashSet;

use super::App;
use whatsrust as wr;

/// Presentation-only Chats row. `target` and `members` are always real chat
/// JIDs; a community never becomes a persisted or addressable chat.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatRow {
    pub label: String,
    pub members: Vec<wr::JID>,
    pub target: wr::JID,
}

impl App<'_> {
    pub fn get_selected_chat(&self) -> Option<wr::JID> {
        let rows = self.visible_chat_rows();
        self.chat_list_state
            .selected()
            .and_then(|index| rows.get(index))
            .map(|row| row.target.clone())
    }

    pub fn chat_rows(&self) -> Vec<ChatRow> {
        let mut grouped = HashSet::new();
        let community_metadata = self
            .communities
            .iter()
            .map(|node| node.jid.clone())
            .collect::<HashSet<_>>();
        let mut rows = Vec::new();
        for community in self.communities.iter().filter(|node| node.is_root) {
            let members = community
                .linked_groups
                .iter()
                .filter(|jid| self.chats.contains_key(*jid))
                .cloned()
                .collect::<Vec<_>>();
            if members.is_empty() {
                continue;
            }
            grouped.extend(members.iter().cloned());
            let target = members
                .iter()
                .max_by_key(|jid| self.chat_recency(jid))
                .expect("non-empty community members")
                .clone();
            rows.push(ChatRow {
                label: community.name.clone(),
                members,
                target,
            });
        }
        rows.extend(
            self.sorted_chats
                .iter()
                .filter(|jid| !grouped.contains(*jid))
                .filter(|jid| !community_metadata.contains(*jid))
                .map(|jid| ChatRow {
                    label: self.contact_name(jid).to_string(),
                    members: vec![(*jid).clone()],
                    target: (*jid).clone(),
                }),
        );
        rows.sort_by(|left, right| {
            self.chat_recency(&right.target)
                .cmp(&self.chat_recency(&left.target))
                .then_with(|| left.label.cmp(&right.label))
                .then_with(|| left.target.0.cmp(&right.target.0))
        });
        rows
    }

    pub fn visible_chat_rows(&self) -> Vec<ChatRow> {
        let query = self.contact_search.input.to_lowercase();
        self.chat_rows()
            .into_iter()
            .filter(|row| {
                query.is_empty()
                    || row.label.to_lowercase().contains(&query)
                    || row
                        .members
                        .iter()
                        .any(|jid| self.contact_name(jid).to_lowercase().contains(&query))
            })
            .collect()
    }

    pub fn chat_row_item(&self, row: &ChatRow) -> crate::ui::contact_list::ContactListItem {
        crate::ui::contact_list::ContactListItem::from_row(self, row)
    }

    fn chat_recency(&self, jid: &wr::JID) -> i64 {
        self.chat_messages
            .get(jid)
            .into_iter()
            .flatten()
            .filter_map(|id| self.messages.get(id).map(|message| message.info.timestamp))
            .max()
            .or_else(|| self.chats.get(jid).and_then(|chat| chat.last_message_time))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
