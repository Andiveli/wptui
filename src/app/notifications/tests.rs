use std::sync::Arc;

use super::*;

fn message(chat: &str, text: &str) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: "id".into(),
            chat: chat.to_owned().into(),
            sender: chat.to_owned().into(),
            timestamp: 1,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: wr::MessageContent::Text(text.into()),
    }
}

#[test]
fn eligibility_skips_status_and_own_messages() {
    assert!(notification_eligibility(&message("chat@g.us", "incoming")));

    let mut own = message("chat@g.us", "own");
    own.info.is_from_me = true;
    assert!(!notification_eligibility(&own));
    assert!(!notification_eligibility(&message(
        STATUS_BROADCAST_CHAT,
        "status"
    )));
}

#[test]
fn mute_policy_excludes_expired_boundary() {
    assert!(notification_is_muted(true, 1_001, 1_000));
    assert!(!notification_is_muted(true, 1_000, 1_000));
    assert!(!notification_is_muted(false, 2_000, 1_000));
}

#[test]
fn projection_preserves_untrusted_message_text() {
    let projection =
        notification_projection(&message("alice@s.whatsapp.net", "ignored"), "Alice".into());
    assert_eq!(projection.summary.as_ref(), "Alice");
    assert_eq!(projection.body, "ignored");

    let message = wr::Message {
        message: wr::MessageContent::Text("こんにちは\n\"quoted\"\u{0007}".into()),
        ..message("alice@s.whatsapp.net", "ignored")
    };
    let projection = notification_projection(&message, Arc::from("名前\n\"sender\""));
    assert_eq!(projection.summary.as_ref(), "名前\n\"sender\"");
    assert_eq!(projection.body, "こんにちは\n\"quoted\"\u{0007}");
}

struct FixedClock(Option<i64>);

impl Clock for FixedClock {
    fn unix_seconds(&self) -> Option<i64> {
        self.0
    }
}

#[test]
fn now_or_uses_clock_or_fallback() {
    assert_eq!(now_or(0, &FixedClock(Some(42))), 42);
    assert_eq!(now_or(7, &FixedClock(None)), 7);
}
