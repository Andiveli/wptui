use std::collections::{HashMap, HashSet};

use super::{
    App, CommunityNode,
    community_hierarchy::{community_group_label, dedupe_nodes, is_announcement_group},
};
use whatsrust as wr;

#[derive(Clone, Debug, Default)]
pub struct ChatListViewModel {
    pub rows: Vec<ChatRow>,
    pub items: Vec<crate::ui::contact_list::ContactListItem>,
    pub visible_indices: Vec<usize>,
    pub revision: u64,
    pub query: String,
    pub detail_rows: Vec<ContactRow>,
    pub detail_items: Vec<crate::ui::contact_list::ContactListItem>,
    pub detail_jid: Option<wr::JID>,
}

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
    VirtualAnnouncement(ChatRow),
    Available {
        name: String,
        jid: Option<wr::JID>,
        participant_count: Option<u32>,
    },
    Header(String),
    Action(String),
}

impl ContactRow {
    pub fn target(&self) -> Option<&wr::JID> {
        match self {
            Self::Chat(row) | Self::VirtualAnnouncement(row) => Some(&row.target),
            Self::Available { .. } | Self::Header(_) | Self::Action(_) => None,
        }
    }

    pub fn avatar_target(&self) -> Option<&wr::JID> {
        match self {
            Self::Chat(row) => Some(&row.target),
            Self::Available { jid, .. } => jid.as_ref(),
            Self::VirtualAnnouncement(_) | Self::Header(_) | Self::Action(_) => None,
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

    pub fn get_selected_chat(&mut self) -> Option<wr::JID> {
        self.selected_contact_row()?.target().cloned()
    }

    pub fn visible_contact_rows(&mut self) -> Vec<ContactRow> {
        if self.community_detail.is_some() {
            self.ensure_detail_view();
            return self.chat_list_view.as_ref().unwrap().detail_rows.clone();
        }
        self.ensure_chat_list_view();
        self.chat_list_view
            .as_ref()
            .expect("chat view is built")
            .visible_indices
            .iter()
            .map(|index| self.chat_list_view.as_ref().unwrap().rows[*index].clone())
            .map(ContactRow::Chat)
            .collect()
    }

    fn selected_contact_row(&mut self) -> Option<ContactRow> {
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
            rows.push(ContactRow::VirtualAnnouncement(ChatRow {
                label: community_group_label(announcement),
                members: vec![announcement.jid.clone()],
                target: announcement.jid.clone(),
            }));
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
                    participant_count: group.participant_count,
                })
        }));
        rows
    }

    pub fn selected_community_contact(&mut self) -> Option<wr::JID> {
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
        let selected = first_chat_index(&self.community_detail_rows());
        self.chat_list_state.select(selected);
    }

    pub(crate) fn close_community_detail(&mut self) {
        self.community_detail = None;
        let selected = first_chat_index(&self.visible_contact_rows());
        self.chat_list_state.select(selected);
    }

    fn build_chat_rows(&self, recency: &HashMap<wr::JID, i64>) -> Vec<ChatRow> {
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
                .max_by_key(|jid| recency.get(*jid).copied().unwrap_or_default())
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
            recency
                .get(&right.target)
                .copied()
                .unwrap_or_default()
                .cmp(&recency.get(&left.target).copied().unwrap_or_default())
                .then_with(|| left.label.cmp(&right.label))
                .then_with(|| left.target.0.cmp(&right.target.0))
        });
        rows
    }

    fn build_visible_chat_indices(&self, rows: &[ChatRow], query: &str) -> Vec<usize> {
        rows.iter()
            .enumerate()
            .filter(|(_, row)| {
                query.is_empty()
                    || row.label.to_lowercase().contains(query)
                    || row
                        .members
                        .iter()
                        .any(|jid| self.contact_name(jid).to_lowercase().contains(query))
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn latest_messages(&self) -> HashMap<wr::JID, Option<wr::Message>> {
        let mut chat_ids = self.chats.keys().cloned().collect::<HashSet<_>>();
        chat_ids.extend(self.chat_messages.keys().cloned());
        chat_ids.extend(self.sorted_chats.iter().cloned());
        chat_ids
            .into_iter()
            .map(|jid| {
                let latest = self
                    .chat_messages
                    .get(&jid)
                    .into_iter()
                    .flatten()
                    .filter_map(|id| self.messages.get(id))
                    .max_by(|left, right| {
                        left.info
                            .timestamp
                            .cmp(&right.info.timestamp)
                            .then_with(|| left.info.id.cmp(&right.info.id))
                    })
                    .cloned();
                (jid.clone(), latest)
            })
            .collect()
    }

    fn ensure_chat_list_view(&mut self) {
        let query = self.contact_search.input.to_lowercase();
        let needs_semantic = self
            .chat_list_view
            .as_ref()
            .is_none_or(|view| view.revision != self.chat_list_revision);
        if needs_semantic {
            let started = std::time::Instant::now();
            let latest = self.latest_messages();
            let recency = latest
                .keys()
                .map(|jid| {
                    (
                        jid.clone(),
                        latest.get(jid).and_then(Option::as_ref).map_or_else(
                            || {
                                self.chats
                                    .get(jid)
                                    .and_then(|chat| chat.last_message_time)
                                    .unwrap_or_default()
                            },
                            |message| message.info.timestamp,
                        ),
                    )
                })
                .collect::<HashMap<_, _>>();
            let rows = self.build_chat_rows(&recency);
            let items = rows
                .iter()
                .map(|row| {
                    let latest_message = row
                        .members
                        .iter()
                        .filter_map(|jid| latest.get(jid).and_then(Option::as_ref))
                        .max_by(|left, right| {
                            left.info
                                .timestamp
                                .cmp(&right.info.timestamp)
                                .then_with(|| left.info.id.cmp(&right.info.id))
                        });
                    let fallback = row
                        .members
                        .iter()
                        .filter_map(|jid| {
                            self.chats.get(jid).and_then(|chat| chat.last_message_time)
                        })
                        .max();
                    let unread = row
                        .members
                        .iter()
                        .map(|jid| self.pending_new_messages(jid))
                        .sum();
                    crate::ui::contact_list::ContactListItem::from_summary(
                        row,
                        latest_message,
                        fallback,
                        unread,
                    )
                })
                .collect::<Vec<_>>();
            self.chat_list_view = Some(ChatListViewModel {
                rows,
                items,
                visible_indices: Vec::new(),
                revision: self.chat_list_revision,
                query: "\0".into(),
                detail_rows: Vec::new(),
                detail_items: Vec::new(),
                detail_jid: None,
            });
            self.runtime_diagnostics
                .record_chat_view_rebuild(started.elapsed());
        } else {
            self.runtime_diagnostics.record_chat_view_cache_hit();
        }
        let query_changed = self
            .chat_list_view
            .as_ref()
            .is_none_or(|view| view.query != query);
        if query_changed {
            let indices = self.build_visible_chat_indices(
                &self
                    .chat_list_view
                    .as_ref()
                    .expect("chat view is built")
                    .rows,
                &query,
            );
            let view = self.chat_list_view.as_mut().expect("chat view is built");
            view.visible_indices = indices;
            view.query = query;
        }
    }

    pub fn chat_rows(&mut self) -> Vec<ChatRow> {
        self.ensure_chat_list_view();
        self.chat_list_view.as_ref().unwrap().rows.clone()
    }

    pub fn visible_chat_rows(&mut self) -> Vec<ChatRow> {
        self.ensure_chat_list_view();
        let view = self.chat_list_view.as_ref().unwrap();
        view.visible_indices
            .iter()
            .map(|index| view.rows[*index].clone())
            .collect()
    }

    pub(crate) fn cached_contact_view(
        &mut self,
    ) -> (
        Vec<ContactRow>,
        Vec<crate::ui::contact_list::ContactListItem>,
    ) {
        if self.community_detail.is_some() {
            self.ensure_detail_view();
            let view = self.chat_list_view.as_ref().unwrap();
            return (view.detail_rows.clone(), view.detail_items.clone());
        }
        self.ensure_chat_list_view();
        let view = self.chat_list_view.as_ref().unwrap();
        let rows = view
            .visible_indices
            .iter()
            .map(|index| ContactRow::Chat(view.rows[*index].clone()))
            .collect();
        let items = view
            .visible_indices
            .iter()
            .map(|index| view.items[*index].clone())
            .collect();
        (rows, items)
    }

    fn ensure_detail_view(&mut self) {
        self.ensure_chat_list_view();
        let detail_jid = self.community_detail.clone();
        let stale = self.chat_list_view.as_ref().is_none_or(|view| {
            view.revision != self.chat_list_revision || view.detail_jid != detail_jid
        });
        if stale {
            let rows = self.community_detail_rows();
            let items = rows
                .iter()
                .map(|row| crate::ui::contact_list::ContactListItem::from_contact_row(self, row))
                .collect();
            let view = self.chat_list_view.as_mut().unwrap();
            view.detail_rows = rows;
            view.detail_items = items;
            view.detail_jid = detail_jid;
        }
    }

    pub(crate) fn invalidate_chat_list(&mut self) {
        if self.chat_list_mutation_depth > 0 {
            self.chat_list_mutation_pending = true;
        } else {
            self.chat_list_revision = self.chat_list_revision.wrapping_add(1);
        }
    }

    pub(crate) fn with_chat_list_mutation<T>(&mut self, work: impl FnOnce(&mut Self) -> T) -> T {
        self.chat_list_mutation_depth += 1;
        let result = work(self);
        self.chat_list_mutation_depth -= 1;
        if self.chat_list_mutation_depth == 0 && self.chat_list_mutation_pending {
            self.chat_list_mutation_pending = false;
            self.chat_list_revision = self.chat_list_revision.wrapping_add(1);
        }
        result
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn rebuild_chat_list_view_for_tests(&mut self) {
        self.invalidate_chat_list();
        self.ensure_chat_list_view();
    }

    pub fn chat_row_item(&self, row: &ChatRow) -> crate::ui::contact_list::ContactListItem {
        crate::ui::contact_list::ContactListItem::from_row(self, row)
    }
}

#[cfg(test)]
mod tests;
