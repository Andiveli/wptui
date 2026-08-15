use super::App;
use crate::app::test_support::TestApp;
use whatsrust as wr;

#[test]
fn is_group_chat_detects_group_jids() {
    assert!(App::is_group_chat(&wr::JID::from("123@g.us".to_owned())));
    assert!(!App::is_group_chat(&wr::JID::from(
        "123@s.whatsapp.net".to_owned()
    )));
    assert!(!App::is_group_chat(&wr::JID::from(
        "status@broadcast".to_owned()
    )));
}

#[test]
fn open_chat_by_jid_registers_recipient_in_chat_list() {
    let mut app = TestApp::new();
    let recipient = wr::JID::from("alice@s.whatsapp.net".to_owned());

    app.open_chat_by_jid(recipient.clone());

    assert_eq!(app.open_chat(), Some(recipient.clone()));
    assert!(app.chats.contains_key(&recipient));
    assert!(app.sorted_chats.contains(&recipient));
}
