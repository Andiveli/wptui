use super::super::{
    STATUS_BROADCAST_CHAT,
    test_support::{TestApp, message},
};
use whatsrust as wr;

#[test]
fn status_broadcast_chat_never_appears_in_the_chat_list() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("chat@example.test".to_owned());
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    app.add_message(message(&chat, "c1", 100));
    let mut status = message(&alice, "s1", 200);
    status.info.chat = wr::JID::from(STATUS_BROADCAST_CHAT.to_owned());
    app.add_message(status);
    app.sort_chats();

    assert!(app.sorted_chats.contains(&chat));
    assert!(
        !app.sorted_chats
            .iter()
            .any(|jid| jid.0.as_ref() == STATUS_BROADCAST_CHAT)
    );
}
