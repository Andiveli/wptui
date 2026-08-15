use super::super::presence::PresenceMarker;
use super::super::test_support::{MutableClock, RecordingNotifier, TestApp};
use super::*;
use std::time::Duration;

#[test]
fn injected_clock_preserves_mute_boundary_and_presence_timing() {
    let chat = wr::JID::from("alice@s.whatsapp.net".to_owned());
    let clock = MutableClock::new(Some(1_000));
    let mut app = TestApp::with_ports(clock.clone(), RecordingNotifier::default());
    app.open_chat = Some(chat.clone());
    let now = app.now();
    app.selected_presence.select(Some(chat.clone()), now);
    app.selected_presence.update(&chat, true, 0, now);

    let now = app.now();
    assert!(!notification_is_muted(true, 1_000, now));
    assert_eq!(
        app.selected_presence.marker(Some(&chat), now),
        Some(PresenceMarker::RecentlyOffline)
    );
    assert_eq!(
        app.selected_presence.redraw_after(now),
        Some(Duration::from_secs(300))
    );

    clock.set(Some(1_299));
    let now = app.now();
    assert_eq!(
        app.selected_presence.marker(Some(&chat), now),
        Some(PresenceMarker::RecentlyOffline)
    );
    assert_eq!(
        app.selected_presence.redraw_after(now),
        Some(Duration::from_secs(1))
    );
    clock.set(Some(1_300));
    let now = app.now();
    assert!(notification_is_muted(true, 1_301, now));
    assert!(!notification_is_muted(true, 1_300, now));
    assert_eq!(
        app.selected_presence.marker(Some(&chat), now),
        Some(PresenceMarker::Offline)
    );
    assert_eq!(app.selected_presence.redraw_after(now), None);

    clock.set(None);
    assert_eq!(app.now(), 0);
}
