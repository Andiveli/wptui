use super::*;
use crate::app::Clock;
use crate::app::test_support::{TestApp, message};
use std::sync::Arc;
#[derive(Debug)]
struct FixedClock(Option<i64>);
impl Clock for FixedClock {
    fn unix_seconds(&self) -> Option<i64> {
        self.0
    }
}
fn app_with_clock(clock: Option<i64>) -> TestApp {
    let mut app = TestApp::new();
    app.clock = Box::new(FixedClock(clock));
    app
}
#[test]
fn actions_replay_in_stable_order_and_delete_wins_the_display_status() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = TestApp::with_database(directory.path());
    let chat = wr::JID::from("chat@example.test".to_owned());
    app.add_message(message(&chat, "target", 1));
    for (id, replacement, order) in [("edit-2", "second", 2), ("edit-1", "first", 1)] {
        app.apply_message_action(MessageAction {
            action_id: id.into(),
            target_message_id: "target".into(),
            chat: chat.clone(),
            sender: chat.clone(),
            kind: MessageActionKind::Edit {
                replacement: replacement.into(),
            },
            occurred_at: 2,
            arrival_order: order,
        });
    }
    app.apply_message_action(MessageAction {
        action_id: "delete".into(),
        target_message_id: "target".into(),
        chat: chat.clone(),
        sender: chat,
        kind: MessageActionKind::Delete,
        occurred_at: 3,
        arrival_order: 3,
    });

    assert!(
        matches!(&app.messages["target"].message, wr::MessageContent::Text(text) if text.as_ref() == DELETED_MESSAGE_TEXT)
    );
    assert_eq!(
        app.message_status(&"target".into()),
        MessageStatus {
            edited: false,
            deleted: true
        }
    );
    assert_eq!(
        app.sorted_message_actions(&"target".into())
            .iter()
            .map(|action| action.action_id.as_ref())
            .collect::<Vec<_>>(),
        ["delete"]
    );
}
#[test]
fn action_before_base_message_is_applied_when_the_base_arrives() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = TestApp::with_database(directory.path());
    let chat = wr::JID::from("chat@example.test".to_owned());
    app.apply_message_action(MessageAction {
        action_id: "edit".into(),
        target_message_id: "target".into(),
        chat: chat.clone(),
        sender: chat.clone(),
        kind: MessageActionKind::Edit {
            replacement: "replacement".into(),
        },
        occurred_at: 2,
        arrival_order: 1,
    });
    app.add_message(message(&chat, "target", 1));

    assert!(
        matches!(&app.messages["target"].message, wr::MessageContent::Text(text) if text.as_ref() == "replacement")
    );
}
#[test]
fn local_edit_is_projected_persisted_and_not_duplicated_by_its_inbound_echo() {
    let directory = tempfile::tempdir().unwrap();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let mut original = message(&chat, "target", 1);
    original.info.is_from_me = true;
    {
        let mut app = TestApp::with_database(directory.path());
        app.db_handler.add_message(&original);
        app.add_message(original.clone());

        app.record_local_message_edit(&original, "replacement".into());
        assert!(
            matches!(&app.messages["target"].message, wr::MessageContent::Text(text) if text.as_ref() == "replacement")
        );
        assert!(app.message_status(&"target".into()).edited);
        assert_eq!(app.message_actions["target"].len(), 1);

        app.apply_message_action(MessageAction {
            action_id: "server-edit".into(),
            target_message_id: "target".into(),
            chat: chat.clone(),
            sender: chat.clone(),
            kind: MessageActionKind::Edit {
                replacement: "replacement".into(),
            },
            occurred_at: 2,
            arrival_order: 2,
        });
        assert_eq!(app.message_actions["target"].len(), 1);
    }

    let mut reloaded = TestApp::with_database(directory.path());
    reloaded.load_data_from_db();
    assert!(
        matches!(&reloaded.messages["target"].message, wr::MessageContent::Text(text) if text.as_ref() == "replacement")
    );
    assert!(reloaded.message_status(&"target".into()).edited);
    assert_eq!(reloaded.message_actions["target"].len(), 1);
}
#[test]
fn local_message_actions_use_injected_clock_and_message_timestamp_fallback() {
    let chat = wr::JID::from("chat@g.us".to_owned());
    let message = message(&chat, "local-action", 77);
    let mut app = app_with_clock(Some(1_700_000_000));
    app.record_local_message_edit(&message, Arc::from("edited"));
    assert_eq!(
        app.message_actions[&message.info.id][0].occurred_at,
        1_700_000_000
    );

    let mut app = app_with_clock(Some(1_700_000_000));
    app.record_local_message_delete(&message);
    assert_eq!(
        app.message_actions[&message.info.id][0].occurred_at,
        1_700_000_000
    );

    let mut app = app_with_clock(None);
    app.record_local_message_delete(&message);
    assert_eq!(app.message_actions[&message.info.id][0].occurred_at, 77);
}
