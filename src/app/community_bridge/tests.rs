use super::super::test_support::TestApp;
use crate::app::Section;
use whatsrust as wr;

fn community(name: &str, jid: &str, is_parent: bool) -> wr::CommunityInfo {
    wr::CommunityInfo {
        jid: jid.to_owned().into(),
        name: name.into(),
        parent_jid: None,
        is_parent,
        is_joined: true,
        is_default_subgroup: false,
        is_announce: Some(false),
        participant_count: None,
    }
}

#[test]
fn communities_load_after_readiness_event() {
    let mut app = TestApp::new();
    app.refresh_communities(|| Ok(vec![community("Community", "root@g.us", true)]));

    assert_eq!(app.communities.len(), 1);
    assert!(!app.communities_unavailable);
}

#[test]
fn communities_refresh_replaces_snapshot() {
    let mut app = TestApp::new();
    app.refresh_communities(|| Ok(vec![community("Old", "old@g.us", true)]));
    app.refresh_communities(|| Ok(vec![community("New", "new@g.us", true)]));

    assert_eq!(app.communities[0].name, "New");
    assert_eq!(app.communities[0].jid, wr::JID::from("new@g.us".to_owned()));
}

#[test]
fn successful_empty_communities_clears_snapshot() {
    let mut app = TestApp::new();
    app.refresh_communities(|| Ok(vec![community("Old", "old@g.us", true)]));
    app.refresh_communities(|| Ok(Vec::new()));

    assert!(app.communities.is_empty());
    assert!(!app.communities_unavailable);
}

#[test]
fn transient_communities_failure_preserves_last_successful_snapshot() {
    let mut app = TestApp::new();
    app.refresh_communities(|| Ok(vec![community("Known", "known@g.us", true)]));
    app.refresh_communities(|| Err(wr::CommunitiesError::BridgeUnavailable));

    assert_eq!(app.communities[0].name, "Known");
    assert!(!app.communities_unavailable);
}

#[test]
fn communities_refresh_preserves_explicit_node_across_reorder_and_target_change() {
    let mut app = TestApp::new();
    let selected: wr::JID = "selected@g.us".to_owned().into();
    let other: wr::JID = "other@g.us".to_owned().into();
    app.selected_section = Section::Communities;
    app.refresh_communities(|| {
        Ok(vec![
            community("Selected", "selected@g.us", true),
            wr::CommunityInfo {
                jid: other.clone(),
                name: "Other".into(),
                parent_jid: Some(selected.clone()),
                is_parent: false,
                is_joined: true,
                is_default_subgroup: false,
                is_announce: Some(false),
                participant_count: None,
            },
        ])
    });
    app.chat_list_state.select(Some(0));
    app.refresh_communities(|| {
        Ok(vec![
            community("Aardvark", "aardvark@g.us", true),
            community("Selected", "selected@g.us", true),
            wr::CommunityInfo {
                jid: other.clone(),
                name: "Other".into(),
                parent_jid: Some(selected),
                is_parent: false,
                is_joined: true,
                is_default_subgroup: false,
                is_announce: Some(false),
                participant_count: None,
            },
        ])
    });

    assert_eq!(app.selected_community_node_jid(), Some(other.clone()));
    assert_eq!(app.get_selected_community(), Some(other));
}

#[test]
fn communities_refresh_falls_back_when_selected_node_is_removed() {
    let mut app = TestApp::new();
    app.selected_section = Section::Communities;
    app.refresh_communities(|| {
        Ok(vec![
            community("Removed", "removed@g.us", true),
            community("Kept", "kept@g.us", true),
        ])
    });
    app.chat_list_state.select(Some(0));
    app.refresh_communities(|| Ok(vec![community("Kept", "kept@g.us", true)]));

    assert_eq!(app.chat_list_state.selected(), None);
    assert_eq!(app.selected_community_node_jid(), None);
}
