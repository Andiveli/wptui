use std::collections::HashMap;

use super::App;
use whatsrust as wr;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommunityNode {
    pub jid: wr::JID,
    pub name: String,
    pub is_root: bool,
    pub linked_groups: Vec<wr::JID>,
    pub is_joined: bool,
    pub is_default_subgroup: bool,
    pub is_announce: Option<bool>,
    pub participant_count: Option<u32>,
}

fn record_key(
    record: &wr::CommunityInfo,
) -> (bool, String, String, bool, bool, Option<bool>, Option<u32>) {
    (
        !record.is_joined,
        record.name.to_string(),
        record
            .parent_jid
            .as_ref()
            .map(|jid| jid.0.to_string())
            .unwrap_or_default(),
        !record.is_parent,
        !record.is_default_subgroup,
        record.is_announce,
        record.participant_count,
    )
}

fn node_key(node: &CommunityNode) -> (bool, String, bool, bool, Option<bool>, Option<u32>, String) {
    (
        !node.is_joined,
        node.name.clone(),
        !node.is_root,
        !node.is_default_subgroup,
        node.is_announce,
        node.participant_count,
        node.linked_groups
            .iter()
            .map(|jid| jid.0.as_ref())
            .collect::<Vec<_>>()
            .join("\0"),
    )
}

pub(crate) fn dedupe_nodes<'a, I>(nodes: I) -> Vec<&'a CommunityNode>
where
    I: IntoIterator<Item = &'a CommunityNode>,
{
    let mut unique: Vec<&CommunityNode> = Vec::new();
    for node in nodes {
        if let Some(existing) = unique.iter_mut().find(|existing| existing.jid == node.jid) {
            if node_key(node) < node_key(existing) {
                *existing = node;
            }
        } else {
            unique.push(node);
        }
    }
    unique
}

fn merged_records(records: &[wr::CommunityInfo]) -> Vec<wr::CommunityInfo> {
    let mut by_jid = HashMap::new();
    for record in records {
        match by_jid.get_mut(&record.jid) {
            Some(current) if record_key(record) < record_key(current) => {
                *current = record.clone();
            }
            None => {
                by_jid.insert(record.jid.clone(), record.clone());
            }
            _ => {}
        }
    }
    let mut merged = by_jid.into_values().collect::<Vec<_>>();
    merged.sort_by_key(record_key);
    merged
}

fn node(record: &wr::CommunityInfo, linked_groups: Vec<wr::JID>) -> CommunityNode {
    CommunityNode {
        jid: record.jid.clone(),
        name: record.name.to_string(),
        is_root: record.is_parent,
        linked_groups,
        is_joined: record.is_joined,
        is_default_subgroup: record.is_default_subgroup,
        is_announce: record.is_announce,
        participant_count: record.participant_count,
    }
}

impl App<'_> {
    pub fn get_selected_community(&self) -> Option<wr::JID> {
        self.chat_list_state
            .selected()
            .and_then(|index| self.selectable_community_nodes().into_iter().nth(index))
            .map(|node| node.jid.clone())
    }

    pub(crate) fn selected_community_node_jid(&self) -> Option<wr::JID> {
        self.get_selected_community()
    }

    pub(crate) fn select_community_node(&mut self, jid: Option<wr::JID>) {
        let selectable = self.selectable_community_nodes();
        let selected = jid
            .and_then(|jid| selectable.iter().position(|node| node.jid == jid))
            .or_else(|| (!selectable.is_empty()).then_some(0));
        self.chat_list_state.select(selected);
    }

    pub(crate) fn selectable_community_nodes(&self) -> Vec<&CommunityNode> {
        dedupe_nodes(
            self.communities
                .iter()
                .filter(|node| !node.is_root && node.is_joined),
        )
    }

    pub(crate) fn build_community_nodes(records: &[wr::CommunityInfo]) -> Vec<CommunityNode> {
        let records = merged_records(records);
        let mut roots = records
            .iter()
            .filter(|record| record.is_parent)
            .collect::<Vec<_>>();
        roots.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.jid.0.cmp(&right.jid.0))
        });
        let mut nodes = Vec::new();
        for root in roots {
            let mut children = records
                .iter()
                .filter(|record| record.parent_jid.as_ref() == Some(&root.jid))
                .collect::<Vec<_>>();
            children.sort_by(|left, right| {
                left.name
                    .cmp(&right.name)
                    .then_with(|| left.jid.0.cmp(&right.jid.0))
            });
            nodes.push(node(
                root,
                children.iter().map(|child| child.jid.clone()).collect(),
            ));
            nodes.extend(children.into_iter().map(|child| node(child, Vec::new())));
        }
        nodes
    }
}

#[cfg(test)]
mod tests;
