use std::sync::{Arc, Mutex};

use super::super::{STATUS_BROADCAST_CHAT, test_support::TestApp};
use whatsrust as wr;

fn message(chat: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: chat.clone(),
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
