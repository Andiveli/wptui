use super::super::presence::PresenceMarker;
use super::super::test_support::TestApp;
use whatsrust as wr;

fn jid(value: &str) -> wr::JID {
    value.to_owned().into()
}

#[test]
fn sync_selects_only_the_open_individual_chat() {
    let mut app = TestApp::new();
    app.open_chat = Some(jid("123@g.us"));
    app.sync_selected_presence();
    assert_eq!(app.selected_presence.selected(), None);

    app.open_chat = Some(jid("123@s.whatsapp.net"));
    app.sync_selected_presence();
    assert_eq!(
        app.selected_presence.selected(),
        Some(&jid("123@s.whatsapp.net"))
    );
}

#[test]
fn presence_update_only_redraws_when_it_targets_selected_chat() {
    let mut app = TestApp::new();
    let selected = jid("123@s.whatsapp.net");
    app.open_chat = Some(selected.clone());
    app.sync_selected_presence();

    assert!(!app.handle_presence_update(wr::PresenceUpdate {
        from: jid("456@s.whatsapp.net"),
        unavailable: false,
        last_seen: 0,
    }));
    let now = app.now();
    assert_eq!(
        app.selected_presence.marker(Some(&selected), now),
        Some(PresenceMarker::Offline)
    );

    assert!(app.handle_presence_update(wr::PresenceUpdate {
        from: selected.clone(),
        unavailable: false,
        last_seen: 0,
    }));
    let now = app.now();
    assert_eq!(
        app.selected_presence.marker(Some(&selected), now),
        Some(PresenceMarker::Online)
    );
}

#[test]
fn connected_readiness_resets_subscription_state_for_reconnect() {
    let mut app = TestApp::new();
    app.mark_presence_ready();
    assert_eq!(app.selected_presence.subscription_due(app.now()), None);
}
