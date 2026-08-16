use crate::app::actions::{ActionNotice, ConversationMode, FocusPane, Section};
use crate::app::test_support::TestApp;
use crate::app::{MessageAction, MessageActionKind};
use whatsrust as wr;

fn message(chat: &wr::JID, sender: &wr::JID, id: &str, is_from_me: bool) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: sender.clone(),
            timestamp: 100,
            forwarding: Default::default(),
            is_from_me,
            quote_id: None,
            read_by: 0,
        },
        message: wr::MessageContent::Text("hello group".into()),
    }
}

fn select(app: &mut TestApp, message: wr::Message) {
    let id = message.info.id.clone();
    app.add_message(message);
    app.message_list_state.set_selected_message(id);
}

#[test]
fn reply_privately_from_group_registers_direct_chat_and_preserves_quote_and_composer() {
    let mut app = TestApp::new();
    let group = wr::JID::from("123@g.us".to_owned());
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    select(&mut app, message(&group, &alice, "g1", false));
    app.open_chat = Some(group);

    app.reply_privately();

    assert_eq!(app.open_chat(), Some(alice.clone()));
    assert!(app.chats.contains_key(&alice));
    assert_eq!(app.selected_section, Section::Chats);
    assert_eq!(app.focus_pane, FocusPane::Conversation);
    assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
    assert_eq!(
        app.composer.quote.as_ref().map(|m| m.info.id.as_ref()),
        Some("g1")
    );
    assert!(matches!(
        app.action_notice,
        Some(ActionNotice::ReplyPrivatelyNamed(_))
    ));
}

#[test]
fn reply_privately_refuses_invalid_selection_and_preserves_chat() {
    let mut app = TestApp::new();
    let group = wr::JID::from("123@g.us".to_owned());
    app.open_chat = Some(group.clone());

    app.reply_privately();

    assert_eq!(app.open_chat(), Some(group));
    assert!(matches!(
        app.action_notice,
        Some(ActionNotice::Unavailable(_))
    ));
}

#[test]
fn reply_privately_refuses_own_outside_group_status_and_deleted_messages() {
    let mut app = TestApp::new();
    let group = wr::JID::from("123@g.us".to_owned());
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    let dm = alice.clone();

    select(&mut app, message(&group, &alice, "own", true));
    app.open_chat = Some(group.clone());
    app.reply_privately();
    assert!(matches!(
        app.action_notice,
        Some(ActionNotice::Unavailable(_))
    ));

    select(&mut app, message(&dm, &dm, "dm", false));
    app.open_chat = Some(dm.clone());
    app.reply_privately();
    assert!(matches!(
        app.action_notice,
        Some(ActionNotice::Unavailable(_))
    ));

    select(&mut app, message(&group, &alice, "status", false));
    app.open_chat = Some(group.clone());
    app.selected_section = Section::Status;
    app.reply_privately();
    assert!(matches!(
        app.action_notice,
        Some(ActionNotice::Unavailable(_))
    ));

    select(&mut app, message(&group, &alice, "deleted", false));
    app.selected_section = Section::Chats;
    app.apply_message_action(MessageAction {
        action_id: "delete".into(),
        target_message_id: "deleted".into(),
        chat: group.clone(),
        sender: alice,
        kind: MessageActionKind::Delete,
        occurred_at: 101,
        arrival_order: 1,
    });
    app.reply_privately();
    assert!(matches!(
        app.action_notice,
        Some(ActionNotice::Unavailable(_))
    ));
}

#[test]
fn reply_privately_refuses_message_from_another_chat() {
    let mut app = TestApp::new();
    let open_group = wr::JID::from("123@g.us".to_owned());
    let other_group = wr::JID::from("456@g.us".to_owned());
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    select(&mut app, message(&other_group, &alice, "other", false));
    app.open_chat = Some(open_group);

    app.reply_privately();

    assert!(matches!(
        app.action_notice,
        Some(ActionNotice::Unavailable(_))
    ));
}
