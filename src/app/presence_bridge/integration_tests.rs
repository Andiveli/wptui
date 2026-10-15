use super::super::presence::PresenceMarker;
use super::super::read_receipts::VisibilityPlan;
use super::super::test_support::{FixedClock, RecordingNotifier, TestApp};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use whatsrust as wr;

#[test]
fn injected_clock_reaches_presence_and_ui_marker_path() {
    let chat = wr::JID::from("alice@s.whatsapp.net".to_owned());
    let mut app = TestApp::with_ports(FixedClock::new(1_700_000_000), RecordingNotifier::default());
    app.contacts.insert(chat.clone(), "Alice".into());
    app.open_chat = Some(chat.clone());
    let now = app.now();
    app.selected_presence.select(Some(chat.clone()), now);
    app.selected_presence.update(&chat, true, 0, now);
    assert_eq!(
        app.selected_presence.marker(Some(&chat), now),
        Some(PresenceMarker::RecentlyOffline)
    );

    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    let mut media_render_plan = crate::app::events::MediaRenderPlan::default();
    let mut visibility_plan = VisibilityPlan::default();
    terminal
        .draw(|frame| {
            crate::ui::render_chats_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                &mut visibility_plan,
                Rect::new(0, 0, 40, 8),
            )
        })
        .unwrap();
    assert!(media_render_plan.into_effects().is_empty());
    let row = terminal
        .backend()
        .buffer()
        .content()
        .chunks(40)
        .next()
        .unwrap();
    let title = row.iter().map(|cell| cell.symbol()).collect::<String>();
    assert!(title.contains("● Alice"), "rendered title: {title:?}");
}
