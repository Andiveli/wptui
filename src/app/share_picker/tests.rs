use std::collections::HashMap;

use whatsrust::JID;

use super::{SharePicker, is_forwardable_recipient};

fn jid(value: &str) -> JID {
    JID::from(value.to_owned())
}

fn picker() -> SharePicker {
    let contacts = vec![
        jid("zed@s.whatsapp.net"),
        jid("alice@s.whatsapp.net"),
        jid("bob@s.whatsapp.net"),
    ];
    let labels = HashMap::from([
        (jid("zed@s.whatsapp.net"), "Zed".to_owned()),
        (jid("alice@s.whatsapp.net"), "Alice".to_owned()),
        (jid("bob@s.whatsapp.net"), "Bob".to_owned()),
    ]);
    let recency = HashMap::from([
        (jid("zed@s.whatsapp.net"), 1),
        (jid("alice@s.whatsapp.net"), 3),
        (jid("bob@s.whatsapp.net"), 2),
    ]);
    SharePicker::new(contacts, labels, recency)
}

#[test]
fn sorts_by_recency_then_filters_by_jid_or_label() {
    let mut picker = picker();
    assert_eq!(
        picker.visible_contacts()[0].0.as_ref(),
        "alice@s.whatsapp.net"
    );
    picker.search_character('z');
    assert_eq!(picker.visible_contacts(), vec![&jid("zed@s.whatsapp.net")]);
}

#[test]
fn destinations_preserve_contact_order_and_selection_across_search() {
    let mut picker = picker();
    picker.toggle_selected();
    picker.move_selection(1);
    picker.toggle_selected();
    picker.search_character('a');
    assert_eq!(picker.selected_count(), 2);
    assert_eq!(
        picker.destinations(),
        vec![jid("alice@s.whatsapp.net"), jid("bob@s.whatsapp.net")]
    );
}

#[test]
fn viewport_clamps_and_keeps_selected_row_visible() {
    let mut picker = picker();
    picker.set_viewport_height(2);
    picker.move_selection(2);
    assert_eq!(picker.selected, 2);
    assert_eq!(picker.viewport(), 1..3);
    picker.move_selection(10);
    assert_eq!(picker.selected, 2);
    assert!(picker.viewport().contains(&picker.selected));
}

#[test]
fn search_reset_returns_to_first_visible_row() {
    let mut picker = picker();
    picker.set_viewport_height(1);
    picker.move_selection(2);
    picker.search_character('b');
    assert_eq!((picker.selected, picker.offset), (0, 0));
    picker.search_backspace();
    assert_eq!((picker.selected, picker.offset), (0, 0));
}

#[test]
fn recipient_policy_excludes_broadcast_and_newsletter_chats() {
    assert!(is_forwardable_recipient(&jid("alice@s.whatsapp.net")));
    assert!(!is_forwardable_recipient(&jid("status@broadcast")));
    assert!(!is_forwardable_recipient(&jid("channel@newsletter")));
}
