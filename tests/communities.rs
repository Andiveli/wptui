mod common;

use common::TestApp;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use whatsrust as wr;
use wp_tui::{
    app::{
        Chat, CommunityNode,
        actions::{AppAction, FocusPane, Section},
    },
    ui::message_list::render_messages,
};

fn jid(value: &str) -> wr::JID {
    wr::JID::from(value.to_owned())
}

fn message(chat: &wr::JID, id: &str, timestamp: i64, text: &str, read_by: u16) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: jid("member@s.whatsapp.net"),
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by,
            forwarding: Default::default(),
        },
        message: wr::MessageContent::Text(text.into()),
    }
}

#[test]
fn opening_selected_community_renders_the_existing_chat_context() {
    let mut test_app = TestApp::new();
    let group = wr::JID::from("child@g.us".to_owned());
    test_app.communities = vec![CommunityNode {
        jid: group.clone(),
        name: "Announcements".into(),
        is_root: false,
        linked_groups: Vec::new(),
    }];
    test_app.selected_section = Section::Communities;
    test_app.focus_pane = FocusPane::ChatList;
    test_app.chat_list_state.select(Some(0));
    test_app.add_message(wr::Message {
        info: wr::MessageInfo {
            id: "community-message".into(),
            chat: group.clone(),
            sender: group.clone(),
            timestamp: 1_700_000_000,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: wr::MessageContent::Text("community context".into()),
    });

    test_app.dispatch_action(AppAction::OpenChat);
    assert_eq!(test_app.open_chat(), Some(group));
    assert_eq!(test_app.focus_pane, FocusPane::Conversation);

    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    terminal
        .draw(|frame| {
            render_messages(frame, &mut test_app, Rect::new(0, 0, 40, 8));
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rendered = (0..buffer.area().height)
        .map(|y| {
            (0..buffer.area().width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("community context"), "{rendered}");
    assert!(rendered.contains("↓ 1 new messages"), "{rendered}");
}

#[test]
fn community_row_collapses_linked_groups_and_preserves_latest_target() {
    let mut app = TestApp::new();
    let root = jid("community@g.us");
    let first = jid("first@g.us");
    let second = jid("second@g.us");
    app.contacts.insert(first.clone(), "First Group".into());
    app.contacts.insert(second.clone(), "Second Group".into());
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
        jid: root,
        name: "Project Team".into(),
        is_root: true,
        linked_groups: vec![first.clone(), second.clone()],
    }];
    app.add_message(message(&first, "one", 10, "old", 0));
    app.add_message(message(&second, "two", 20, "new", 0));

    let rows = app.chat_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "Project Team");
    assert_eq!(rows[0].target, second);
    assert!(app.chats.contains_key(&first));
    assert!(app.chats.contains_key(&second));
}

#[test]
fn community_search_matches_community_and_member_names_without_hiding_normal_chats() {
    let mut app = TestApp::new();
    let group = jid("group@g.us");
    let direct = jid("direct@s.whatsapp.net");
    app.contacts.insert(group.clone(), "Release Crew".into());
    app.contacts.insert(direct.clone(), "Alice".into());
    app.chats.insert(
        group.clone(),
        Chat {
            jid: group.clone(),
            last_message_time: Some(10),
        },
    );
    app.chats.insert(
        direct.clone(),
        Chat {
            jid: direct.clone(),
            last_message_time: Some(5),
        },
    );
    app.sorted_chats = vec![group.clone(), direct.clone()];
    app.communities = vec![CommunityNode {
        jid: jid("community@g.us"),
        name: "Engineering".into(),
        is_root: true,
        linked_groups: vec![group],
    }];

    app.contact_search.input = "engineering".into();
    assert_eq!(app.visible_chat_rows().len(), 1);
    app.contact_search.input = "release".into();
    assert_eq!(app.visible_chat_rows().len(), 1);
    app.contact_search.input = "alice".into();
    assert_eq!(app.visible_chat_rows()[0].target, direct);
}

#[test]
fn latest_message_changes_community_order_and_activation_target() {
    let mut app = TestApp::new();
    let first = jid("first@g.us");
    let second = jid("second@g.us");
    let other = jid("other@s.whatsapp.net");
    for chat in [&first, &second, &other] {
        app.chats.insert(
            chat.clone(),
            Chat {
                jid: chat.clone(),
                last_message_time: Some(1),
            },
        );
    }
    app.sorted_chats = vec![first.clone(), second.clone(), other];
    app.communities = vec![CommunityNode {
        jid: jid("community@g.us"),
        name: "Community".into(),
        is_root: true,
        linked_groups: vec![first.clone(), second.clone()],
    }];
    app.add_message(message(&first, "first-message", 100, "first", 1));
    app.add_message(message(&second, "second-message", 200, "second", 1));
    let row = &app.chat_rows()[0];
    assert_eq!(row.target, second);
    app.chat_list_state.select(Some(0));
    app.dispatch_action(AppAction::OpenChat);
    assert_eq!(app.open_chat(), Some(second));
}

#[test]
fn hierarchy_refresh_keeps_selection_on_a_linked_group_when_target_changes() {
    let mut app = TestApp::new();
    let first = jid("first@g.us");
    let second = jid("second@g.us");
    let root = jid("community@g.us");
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
        jid: root.clone(),
        name: "Community".into(),
        is_root: true,
        linked_groups: vec![first.clone(), second.clone()],
    }];
    app.chat_list_state.select(Some(0));
    let selected = app.get_selected_chat();
    app.refresh_communities(|| {
        Ok(vec![
            wr::CommunityInfo {
                jid: root.clone(),
                name: "Community".into(),
                parent_jid: None,
                is_parent: true,
            },
            wr::CommunityInfo {
                jid: first.clone(),
                name: "First".into(),
                parent_jid: Some(root.clone()),
                is_parent: false,
            },
            wr::CommunityInfo {
                jid: second.clone(),
                name: "Second".into(),
                parent_jid: Some(root),
                is_parent: false,
            },
        ])
    });
    assert_eq!(app.get_selected_chat(), selected);
    assert_eq!(app.chat_rows()[0].target, second);
}
