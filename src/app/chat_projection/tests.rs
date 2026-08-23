use super::super::{Chat, ChatRow, CommunityNode, ContactRow, test_support::TestApp};
use crate::ui::contact_list::ContactListItem;
use whatsrust as wr;

fn jid(value: &str) -> wr::JID {
    wr::JID::from(value.to_owned())
}

fn add_chat(app: &mut TestApp, jid: &wr::JID) {
    app.chats.insert(
        jid.clone(),
        Chat {
            jid: jid.clone(),
            last_message_time: Some(1),
        },
    );
}

fn message(chat: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
    crate::app::test_support::message(chat, id, timestamp)
}

fn unread_message(
    chat: &wr::JID,
    id: &str,
    timestamp: i64,
    mentions_self: bool,
    quote_id: Option<&str>,
) -> wr::Message {
    let mut message = message(chat, id, timestamp);
    message.info.mentions_self = mentions_self;
    message.info.quote_id = quote_id.map(Into::into);
    message
}

#[test]
fn pending_activity_projects_unread_attention_from_cursor_and_metadata() {
    let mut app = TestApp::new();
    let group = jid("group@g.us");
    let direct = jid("alice@s.whatsapp.net");
    add_chat(&mut app, &group);
    add_chat(&mut app, &direct);

    let mut own = message(&group, "own", 10);
    own.info.is_from_me = true;
    app.add_message(own);
    app.add_message(message(&group, "own", 11));
    assert!(app.messages["own"].info.is_from_me);
    app.add_message(message(&group, "someone", 15));
    app.mark_chat_read_at_latest(&group);
    app.add_message(unread_message(&group, "mention", 20, true, None));
    app.add_message(unread_message(&group, "reply", 30, false, Some("own")));
    app.add_message(unread_message(
        &group,
        "other-reply",
        40,
        false,
        Some("someone"),
    ));
    app.add_message(unread_message(&direct, "direct-mention", 20, true, None));

    assert_eq!(app.pending_chat_activity(&group), (3, true));
    assert_eq!(app.pending_chat_activity(&direct), (1, false));
    let group_item = ContactListItem::from_contact_row(
        &app,
        &ContactRow::Chat(ChatRow {
            label: "Group".into(),
            members: vec![group.clone()],
            target: group.clone(),
        }),
    );
    let direct_item = ContactListItem::from_contact_row(
        &app,
        &ContactRow::Chat(ChatRow {
            label: "Alice".into(),
            members: vec![direct.clone()],
            target: direct.clone(),
        }),
    );
    assert!(group_item.unread && group_item.attention);
    assert!(direct_item.unread && !direct_item.attention);

    app.mark_chat_read_at_latest(&group);
    assert_eq!(app.pending_chat_activity(&group), (0, false));
}

#[test]
fn pending_activity_uses_message_id_for_equal_timestamp_boundaries() {
    let mut app = TestApp::new();
    let group = jid("group@g.us");
    add_chat(&mut app, &group);
    app.add_message(message(&group, "a", 10));
    app.mark_chat_read_at_latest(&group);
    app.add_message(unread_message(&group, "b", 10, true, None));

    assert_eq!(app.pending_chat_activity(&group), (1, true));
}

#[test]
fn unread_group_projection_reports_reply_resolution_outcomes_without_raw_ids() {
    let mut app = TestApp::new();
    app.enable_message_action_diagnostics(true);
    let group = jid("group@g.us");
    let other_group = jid("other@g.us");
    let mut own = message(&group, "own-message", 1);
    own.info.is_from_me = true;
    app.add_message(own);
    app.mark_chat_read_at_latest(&group);

    app.add_message(unread_message(&group, "no-quote", 2, false, None));
    app.add_message(unread_message(
        &group,
        "missing-quote",
        3,
        false,
        Some("missing"),
    ));
    let mut wrong_chat_target = message(&other_group, "wrong-chat-target", 4);
    wrong_chat_target.info.is_from_me = true;
    app.add_message(wrong_chat_target);
    app.add_message(unread_message(
        &group,
        "wrong-chat-reply",
        5,
        false,
        Some("wrong-chat-target"),
    ));
    app.add_message(message(&group, "other-target", 6));
    app.add_message(unread_message(
        &group,
        "reply",
        7,
        false,
        Some("own-message"),
    ));
    app.add_message(unread_message(
        &group,
        "other-reply",
        8,
        false,
        Some("other-target"),
    ));

    assert_eq!(app.pending_chat_activity(&group), (6, true));
    let mut output = Vec::new();
    app.write_message_action_diagnostics(&mut output).unwrap();
    let output = String::from_utf8(output).unwrap();

    assert!(!output.contains("quote_present=false"));
    assert!(output.contains("quote_present=true lookup=miss"));
    assert!(output.contains(
        "lookup=hit target_chat_equal=false target_is_from_me=true attention_applied=false"
    ));
    assert!(output.contains(
        "lookup=hit target_chat_equal=true target_is_from_me=false attention_applied=false"
    ));
    assert!(output.contains(
        "lookup=hit target_chat_equal=true target_is_from_me=true attention_applied=true"
    ));
    for raw in [
        "group@g.us",
        "no-quote",
        "missing-quote",
        "wrong-chat-target",
        "own-message",
        "other-target",
    ] {
        assert!(!output.contains(raw));
    }
}

#[test]
fn disabled_unread_group_diagnostics_do_not_change_projection_or_report() {
    let mut app = TestApp::new();
    app.enable_message_action_diagnostics(false);
    let group = jid("group@g.us");
    let mut own = message(&group, "own", 1);
    own.info.is_from_me = true;
    app.add_message(own);
    app.mark_chat_read_at_latest(&group);
    app.add_message(unread_message(&group, "reply", 2, false, Some("own")));

    assert_eq!(app.pending_chat_activity(&group), (1, true));
    let mut output = Vec::new();
    app.write_message_action_diagnostics(&mut output).unwrap();
    assert!(output.is_empty());
}

#[test]
fn community_rows_use_the_most_recent_linked_chat_as_target() {
    let mut app = TestApp::new();
    let first = jid("first@g.us");
    let second = jid("second@g.us");

    app.chats.insert(
        first.clone(),
        Chat {
            jid: first.clone(),
            last_message_time: Some(10),
        },
    );
    app.chats.insert(
        second.clone(),
        Chat {
            jid: second.clone(),
            last_message_time: Some(20),
        },
    );
    app.sorted_chats = vec![first.clone(), second.clone()];
    app.communities = vec![CommunityNode {
        jid: jid("community@g.us"),
        name: "Project Team".into(),
        is_root: true,
        linked_groups: vec![first, second.clone()],
        is_joined: true,
        is_default_subgroup: false,
        is_announce: None,
        participant_count: None,
    }];

    let rows = app.chat_rows();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "Project Team");
    assert_eq!(rows[0].target, second);
}

#[test]
fn selected_chat_follows_the_visible_row_target() {
    let mut app = TestApp::new();
    let chat = jid("alice@s.whatsapp.net");
    app.chats.insert(
        chat.clone(),
        Chat {
            jid: chat.clone(),
            last_message_time: Some(1),
        },
    );
    app.sorted_chats = vec![chat.clone()];
    app.chat_list_state.select(Some(0));

    assert_eq!(app.get_selected_chat(), Some(chat));
}

#[test]
fn community_row_aggregates_groups_and_opens_the_selected_target() {
    let mut app = TestApp::new();
    let first = jid("first@g.us");
    let second = jid("second@g.us");
    let normal = jid("normal@s.whatsapp.net");

    for (chat, timestamp) in [(&first, 10), (&second, 20), (&normal, 5)] {
        app.chats.insert(
            chat.clone(),
            Chat {
                jid: chat.clone(),
                last_message_time: Some(timestamp),
            },
        );
    }
    app.sorted_chats = vec![first.clone(), second.clone(), normal.clone()];
    app.communities = vec![CommunityNode {
        jid: jid("community@g.us"),
        name: "Project Team".into(),
        is_root: true,
        linked_groups: vec![first, second.clone()],
        is_joined: true,
        is_default_subgroup: false,
        is_announce: None,
        participant_count: None,
    }];

    let rows = app.visible_chat_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].label, "Project Team");
    assert_eq!(rows[0].members, vec![jid("first@g.us"), second.clone()]);
    assert_eq!(rows[0].target, second.clone());
    assert_eq!(rows[1].label, "normal@s.whatsapp.net");

    app.chat_list_state.select(Some(0));
    app.open_selected_chat();
    assert_eq!(app.open_chat(), Some(second));
}

#[test]
fn one_unread_linked_group_opens_directly() {
    let mut app = TestApp::new();
    let unread = jid("unread@g.us");
    let quiet = jid("quiet@g.us");
    for chat in [&unread, &quiet] {
        add_chat(&mut app, chat);
    }
    app.sorted_chats = vec![unread.clone(), quiet.clone()];
    app.communities = vec![CommunityNode {
        jid: jid("community@g.us"),
        name: "Project Team".into(),
        is_root: true,
        linked_groups: vec![unread.clone(), quiet],
        is_joined: true,
        is_default_subgroup: false,
        is_announce: None,
        participant_count: None,
    }];
    app.add_message(crate::app::test_support::message(&unread, "new", 2));
    app.chat_list_state.select(Some(0));
    app.dispatch_action(crate::app::actions::AppAction::OpenChat);
    assert_eq!(app.open_chat(), Some(unread));
    assert_eq!(app.community_detail, None);
}
#[test]
fn zero_unread_linked_groups_open_virtual_detail_rows() {
    let mut app = TestApp::new();
    let group = jid("group@g.us");
    add_chat(&mut app, &group);
    app.sorted_chats = vec![group.clone()];
    app.communities = vec![CommunityNode {
        jid: jid("community@g.us"),
        name: "Project Team".into(),
        is_root: true,
        linked_groups: vec![group.clone()],
        is_joined: true,
        is_default_subgroup: false,
        is_announce: None,
        participant_count: None,
    }];
    app.chat_list_state.select(Some(0));
    app.dispatch_action(crate::app::actions::AppAction::OpenChat);
    assert_eq!(app.open_chat(), None);
    assert_eq!(app.community_detail, Some(jid("community@g.us")));
    let rows = app.visible_contact_rows();
    assert!(matches!(
        rows.as_slice(),
        [
            ContactRow::Header(_),
            ContactRow::Chat(ChatRow { target, .. }),
            ContactRow::Action(_),
            ContactRow::Header(_)
        ] if target == &group
    ));
    app.chat_list_state.select(Some(1));
    app.dispatch_action(crate::app::actions::AppAction::OpenChat);
    assert_eq!(app.open_chat(), Some(group));
}

#[test]
fn detail_available_groups_with_group_jids_are_avatar_targets() {
    let mut app = TestApp::new();
    let root = jid("root@g.us");
    let available = jid("available@g.us");
    app.communities = super::super::App::build_community_nodes(&[
        wr::CommunityInfo {
            jid: root.clone(),
            name: "Project Team".into(),
            parent_jid: None,
            is_parent: true,
            is_joined: true,
            is_default_subgroup: false,
            is_announce: None,
            participant_count: None,
        },
        wr::CommunityInfo {
            jid: available.clone(),
            name: "Available".into(),
            parent_jid: Some(root.clone()),
            is_parent: false,
            is_joined: false,
            is_default_subgroup: false,
            is_announce: Some(false),
            participant_count: Some(2),
        },
    ]);
    app.community_detail = Some(root);

    assert_eq!(
        app.visible_contact_rows()[3].avatar_target(),
        Some(&available)
    );
}
#[test]
fn virtual_detail_uses_the_confirmed_announcement_subgroup() {
    let mut app = TestApp::new();
    let root = jid("root@g.us");
    let announcements = jid("announcements@g.us");
    let group = jid("group@g.us");
    let records = vec![
        wr::CommunityInfo {
            jid: root.clone(),
            name: "Project Team".into(),
            parent_jid: None,
            is_parent: true,
            is_joined: true,
            is_default_subgroup: false,
            is_announce: None,
            participant_count: None,
        },
        wr::CommunityInfo {
            jid: announcements.clone(),
            name: "Announcements".into(),
            parent_jid: Some(root.clone()),
            is_parent: false,
            is_joined: true,
            is_default_subgroup: true,
            is_announce: Some(true),
            participant_count: None,
        },
        wr::CommunityInfo {
            jid: group.clone(),
            name: "Engineering".into(),
            parent_jid: Some(root.clone()),
            is_parent: false,
            is_joined: true,
            is_default_subgroup: false,
            is_announce: Some(false),
            participant_count: None,
        },
    ];
    app.communities = super::super::App::build_community_nodes(&records);
    add_chat(&mut app, &announcements);
    add_chat(&mut app, &group);
    app.community_detail = Some(root);

    let rows = app.visible_contact_rows();
    assert!(matches!(
        rows.first(),
        Some(ContactRow::VirtualAnnouncement(ChatRow { label, target, .. }))
            if label == "Announcements"
                && target == &announcements
                && rows.first().unwrap().avatar_target().is_none()
                && rows.first().unwrap().target() == Some(&announcements)
    ));
    assert_eq!(
        ContactListItem::from_contact_row(&app, rows.first().unwrap()).initials,
        "📢"
    );
}

#[test]
fn detail_separates_joined_and_available_groups_without_addressing_available_rows() {
    let mut app = TestApp::new();
    let root = jid("root@g.us");
    let joined = jid("joined@g.us");
    let available = jid("available@g.us");
    app.communities = super::super::App::build_community_nodes(&[
        wr::CommunityInfo {
            jid: root.clone(),
            name: "Project Team".into(),
            parent_jid: None,
            is_parent: true,
            is_joined: true,
            is_default_subgroup: false,
            is_announce: None,
            participant_count: None,
        },
        wr::CommunityInfo {
            jid: joined.clone(),
            name: "Joined".into(),
            parent_jid: Some(root.clone()),
            is_parent: false,
            is_joined: true,
            is_default_subgroup: false,
            is_announce: Some(false),
            participant_count: None,
        },
        wr::CommunityInfo {
            jid: available.clone(),
            name: "Available".into(),
            parent_jid: Some(root.clone()),
            is_parent: false,
            is_joined: false,
            is_default_subgroup: false,
            is_announce: Some(false),
            participant_count: Some(23),
        },
    ]);
    add_chat(&mut app, &joined);
    app.community_detail = Some(root);

    let rows = app.visible_contact_rows();
    assert!(matches!(rows[0], ContactRow::Header(ref title) if title == "Groups you're in"));
    assert!(matches!(
        rows[1],
        ContactRow::Chat(ChatRow { ref label, ref target, .. })
            if label == "Joined" && target == &joined
    ));
    assert!(matches!(rows[3], ContactRow::Header(ref title) if title == "Groups you can join"));
    assert!(matches!(
        rows[4],
        ContactRow::Available { ref name, participant_count: Some(23), .. }
            if name == "Available"
    ));

    app.chat_list_state.select(Some(4));
    app.dispatch_action(crate::app::actions::AppAction::OpenChat);
    assert_eq!(app.open_chat(), None);
    assert_eq!(app.community_detail, Some(jid("root@g.us")));
}

#[test]
fn available_group_count_is_omitted_when_unknown_and_rendered_when_known() {
    let unknown = ContactRow::Available {
        name: "Unknown".into(),
        jid: None,
        participant_count: None,
    };
    let known = ContactRow::Available {
        name: "Known".into(),
        jid: None,
        participant_count: Some(42),
    };
    let app = TestApp::new();

    assert_eq!(
        ContactListItem::from_contact_row(&app, &unknown).preview,
        ""
    );
    assert_eq!(
        ContactListItem::from_contact_row(&app, &known).preview,
        "42 members"
    );
}

#[test]
fn community_search_matches_linked_group_contact_names() {
    let mut app = TestApp::new();
    let group = jid("group@g.us");
    app.contacts
        .insert(group.clone(), "Engineering Alerts".into());
    app.chats.insert(
        group.clone(),
        Chat {
            jid: group.clone(),
            last_message_time: Some(1),
        },
    );
    app.sorted_chats = vec![group];
    app.communities = vec![CommunityNode {
        jid: jid("community@g.us"),
        name: "Project Team".into(),
        is_root: true,
        linked_groups: vec![jid("group@g.us")],
        is_joined: true,
        is_default_subgroup: false,
        is_announce: None,
        participant_count: None,
    }];
    app.contact_search.input = "engineering".into();

    let rows = app.visible_chat_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "Project Team");
}

#[test]
fn projection_deduplicates_duplicate_group_jids_with_joined_precedence() {
    let mut app = TestApp::new();
    let root = jid("community@g.us");
    let group = jid("group@g.us");
    app.chats.insert(
        group.clone(),
        Chat {
            jid: group.clone(),
            last_message_time: Some(1),
        },
    );
    app.sorted_chats = vec![group.clone()];
    app.communities = vec![
        CommunityNode {
            jid: root,
            name: "Project Team".into(),
            is_root: true,
            linked_groups: vec![group.clone(), group.clone()],
            is_joined: true,
            is_default_subgroup: false,
            is_announce: None,
            participant_count: None,
        },
        CommunityNode {
            jid: group.clone(),
            name: "Available".into(),
            is_root: false,
            linked_groups: Vec::new(),
            is_joined: false,
            is_default_subgroup: false,
            is_announce: Some(true),
            participant_count: Some(8),
        },
        CommunityNode {
            jid: group,
            name: "Joined".into(),
            is_root: false,
            linked_groups: Vec::new(),
            is_joined: true,
            is_default_subgroup: false,
            is_announce: Some(false),
            participant_count: None,
        },
    ];

    let rows = app.chat_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].members, vec![jid("group@g.us")]);
}

#[test]
fn root_linked_jid_without_child_is_projected_as_joined_chat() {
    let mut app = TestApp::new();
    let root = jid("community@g.us");
    let group = jid("group@g.us");
    add_chat(&mut app, &group);
    app.sorted_chats = vec![group.clone()];
    app.communities = vec![CommunityNode {
        jid: root.clone(),
        name: "Project Team".into(),
        is_root: true,
        linked_groups: vec![group.clone()],
        is_joined: true,
        is_default_subgroup: false,
        is_announce: None,
        participant_count: None,
    }];

    let rows = app.chat_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].members, vec![group.clone()]);

    app.community_detail = Some(root);
    assert!(matches!(
        app.visible_contact_rows().as_slice(),
        [
            ContactRow::Header(_),
            ContactRow::Chat(ChatRow { label, target, .. }),
            ContactRow::Action(_),
            ContactRow::Header(_),
        ] if label == "group@g.us" && target == &group
    ));
}

#[test]
fn cached_chat_view_reuses_semantic_rows_and_only_refilters_search() {
    let mut app = TestApp::new();
    let chat = jid("cached@example.test");
    add_chat(&mut app, &chat);
    app.sorted_chats = vec![chat.clone()];

    assert_eq!(app.visible_chat_rows().len(), 1);
    let revision = app.chat_list_revision;
    assert_eq!(app.visible_chat_rows().len(), 1);
    assert_eq!(app.chat_list_revision, revision);

    app.contact_search.input = "cached".into();
    assert_eq!(app.visible_chat_rows().len(), 1);
    assert_eq!(app.chat_list_revision, revision);
}

#[test]
fn duplicate_message_does_not_advance_chat_view_revision() {
    let mut app = TestApp::new();
    let chat = jid("duplicate@example.test");
    app.add_message(crate::app::test_support::message(&chat, "message", 1));
    let _ = app.visible_chat_rows();
    let revision = app.chat_list_revision;

    app.add_message(crate::app::test_support::message(&chat, "message", 1));

    assert_eq!(app.chat_list_revision, revision);
}
