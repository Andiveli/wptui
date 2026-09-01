use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
};

use super::super::{
    Chat, MessageReactionWritePort, RecordMessageReaction, STATUS_BROADCAST_CHAT,
    chat_store::write_port::{ChatStoreWritePort, PersistMessage},
    test_support::{FakeChatReadCursorPort, TestApp},
};
use whatsrust as wr;

struct RecordingChatStoreWritePort {
    persisted: Arc<Mutex<Vec<(Chat, wr::Message)>>>,
}

impl ChatStoreWritePort for RecordingChatStoreWritePort {
    fn persist(&self, command: super::super::chat_store::write_port::PersistChatMessage) {
        self.persisted
            .lock()
            .unwrap()
            .push((command.chat, command.message));
    }

    fn persist_message(&self, command: PersistMessage) {
        let message = command.message;
        self.persisted.lock().unwrap().push((
            Chat {
                jid: message.info.chat.clone(),
                last_message_time: Some(message.info.timestamp),
            },
            message,
        ));
    }
}

struct RecordingMessageReactionWritePort {
    recorded: Arc<Mutex<Vec<RecordMessageReaction>>>,
}

impl MessageReactionWritePort for RecordingMessageReactionWritePort {
    fn record(&self, command: RecordMessageReaction) {
        self.recorded.lock().unwrap().push(command);
    }
}

struct PanickingMessageReactionWritePort;

impl MessageReactionWritePort for PanickingMessageReactionWritePort {
    fn record(&self, _: RecordMessageReaction) {
        panic!("reaction persistence failed");
    }
}

fn message(chat: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: chat.clone(),
            mentions_self: false,
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: wr::MessageContent::Text(id.into()),
    }
}

#[test]
fn notification_eligibility_and_ingestion_continuation_are_preserved() {
    let chat = wr::JID::from("chat@g.us".to_owned());
    let mut app = TestApp::new();
    assert!(app.should_notify(&message(&chat, "incoming", 1)));

    let mut own = message(&chat, "own", 2);
    own.info.is_from_me = true;
    assert!(!app.should_notify(&own));
    assert!(!app.should_notify(&message(
        &wr::JID::from(STATUS_BROADCAST_CHAT.to_owned()),
        "status",
        3,
    )));

    let lookup_calls = Arc::new(Mutex::new(0));
    let calls = lookup_calls.clone();
    assert!(
        app.process_message_with_lookup(message(&chat, "ordinary", 4), false, |_| {
            *calls.lock().unwrap() += 1;
            Default::default()
        })
    );
    assert_eq!(*lookup_calls.lock().unwrap(), 1);
    assert!(app.messages.contains_key("ordinary"));
}

#[test]
fn sync_messages_skip_notification_lookup_and_return_false() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@g.us".to_owned());

    assert!(
        !app.process_message_with_lookup(message(&chat, "sync", 5), true, |_| panic!(
            "sync messages must not look up chat settings"
        ))
    );
    assert!(app.messages.contains_key("sync"));
}

#[test]
fn self_mention_semantics_survive_live_and_sync_ingestion() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@g.us".to_owned());

    let mut live = message(&chat, "live-self-mention", 5);
    live.info.mentions_self = true;
    app.process_message_with_lookup(live, false, |_| Default::default());
    assert!(app.messages["live-self-mention"].info.mentions_self);

    let mut sync = message(&chat, "sync-self-mention", 6);
    sync.info.mentions_self = true;
    app.process_message_with_lookup(sync, true, |_| panic!("sync must not notify"));
    assert!(app.messages["sync-self-mention"].info.mentions_self);
}

#[test]
fn live_incoming_messages_update_persistent_timeline_state() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@g.us".to_owned());

    app.process_message_with_lookup(message(&chat, "live", 6), false, |_| Default::default());
    assert_eq!(app.pending_new_messages(&chat), 1);

    app.process_message_with_lookup(message(&chat, "synced", 7), true, |_| Default::default());
    assert_eq!(app.pending_new_messages(&chat), 1);

    let mut own = message(&chat, "own", 8);
    own.info.is_from_me = true;
    app.process_message_with_lookup(own, false, |_| Default::default());
    assert_eq!(app.pending_new_messages(&chat), 0);
}

#[test]
fn live_inbound_message_persists_owned_chat_and_message_after_updating_memory() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@g.us".to_owned());
    let persisted = Arc::new(Mutex::new(Vec::new()));
    app.chat_store_write = Box::new(RecordingChatStoreWritePort {
        persisted: persisted.clone(),
    });

    app.process_message_with_lookup(message(&chat, "live", 6), false, |_| Default::default());

    assert_eq!(app.chats[&chat].last_message_time, Some(6));
    let persisted = persisted.lock().unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].0.jid, chat);
    assert_eq!(persisted[0].0.last_message_time, Some(6));
    assert_eq!(persisted[0].1.info.id.as_ref(), "live");
}

#[test]
fn outgoing_read_receipts_persist_each_increment_without_a_chat_cursor() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@g.us".to_owned());
    let persisted = Arc::new(Mutex::new(Vec::new()));
    let cursor = FakeChatReadCursorPort::default();
    app.chat_store_write = Box::new(RecordingChatStoreWritePort {
        persisted: persisted.clone(),
    });
    app.chat_read_cursor = Box::new(cursor.clone());
    let mut sent = message(&chat, "sent", 6);
    sent.info.is_from_me = true;
    app.add_message(sent);
    cursor.stored.lock().unwrap().clear();

    for expected_read_by in [1, 2] {
        app.apply_receipt(wr::ReceiptKind::Read, chat.clone(), vec!["sent".into()]);
        assert_eq!(app.messages["sent"].info.read_by, expected_read_by);
        assert_eq!(persisted.lock().unwrap().len(), expected_read_by as usize);
    }

    let persisted = persisted.lock().unwrap();
    assert_eq!(persisted[0].1.info.read_by, 1);
    assert_eq!(persisted[1].1.info.read_by, 2);
    assert!(cursor.stored.lock().unwrap().is_empty());
}

#[test]
fn sync_message_refreshes_a_primed_chat_view_once_even_when_chat_timestamp_is_zero() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("history@g.us".to_owned());
    app.chats.insert(
        chat.clone(),
        Chat {
            jid: chat.clone(),
            last_message_time: Some(0),
        },
    );
    app.sorted_chats = vec![chat.clone()];
    app.timeline
        .entry(chat.clone())
        .or_default()
        .pending_new_messages = 2;
    let _ = app.visible_chat_rows();
    let before_time = app.chat_list_view.as_ref().unwrap().items[0]
        .local_time
        .clone();
    let before = app.chat_list_revision;

    assert!(
        !app.process_message_with_lookup(message(&chat, "history", 3600), true, |_| {
            panic!("sync must not notify")
        })
    );

    let view = app.chat_list_view.as_ref().unwrap();
    assert_eq!(app.chat_list_revision, before + 1);
    assert_eq!(view.items[0].preview, "2 unread");
    assert!(view.items[0].local_time.is_some());
    assert_ne!(view.items[0].local_time, before_time);
    assert_eq!(view.rows[0].target, chat);

    let after = app.chat_list_revision;
    assert!(
        !app.process_message_with_lookup(message(&chat, "history", 3600), true, |_| {
            panic!("sync must not notify")
        })
    );
    assert_eq!(app.chat_list_revision, after);
}

#[test]
fn reactions_are_persisted_before_replacing_the_in_memory_value() {
    let mut app = TestApp::new();
    let message_id: wr::MessageId = "reaction-message".into();
    let participant = wr::JID::from("participant@s.whatsapp.net".to_owned());
    let recorded = Arc::new(Mutex::new(Vec::new()));
    app.message_reaction_write = Box::new(RecordingMessageReactionWritePort {
        recorded: recorded.clone(),
    });

    app.apply_reaction(&message_id, participant.clone(), Arc::from("👍"));
    app.apply_reaction(&message_id, participant.clone(), Arc::from("❤️"));

    let recorded = recorded.lock().unwrap();
    assert_eq!(recorded.len(), 2);
    assert_eq!(recorded[0].message_id, message_id);
    assert_eq!(recorded[0].participant, participant);
    assert_eq!(recorded[0].emoji.as_ref(), "👍");
    assert_eq!(recorded[1].message_id, message_id);
    assert_eq!(recorded[1].participant, participant);
    assert_eq!(recorded[1].emoji.as_ref(), "❤️");
    drop(recorded);
    assert_eq!(app.reactions[&message_id][&participant].as_ref(), "❤️");
}

#[test]
fn reaction_removal_is_persisted_before_removing_the_in_memory_entry() {
    let mut app = TestApp::new();
    let message_id: wr::MessageId = "reaction-message".into();
    let participant = wr::JID::from("participant@s.whatsapp.net".to_owned());
    let recorded = Arc::new(Mutex::new(Vec::new()));
    app.message_reaction_write = Box::new(RecordingMessageReactionWritePort {
        recorded: recorded.clone(),
    });
    app.reactions
        .entry(message_id.clone())
        .or_default()
        .insert(participant.clone(), Arc::from("👍"));

    app.apply_reaction(&message_id, participant.clone(), Arc::from(""));

    let recorded = recorded.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].message_id, message_id);
    assert_eq!(recorded[0].participant, participant);
    assert_eq!(recorded[0].emoji.as_ref(), "");
    drop(recorded);
    assert!(!app.reactions.contains_key(&message_id));
}

#[test]
fn reaction_memory_is_unchanged_when_persistence_panics() {
    let mut app = TestApp::new();
    let message_id: wr::MessageId = "reaction-message".into();
    let participant = wr::JID::from("participant@s.whatsapp.net".to_owned());
    app.reactions
        .entry(message_id.clone())
        .or_default()
        .insert(participant.clone(), Arc::from("👍"));
    app.message_reaction_write = Box::new(PanickingMessageReactionWritePort);

    let result = catch_unwind(AssertUnwindSafe(|| {
        app.apply_reaction(&message_id, participant.clone(), Arc::from("❤️"));
    }));

    assert!(result.is_err());
    assert_eq!(app.reactions[&message_id][&participant].as_ref(), "👍");
}

#[test]
fn unread_boundary_uses_message_id_as_equal_timestamp_tiebreaker() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@g.us".to_owned());

    app.process_message_with_lookup(message(&chat, "b", 10), false, |_| Default::default());
    app.mark_chat_read_at_latest(&chat);
    app.process_message_with_lookup(message(&chat, "a", 10), false, |_| Default::default());
    assert_eq!(app.unread_boundary(&chat), None);
    app.process_message_with_lookup(message(&chat, "c", 10), false, |_| Default::default());
    assert_eq!(app.unread_boundary(&chat), Some((1, 10)));
}
