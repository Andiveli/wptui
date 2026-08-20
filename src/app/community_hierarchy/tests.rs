use super::{App, CommunityNode, community_group_label};
use crate::app::test_support::TestApp;
use ratatui::widgets::ListState;
use whatsrust as wr;

fn jid(value: &str) -> wr::JID {
    wr::JID::from(value.to_owned())
}

fn community(name: &str, jid_value: &str, is_parent: bool) -> wr::CommunityInfo {
    wr::CommunityInfo {
        jid: jid(jid_value),
        name: name.into(),
        parent_jid: None,
        is_parent,
        is_joined: true,
        is_default_subgroup: false,
        is_announce: Some(false),
        participant_count: None,
    }
}

fn subgroup(
    name: &str,
    jid_value: &str,
    parent_jid: &str,
    is_joined: bool,
    is_default_subgroup: bool,
    is_announce: Option<bool>,
    participant_count: Option<u32>,
) -> wr::CommunityInfo {
    wr::CommunityInfo {
        jid: jid(jid_value),
        name: name.into(),
        parent_jid: Some(jid(parent_jid)),
        is_parent: false,
        is_joined,
        is_default_subgroup,
        is_announce,
        participant_count,
    }
}

fn nodes() -> Vec<CommunityNode> {
    let root = jid("root@g.us");
    let child = jid("child@g.us");
    vec![
        CommunityNode {
            jid: root,
            name: "Community".into(),
            is_root: true,
            linked_groups: vec![child.clone()],
            is_joined: true,
            is_default_subgroup: false,
            is_announce: None,
            participant_count: None,
        },
        CommunityNode {
            jid: child,
            name: "Child".into(),
            is_root: false,
            linked_groups: Vec::new(),
            is_joined: true,
            is_default_subgroup: false,
            is_announce: Some(false),
            participant_count: None,
        },
    ]
}

#[test]
fn build_community_nodes_deduplicates_with_joined_metadata_precedence() {
    let nodes = App::build_community_nodes(&[
        subgroup(
            "Available name",
            "group@g.us",
            "root@g.us",
            false,
            true,
            Some(true),
            Some(8),
        ),
        subgroup(
            "Joined name",
            "group@g.us",
            "root@g.us",
            true,
            false,
            Some(false),
            None,
        ),
        community("Community", "root@g.us", true),
    ]);

    let group = nodes
        .iter()
        .find(|node| node.jid == jid("group@g.us"))
        .unwrap();
    assert_eq!(group.name, "Joined name");
    assert!(group.is_joined);
    assert!(!group.is_default_subgroup);
    assert_eq!(group.is_announce, Some(false));
    assert_eq!(group.participant_count, None);
}

#[test]
fn build_community_nodes_sorts_roots_and_children() {
    let root = jid("root@g.us");
    let child = jid("child@g.us");
    let mut child_record = community("Child", "child@g.us", false);
    child_record.parent_jid = Some(root.clone());
    assert_eq!(
        App::build_community_nodes(&[child_record, community("Community", "root@g.us", true)]),
        vec![
            CommunityNode {
                jid: root,
                name: "Community".into(),
                is_root: true,
                linked_groups: vec![child.clone()],
                is_joined: true,
                is_default_subgroup: false,
                is_announce: Some(false),
                participant_count: None,
            },
            CommunityNode {
                jid: child,
                name: "Child".into(),
                is_root: false,
                linked_groups: Vec::new(),
                is_joined: true,
                is_default_subgroup: false,
                is_announce: Some(false),
                participant_count: None,
            },
        ]
    );
}

#[test]
fn selection_excludes_roots_and_nonjoined_groups() {
    let mut app = TestApp::new();
    let mut nodes = nodes();
    nodes.push(CommunityNode {
        jid: jid("available@g.us"),
        name: "Available".into(),
        is_root: false,
        linked_groups: Vec::new(),
        is_joined: false,
        is_default_subgroup: false,
        is_announce: Some(false),
        participant_count: Some(3),
    });
    app.communities = nodes;
    app.chat_list_state.select(Some(0));
    assert_eq!(app.get_selected_community(), Some(jid("child@g.us")));
    assert_eq!(app.selected_community_node_jid(), Some(jid("child@g.us")));

    app.chat_list_state.select(Some(1));
    assert_eq!(app.get_selected_community(), None);
    assert_eq!(app.selected_community_node_jid(), None);

    app.select_community_node(Some(jid("available@g.us")));
    assert_eq!(app.chat_list_state.selected(), Some(0));
}

#[test]
fn selection_falls_back_to_no_selection_when_no_groups_exist() {
    let mut app = TestApp::new();
    app.communities = vec![CommunityNode {
        jid: jid("root@g.us"),
        name: "Community".into(),
        is_root: true,
        linked_groups: Vec::new(),
        is_joined: true,
        is_default_subgroup: false,
        is_announce: None,
        participant_count: None,
    }];
    app.chat_list_state = ListState::default();
    app.select_community_node(Some(jid("root@g.us")));
    assert_eq!(app.chat_list_state.selected(), None);
}

#[test]
fn announcement_groups_use_the_shared_main_list_label() {
    let mut node = nodes().pop().unwrap();
    node.is_default_subgroup = true;
    assert_eq!(community_group_label(&node), "Announcements");

    node.is_default_subgroup = false;
    node.is_announce = Some(true);
    assert_eq!(community_group_label(&node), "Announcements");
}
