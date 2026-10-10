use whatsrust as wr;

#[test]
fn chat_settings_are_missing_before_the_whatsapp_client_starts() {
    let jid = wr::JID::from("offline@s.whatsapp.net".to_owned());

    let settings = wr::get_chat_settings(&jid);

    assert!(!settings.found);
    assert!(!settings.archived);
    assert!(!settings.pinned);
    assert_eq!(settings.muted_until, 0);
}
