use super::App;
use whatsrust as wr;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommunityNode {
    pub jid: wr::JID,
    pub name: String,
    pub is_root: bool,
    pub linked_groups: Vec<wr::JID>,
}

impl App<'_> {
    pub fn get_selected_community(&self) -> Option<wr::JID> {
        self.chat_list_state
            .selected()
            .and_then(|index| self.selectable_community_nodes().into_iter().nth(index))
            .map(|node| node.jid.clone())
    }

    pub(crate) fn selected_community_node_jid(&self) -> Option<wr::JID> {
        self.chat_list_state
            .selected()
            .and_then(|index| {
                self.selectable_community_nodes()
                    .into_iter()
                    .nth(index)
                    .or_else(|| {
                        self.communities
                            .iter()
                            .filter(|node| node.is_root)
                            .nth(index)
                    })
            })
            .map(|node| node.jid.clone())
    }

    pub(crate) fn select_community_node(&mut self, jid: Option<wr::JID>) {
        let selected = jid
            .and_then(|jid| {
                self.selectable_community_nodes()
                    .iter()
                    .position(|node| node.jid == jid)
            })
            .or_else(|| (!self.selectable_community_nodes().is_empty()).then_some(0));
        self.chat_list_state.select(selected);
    }

    pub(crate) fn selectable_community_nodes(&self) -> Vec<&CommunityNode> {
        self.communities
            .iter()
            .filter(|node| !node.is_root)
            .collect()
    }

    pub(crate) fn build_community_nodes(records: &[wr::CommunityInfo]) -> Vec<CommunityNode> {
        let mut roots = records
            .iter()
            .filter(|record| record.is_parent)
            .collect::<Vec<_>>();
        roots.sort_by(|a, b| a.name.cmp(&b.name));
        let mut nodes = Vec::new();
        for root in roots {
            nodes.push(CommunityNode {
                jid: root.jid.clone(),
                name: root.name.to_string(),
                is_root: true,
                linked_groups: records
                    .iter()
                    .filter(|record| record.parent_jid.as_ref() == Some(&root.jid))
                    .map(|record| record.jid.clone())
                    .collect(),
            });
            let mut children = records
                .iter()
                .filter(|record| record.parent_jid.as_ref() == Some(&root.jid))
                .collect::<Vec<_>>();
            children.sort_by(|a, b| a.name.cmp(&b.name));
            nodes.extend(children.into_iter().map(|child| CommunityNode {
                jid: child.jid.clone(),
                name: child.name.to_string(),
                is_root: false,
                linked_groups: Vec::new(),
            }));
        }
        nodes
    }
}

#[cfg(test)]
mod tests;
