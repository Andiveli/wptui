use super::super::{
    STATUS_BROADCAST_CHAT,
    test_support::{FakeStatusCursorPort, TestApp},
};
use super::*;

fn status_message(sender: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: wr::JID::from(STATUS_BROADCAST_CHAT.to_owned()),
            forwarding: Default::default(),
            sender: sender.clone(),
            mentions_self: false,
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
        },
        message: wr::MessageContent::Text(id.into()),
    }
}

#[test]
fn status_contacts_are_sorted_by_latest_status_newest_first() {
    let mut app = TestApp::new();
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    let bob = wr::JID::from("bob@s.whatsapp.net".to_owned());

    app.add_message(status_message(&alice, "a-old", 100));
    app.add_message(status_message(&bob, "b-status", 200));
    app.add_message(status_message(&alice, "a-new", 300));

    assert_eq!(app.status_contacts, vec![alice.clone(), bob.clone()]);
    assert_eq!(app.status_latest_time(&alice), Some(300));
    assert_eq!(
        app.status_messages(&alice)
            .iter()
            .map(|id| id.as_ref())
            .collect::<Vec<_>>(),
        ["a-old", "a-new"]
    );
}

#[test]
fn status_contacts_break_equal_recency_ties_by_jid() {
    let mut app = TestApp::new();
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    let bob = wr::JID::from("bob@s.whatsapp.net".to_owned());

    app.add_message(status_message(&bob, "b-status", 100));
    app.add_message(status_message(&alice, "a-status", 100));

    assert_eq!(app.status_contacts, vec![alice.clone(), bob.clone()]);
}

#[test]
fn status_selection_defaults_to_first_contact_and_clamps_when_refreshed() {
    let mut app = TestApp::new();
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    let bob = wr::JID::from("bob@s.whatsapp.net".to_owned());
    app.add_message(status_message(&alice, "a-status", 200));
    app.add_message(status_message(&bob, "b-status", 100));

    assert_eq!(app.status_selection.selected(), Some(0));

    app.status_selection.select(Some(5));
    app.add_message(status_message(&alice, "a-new", 300));
    assert_eq!(app.status_selection.selected(), Some(1));
}

#[test]
fn opening_a_status_marks_the_latest_status_as_seen() {
    let mut app = TestApp::new();
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    app.add_message(status_message(&alice, "a-old", 100));
    app.add_message(status_message(&alice, "a-new", 200));

    assert!(app.has_unseen_statuses(&alice));
    app.open_selected_status();
    assert!(!app.has_unseen_statuses(&alice));

    app.add_message(status_message(&alice, "a-newer", 300));
    assert!(app.has_unseen_statuses(&alice));
    app.open_selected_status();
    assert!(!app.has_unseen_statuses(&alice));
}

#[test]
fn load_data_from_db_hydrates_status_cursors_from_the_port() {
    let mut app = TestApp::new();
    let cursor = FakeStatusCursorPort::default();
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    cursor.loaded.lock().unwrap().push((alice.clone(), 200));
    app.status_cursor = Box::new(cursor);

    app.load_data_from_db();

    assert_eq!(app.status_last_seen.get(&alice), Some(&200));
}

#[test]
fn status_view_cursor_survives_reload() {
    let directory = tempfile::tempdir().unwrap();
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    {
        let mut app = TestApp::with_database(directory.path());
        app.add_message(status_message(&alice, "a-status", 200));
        app.open_selected_status();
        assert_eq!(app.status_last_seen.get(&alice), Some(&200));
    }
    {
        let mut app = TestApp::with_database(directory.path());
        app.load_data_from_db();
        assert_eq!(app.status_last_seen.get(&alice), Some(&200));
        assert!(!app.has_unseen_statuses(&alice));
    }
}

#[test]
fn opening_status_stores_latest_cursor_even_when_persistence_fails() {
    let mut app = TestApp::new();
    let cursor = FakeStatusCursorPort::default();
    *cursor.fails.lock().unwrap() = true;
    let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
    app.status_cursor = Box::new(cursor.clone());
    app.add_message(status_message(&alice, "a-status", 200));

    app.open_selected_status();

    let stored = cursor.stored.lock().unwrap();
    assert_eq!(
        (stored[0].contact.clone(), stored[0].timestamp),
        (alice.clone(), 200)
    );
    assert_eq!(app.status_last_seen.get(&alice), Some(&200));
}
