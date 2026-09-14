use super::super::presence::PresenceMarker;
use super::super::presence_diagnostics_port::RawPresenceDiagnosticsPort;
use super::super::presence_subscription_port::PresenceSubscriptionPort;
use super::super::test_support::TestApp;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use whatsrust as wr;

fn jid(value: &str) -> wr::JID {
    value.to_owned().into()
}

struct Port {
    calls: Arc<Mutex<Vec<wr::JID>>>,
    result: wr::SubscribePresenceResult,
}

struct RawDiagnosticsPort {
    reports: Arc<Mutex<VecDeque<Option<String>>>>,
    calls: Arc<Mutex<usize>>,
}

impl RawPresenceDiagnosticsPort for RawDiagnosticsPort {
    fn drain(&self) -> Option<String> {
        *self.calls.lock().unwrap() += 1;
        self.reports.lock().unwrap().pop_front().flatten()
    }
}

impl PresenceSubscriptionPort for Port {
    fn subscribe(&self, jid: &wr::JID) -> wr::SubscribePresenceResult {
        self.calls.lock().unwrap().push(jid.clone());
        self.result
    }
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
fn subscription_uses_the_injected_port_and_preserves_accepted_policy() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut app = TestApp::with_presence_subscription(Box::new(Port {
        calls: Arc::clone(&calls),
        result: wr::SubscribePresenceResult::Accepted,
    }));
    let selected = jid("123@s.whatsapp.net");
    app.open_chat = Some(selected.clone());
    app.mark_presence_ready();

    app.sync_selected_presence();

    assert_eq!(*calls.lock().unwrap(), vec![selected]);
    assert_eq!(app.selected_presence.subscription_due(app.now()), None);
}

#[test]
fn connected_readiness_resets_subscription_state_for_reconnect() {
    let mut app = TestApp::new();
    app.mark_presence_ready();
    assert_eq!(app.selected_presence.subscription_due(app.now()), None);
}

#[test]
fn raw_diagnostics_drain_unconditionally_and_preserve_report_contract() {
    let reports = Arc::new(Mutex::new(VecDeque::from([
        Some("raw presence events received: 1\n".to_owned()),
        Some("raw presence events received: 2\n".to_owned()),
        None,
    ])));
    let calls = Arc::new(Mutex::new(0));
    let mut app = TestApp::new();
    app.set_raw_presence_diagnostics(Box::new(RawDiagnosticsPort {
        reports: Arc::clone(&reports),
        calls: Arc::clone(&calls),
    }));

    let mut disabled_output = Vec::new();
    app.write_presence_diagnostics(&mut disabled_output);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert!(disabled_output.is_empty());

    app.enable_presence_diagnostics(true);
    app.mark_presence_ready();
    let mut enabled_output = Vec::new();
    app.write_presence_diagnostics(&mut enabled_output);

    assert_eq!(*calls.lock().unwrap(), 2);
    assert_eq!(
        String::from_utf8(enabled_output).unwrap(),
        "Presence diagnostics:\nRust translated Presence updates: 0\n1. self presence available: ready\n\nGo raw presence diagnostics:\nraw presence events received: 2\n"
    );

    let mut none_output = Vec::new();
    app.write_presence_diagnostics(&mut none_output);
    assert_eq!(*calls.lock().unwrap(), 3);
    assert_eq!(
        String::from_utf8(none_output).unwrap(),
        "Presence diagnostics:\nRust translated Presence updates: 0\n1. self presence available: ready\n"
    );
    assert!(reports.lock().unwrap().is_empty());
}
