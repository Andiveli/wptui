use std::collections::HashSet;

use super::contact_avatars::AvatarTarget;
use super::{
    App, CommunityNode,
    community_hierarchy::{community_group_label, dedupe_nodes, is_announcement_group},
};
use whatsrust as wr;

/// Presentation-only Chats row. `target` and `members` are always real chat
/// JIDs; a community never becomes a persisted or addressable chat.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatRow {
    pub label: String,
    pub members: Vec<wr::JID>,
    pub target: wr::JID,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContactRow {
    Chat(ChatRow),
    Available {
        name: String,
        jid: Option<wr::JID>,
        parent_jid: Option<wr::JID>,
        participant_count: Option<u32>,
    },
    Header(String),
    Action(String),
}

impl ContactRow {
    pub fn target(&self) -> Option<&wr::JID> {
        match self {
            Self::Chat(row) => Some(&row.target),
            Self::Available { .. } | Self::Header(_) | Self::Action(_) => None,
        }
    }

    pub fn avatar_target(&self) -> Option<AvatarTarget> {
        match self {
            Self::Chat(row) => Some(AvatarTarget::Contact {
                jid: row.target.clone(),
            }),
            Self::Available {
                jid: Some(jid),
                parent_jid,
                ..
            } => Some(AvatarTarget::CommunityGroup {
                jid: jid.clone(),
                parent_jid: parent_jid.clone(),
                is_joined: false,
            }),
            Self::Available { jid: None, .. } => None,
            Self::Header(_) | Self::Action(_) => None,
        }
    }
}

fn chat_row(label: String, jid: &wr::JID) -> ContactRow {
    ContactRow::Chat(ChatRow {
        label,
        members: vec![jid.clone()],
        target: jid.clone(),
    })
}

fn first_chat_index(rows: &[ContactRow]) -> Option<usize> {
    rows.iter().position(|row| row.target().is_some())
}

impl App<'_> {
    fn linked_group_nodes(&self, root: &CommunityNode) -> Vec<CommunityNode> {
        let mut groups = Vec::new();
        for jid in &root.linked_groups {
            if groups.iter().any(|group: &CommunityNode| group.jid == *jid) {
                continue;
            }
            if let Some(node) = dedupe_nodes(
                self.communities
                    .iter()
                    .filter(|node| node.jid == *jid && !node.is_root),
            )
            .into_iter()
            .next()
            {
                groups.push(node.clone());
            } else {
                groups.push(CommunityNode {
                    jid: jid.clone(),
                    name: self.contact_name(jid).to_string(),
                    is_root: false,
                    linked_groups: Vec::new(),
                    is_joined: true,
                    is_default_subgroup: false,
                    is_announce: None,
                    participant_count: None,
                });
            }
        }
        groups
    }

    pub fn get_selected_chat(&self) -> Option<wr::JID> {
        self.selected_contact_row()?.target().cloned()
    }

    pub fn visible_contact_rows(&self) -> Vec<ContactRow> {
        if self.community_detail.is_some() {
            return self.community_detail_rows();
        }
        self.visible_chat_rows()
            .into_iter()
            .map(ContactRow::Chat)
            .collect()
    }

    fn selected_contact_row(&self) -> Option<ContactRow> {
        self.chat_list_state
            .selected()
            .and_then(|index| self.visible_contact_rows().into_iter().nth(index))
    }

    pub fn community_detail_rows(&self) -> Vec<ContactRow> {
        let Some(root) = self.community_detail.as_ref().and_then(|jid| {
            dedupe_nodes(self.communities.iter().filter(|node| node.jid == *jid))
                .into_iter()
                .next()
        }) else {
            return Vec::new();
        };
        let mut groups = self.linked_group_nodes(root);
        groups.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.jid.0.cmp(&right.jid.0))
        });

        let announcement = groups
            .iter()
            .filter(|group| is_announcement_group(group) && self.chats.contains_key(&group.jid))
            .min_by_key(|group| {
                (
                    !group.is_default_subgroup,
                    group.is_announce != Some(true),
                    group.name.clone(),
                    group.jid.0.to_string(),
                )
            });

        let mut rows = Vec::new();
        if let Some(announcement) = announcement {
            rows.push(chat_row(
                community_group_label(announcement),
                &announcement.jid,
            ));
        }
        rows.push(ContactRow::Header("Groups you're in".into()));
        rows.extend(
            groups
                .iter()
                .filter(|group| {
                    group.is_joined
                        && self.chats.contains_key(&group.jid)
                        && announcement.is_none_or(|selected| selected.jid != group.jid)
                })
                .map(|group| chat_row(group.name.clone(), &group.jid)),
        );
        rows.extend([
            ContactRow::Action("Add group".into()),
            ContactRow::Header("Groups you can join".into()),
        ]);
        rows.extend(groups.iter().filter_map(|group| {
            (!group.is_joined && announcement.is_none_or(|selected| selected.jid != group.jid))
                .then(|| ContactRow::Available {
                    name: group.name.clone(),
                    jid: (!group.jid.0.is_empty()).then(|| group.jid.clone()),
                    parent_jid: self.community_detail.clone(),
                    participant_count: group.participant_count,
                })
        }));
        rows
    }

    pub fn selected_community_contact(&self) -> Option<wr::JID> {
        if self.community_detail.is_some() {
            return None;
        }
        let ContactRow::Chat(row) = self.selected_contact_row()? else {
            return None;
        };
        dedupe_nodes(self.communities.iter().filter(|node| node.is_root))
            .into_iter()
            .find(|root| {
                let members = self
                    .linked_group_nodes(root)
                    .into_iter()
                    .filter(|group| self.chats.contains_key(&group.jid) && group.is_joined)
                    .map(|group| group.jid);
                row.label == root.name
                    && members.clone().count() == row.members.len()
                    && members.clone().all(|jid| row.members.contains(&jid))
            })
            .map(|node| node.jid.clone())
    }

    pub(crate) fn open_community_detail(&mut self, root: wr::JID) {
        self.community_detail = Some(root);
        self.chat_list_state
            .select(first_chat_index(&self.community_detail_rows()));
    }

    pub(crate) fn close_community_detail(&mut self) {
        self.community_detail = None;
        self.chat_list_state
            .select(first_chat_index(&self.visible_contact_rows()));
    }

    pub fn chat_rows(&self) -> Vec<ChatRow> {
        let mut grouped = HashSet::new();
        let community_metadata = self
            .communities
            .iter()
            .map(|node| node.jid.clone())
            .collect::<HashSet<_>>();
        let mut rows = Vec::new();
        for community in dedupe_nodes(self.communities.iter().filter(|node| node.is_root)) {
            let members = self
                .linked_group_nodes(community)
                .into_iter()
                .filter(|node| self.chats.contains_key(&node.jid) && node.is_joined)
                .map(|node| node.jid)
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
