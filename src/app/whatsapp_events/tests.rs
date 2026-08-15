use super::*;
use crate::app::test_support::TestApp;
use std::sync::Arc;

#[test]
fn sync_progress_event_updates_history_progress() {
    let mut app = TestApp::new();

    assert!(app.handle_whatsapp_event(wr::Event::SyncProgress(42)));
    assert_eq!(app.history_sync_percent, Some(42));
}

#[test]
fn chat_event_keeps_empty_chat_and_updates_newer_timestamp() {
    let mut app = TestApp::new();
    let jid = wr::JID::from("chat@example.test".to_owned());

    assert!(app.handle_whatsapp_event(wr::Event::Chat {
        jid: jid.clone(),
        last_message_time: 0,
    }));
    assert_eq!(app.chats[&jid].last_message_time, None);

    app.handle_whatsapp_event(wr::Event::Chat {
        jid: jid.clone(),
        last_message_time: 12,
    });
    assert_eq!(app.chats[&jid].last_message_time, Some(12));
}

#[test]
fn reaction_event_is_translated_into_reaction_projection() {
    let mut app = TestApp::new();
    let message_id: wr::MessageId = "message".into();
    let participant = wr::JID::from("participant@example.test".to_owned());

    assert!(app.handle_whatsapp_event(wr::Event::Reaction {
        chat: wr::JID::from("chat@example.test".to_owned()),
        target_message_id: message_id.clone(),
        participant: participant.clone(),
        text: Arc::from("👍"),
        is_from_me: false,
    }));
    assert_eq!(app.reactions[&message_id][&participant].as_ref(), "👍");
}
