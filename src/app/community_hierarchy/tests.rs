use super::{App, CommunityNode};
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
        },
        CommunityNode {
            jid: child,
            name: "Child".into(),
            is_root: false,
            linked_groups: Vec::new(),
        },
    ]
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
            },
            CommunityNode {
                jid: child,
                name: "Child".into(),
                is_root: false,
                linked_groups: Vec::new(),
            },
        ]
    );
}

#[test]
fn selection_ignores_roots_and_preserves_explicit_node_or_falls_back() {
    let mut app = TestApp::new();
    app.communities = nodes();
    app.chat_list_state.select(Some(0));
    assert_eq!(app.get_selected_community(), Some(jid("child@g.us")));
    assert_eq!(app.selected_community_node_jid(), Some(jid("child@g.us")));

    app.select_community_node(Some(jid("missing@g.us")));
    assert_eq!(app.chat_list_state.selected(), Some(0));
    app.select_community_node(None);
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
    }];
    app.chat_list_state = ListState::default();
    app.select_community_node(Some(jid("root@g.us")));
    assert_eq!(app.chat_list_state.selected(), None);
}
