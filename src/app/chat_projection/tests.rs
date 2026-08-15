use super::super::{Chat, CommunityNode, test_support::TestApp};
use whatsrust as wr;

fn jid(value: &str) -> wr::JID {
    wr::JID::from(value.to_owned())
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
