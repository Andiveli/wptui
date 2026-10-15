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

fn file_message(id: &str, path: &str) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: wr::JID::from("chat@example.test".to_owned()),
            sender: wr::JID::from("sender@example.test".to_owned()),
            mentions_self: false,
            timestamp: 1,
            forwarding: Default::default(),
            is_from_me: false,
            quote_id: None,
            read_by: 0,
        },
        message: wr::MessageContent::File(wr::FileContent {
            kind: wr::FileKind::Image,
            path: path.into(),
            ..Default::default()
        }),
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
fn projection_emits_effect_intents_for_action_before_target() {
    let action = action(
        "edit",
        MessageActionKind::Edit {
            replacement: "replacement".into(),
        },
        2,
        1,
    );

    assert_eq!(
        persistence_intent_for(None),
        MessageActionPersistenceIntent::Record
    );
    let projection =
        project_message_action(None, &[], &action, None, MessageActionPersistence::Inserted);

    assert_eq!(projection.actions, Some(vec![action]));
    assert!(projection.message.is_none());
    assert!(projection.writeback.is_none());
    assert!(projection.media_files.is_empty());
    assert!(!projection.invalidate_image_cache);
    assert!(!projection.invalidate_message_height);
    assert!(projection.invalidate_chat_list);
    assert_eq!(projection.diagnostic.unwrap().projection, "unchanged");
}

#[test]
fn duplicate_actions_emit_no_projection_effects() {
    let action = action("duplicate", MessageActionKind::Delete, 2, 1);
    let projection = project_message_action(
        None,
        &[],
        &action,
        None,
        MessageActionPersistence::DuplicateActionID,
    );

    assert!(projection.actions.is_none());
    assert!(projection.message.is_none());
    assert!(projection.writeback.is_none());
    assert!(projection.media_files.is_empty());
    assert!(!projection.remove_metadata);
    assert!(!projection.invalidate_image_cache);
    assert!(!projection.invalidate_message_height);
    assert!(!projection.invalidate_chat_list);
    assert!(projection.diagnostic.is_none());
}

#[test]
fn duplicate_actions_for_existing_media_messages_emit_no_projection_effects() {
    let duplicate_action = action("duplicate", MessageActionKind::Delete, 2, 1);
    let projection = project_message_action(
        Some(&file_message("target", "image.png")),
        &[action("existing-delete", MessageActionKind::Delete, 1, 1)],
        &duplicate_action,
        None,
        MessageActionPersistence::DuplicateActionID,
    );

    assert!(projection.actions.is_none());
    assert!(projection.message.is_none());
    assert!(projection.writeback.is_none());
    assert!(projection.media_files.is_empty());
    assert!(!projection.remove_metadata);
    assert!(!projection.invalidate_image_cache);
    assert!(!projection.invalidate_message_height);
    assert!(!projection.invalidate_chat_list);
    assert!(projection.diagnostic.is_none());
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
