use super::*;

fn action(
    id: &str,
    kind: MessageActionKind,
    occurred_at: i64,
    arrival_order: u64,
) -> MessageAction {
    MessageAction {
        action_id: id.into(),
        target_message_id: "target".into(),
        chat: wr::JID::from("chat@example.test".to_owned()),
        sender: wr::JID::from("chat@example.test".to_owned()),
        kind,
        occurred_at,
        arrival_order,
    }
}

#[test]
fn actions_are_ordered_by_timestamp_arrival_and_id() {
    let sorted = sorted_actions(vec![
        action("same-b", MessageActionKind::Delete, 2, 1),
        action("later", MessageActionKind::Delete, 3, 1),
        action("same-a", MessageActionKind::Delete, 2, 1),
        action("earlier-arrival", MessageActionKind::Delete, 2, 0),
    ]);

    assert_eq!(
        sorted
            .iter()
            .map(|item| item.action_id.as_ref())
            .collect::<Vec<_>>(),
        ["earlier-arrival", "same-a", "same-b", "later"]
    );
}

#[test]
fn delete_status_takes_precedence_over_edit_status() {
    let status = status_for_actions(&[
        action(
            "edit",
            MessageActionKind::Edit {
                replacement: "new".into(),
            },
            1,
            1,
        ),
        action("delete", MessageActionKind::Delete, 2, 2),
    ]);

    assert_eq!(
        status,
        MessageStatus {
            edited: true,
            deleted: true
        }
    );
}
