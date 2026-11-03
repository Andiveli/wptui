use super::*;
use crate::app::actions::{AppAction, ConversationMode, FocusPane, Section};
use crate::app::events::{AppEvent, AppInput, MediaRenderPlan};
use crate::app::read_receipts::VisibilityPlan;
use crate::app::test_support::TestApp;
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers},
};

fn key(app: &mut App<'_>, code: KeyCode) {
    app.on_terminal_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn history() -> NewsHistory {
    let mut newest = installed_note();
    newest.version = "9.2.0".parse().unwrap();
    newest.tag = "v9.2.0".into();
    newest.title = "Newest changes".into();
    newest.published_at = Some("2026-06-01".into());
    newest.body = (0..100)
        .map(|i| format!("Long release line {i}\n"))
        .collect();
    let mut intermediate = newest.clone();
    intermediate.version = "9.1.0".parse().unwrap();
    intermediate.tag = "v9.1.0".into();
    intermediate.body = "Intermediate changes".into();
    NewsHistory {
        releases: vec![newest, intermediate],
        stale: false,
        incomplete: false,
        error: None,
        cache_error: None,
    }
}

fn screen(app: &mut App<'_>, width: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, 28)).unwrap();
    terminal
        .draw(|frame| {
            crate::ui::draw_with_plan(
                frame,
                app,
                &mut MediaRenderPlan::default(),
                &mut VisibilityPlan::default(),
            )
        })
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn news_keys_select_intermediate_notes_scroll_and_never_edit_chat() {
    let mut app = TestApp::new();
    app.selected_section = Section::News;
    app.focus_pane = FocusPane::ChatList;
    app.handle_updater_event(AppEvent::NewsLoaded(history()));
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.news.selected_note().version.to_string(), "9.2.0");
    let text = screen(&mut app, 120);
    for label in ["9.2.0", "2026-06-01", "Newest changes", "Installed"] {
        assert!(text.contains(label), "missing {label}");
    }
    key(&mut app, KeyCode::Down);
    assert_eq!(app.news.selected_note().version.to_string(), "9.1.0");
    assert!(screen(&mut app, 120).contains("Intermediate changes"));
    key(&mut app, KeyCode::Up);
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('d'),
        KeyModifiers::CONTROL,
    )));
    assert!(app.news.scroll > 0);
    assert!(screen(&mut app, 120).contains("Long release line"));
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('u'),
        KeyModifiers::CONTROL,
    )));
    assert_eq!(app.news.scroll, 0);
    for code in [KeyCode::Char('i'), KeyCode::Char('/'), KeyCode::Char('n')] {
        key(&mut app, code);
    }
    assert!(!app.contact_search_active);
    assert_eq!(app.conversation_mode, ConversationMode::MessageNavigation);
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.focus_pane, FocusPane::Conversation);
    app.dispatch_action(AppAction::SelectNext);
    assert_eq!(app.news.selected_note().version.to_string(), "9.1.0");
}

#[test]
fn very_long_notes_remain_readable_beyond_u16_scroll_offsets() {
    let mut app = TestApp::new();
    app.selected_section = Section::News;
    app.news.installed.body = "Earlier line\n".repeat(70_000) + "Final note marker";
    app.news.scroll = 70_000;
    assert!(screen(&mut app, 120).contains("Final note marker"));
    assert!(app.news.scroll > u16::MAX as usize);
}

#[test]
fn news_render_reports_failure_cache_incomplete_and_keeps_installed_notes() {
    let mut app = TestApp::new();
    app.selected_section = Section::News;
    let initial = screen(&mut app, 120);
    assert!(initial.contains("Installed"));
    assert!(initial.contains("News wptui"));
    app.news.loading = true;
    assert!(screen(&mut app, 120).contains("Loading"));
    let mut result = history();
    result.stale = true;
    result.error = Some("offline fixture".into());
    result.cache_error = Some("cache write fixture".into());
    result.incomplete = true;
    app.handle_updater_event(AppEvent::NewsLoaded(result));
    let text = screen(&mut app, 120);
    for label in [
        "Offline",
        "cached",
        "Incomplete",
        "offline fixture",
        "cache write fixture",
        "Installed",
    ] {
        assert!(text.contains(label), "missing {label}");
    }
    assert!(!text.contains("up to date"));
    assert!(!app.news.loading);
}

#[test]
fn rail_keeps_old_order_then_news_then_logout_and_wraps() {
    let mut app = TestApp::new();
    app.focus_pane = FocusPane::SectionRail;
    for expected in [Section::Status, Section::Communities, Section::News] {
        app.dispatch_action(AppAction::SelectNext);
        assert_eq!(app.selected_section, expected);
        assert!(!app.rail_on_logout);
    }
    app.dispatch_action(AppAction::SelectNext);
    assert!(app.rail_on_logout);
    app.dispatch_action(AppAction::SelectPrevious);
    assert_eq!(app.selected_section, Section::News);
    assert!(!app.rail_on_logout);
    app.dispatch_action(AppAction::JumpTop);
    assert_eq!(app.selected_section, Section::Chats);
    app.dispatch_action(AppAction::SelectPrevious);
    assert!(app.rail_on_logout);
}

#[test]
fn news_preserves_chat_and_status_drafts_and_in_memory_notes_after_failure() {
    let mut app = TestApp::new();
    app.selected_section = Section::News;
    app.focus_pane = FocusPane::Conversation;
    app.contact_search_active = true;
    app.status_composition = crate::app::actions::StatusCompositionState::Navigating;
    app.handle_updater_event(AppEvent::NewsLoaded(history()));
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('i'));
    assert_eq!(app.news.selected_note().tag, "v9.2.0");
    assert_eq!(
        app.status_composition,
        crate::app::actions::StatusCompositionState::Navigating
    );
    assert_eq!(app.conversation_mode, ConversationMode::MessageNavigation);
    app.handle_updater_event(AppEvent::NewsLoaded(NewsHistory {
        releases: vec![],
        stale: true,
        incomplete: true,
        error: Some("No cache or connection".into()),
        cache_error: None,
    }));
    assert_eq!(app.news.selected_note().tag, "v9.2.0");
    assert!(screen(&mut app, 120).contains("Newest changes"));
}

#[test]
fn mouse_rail_indices_keep_prior_sections_and_logout_and_news_notes_work() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let mut app = TestApp::new();
    app.mouse_capture_enabled = true;
    app.handle_updater_event(AppEvent::NewsLoaded(history()));
    for section in [
        Section::Chats,
        Section::Status,
        Section::Communities,
        Section::News,
    ] {
        screen(&mut app, 120);
        let hit = app
            .mouse_hit_map
            .section_rows
            .iter()
            .find(|hit| hit.section == Some(section))
            .unwrap()
            .area;
        app.on_terminal_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: hit.x,
            row: hit.y,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(app.selected_section, section);
        assert_eq!(app.focus_pane, FocusPane::ChatList);
        assert!(!app.pending_logout);
    }
    screen(&mut app, 120);
    let hit = app.mouse_hit_map.news_rows[1].0;
    app.on_terminal_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: hit.x,
        row: hit.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.news.selected_note().tag, "v9.2.0");
    screen(&mut app, 120);
    let body = app.mouse_hit_map.news_body.unwrap();
    app.on_terminal_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: body.x,
        row: body.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.news.scroll, 3);
    let hit = app
        .mouse_hit_map
        .section_rows
        .iter()
        .find(|hit| hit.section.is_none())
        .unwrap()
        .area;
    app.on_terminal_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: hit.x,
        row: hit.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(app.pending_logout);
    assert!(app.rail_on_logout);
}

#[test]
fn offline_without_cache_never_claims_current_and_complete_empty_history_does() {
    let mut app = TestApp::new();
    app.selected_section = Section::News;
    app.handle_updater_event(AppEvent::NewsLoaded(NewsHistory {
        releases: vec![],
        stale: true,
        incomplete: true,
        error: Some("Connection failed".into()),
        cache_error: None,
    }));
    let text = screen(&mut app, 120);
    assert!(text.contains("Connection failed"));
    assert!(text.contains("Installed"));
    assert!(!text.contains("No newer stable"));
    app.handle_updater_event(AppEvent::NewsLoaded(NewsHistory {
        releases: vec![],
        stale: false,
        incomplete: false,
        error: None,
        cache_error: None,
    }));
    assert!(screen(&mut app, 120).contains("No newer stable releases found."));
}

#[test]
fn worker_failure_reaches_the_ui_and_clears_loading() {
    let mut app = TestApp::new();
    app.news.source = std::sync::Arc::new(|_: &std::path::Path| panic!("fixture failure"));
    app.selected_section = Section::News;
    app.focus_pane = FocusPane::Conversation;
    key(&mut app, KeyCode::Char('r'));
    let AppInput::App(event) = app
        .rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap()
    else {
        panic!("expected App event")
    };
    app.handle_updater_event(event);
    assert!(!app.news.loading);
    assert!(screen(&mut app, 120).contains("worker stopped unexpectedly"));
}

#[test]
fn refresh_is_single_flight_and_delivered_as_an_app_event() {
    let mut app = TestApp::new();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finish_tx, finish_rx) = std::sync::mpsc::channel();
    let finish_rx = std::sync::Mutex::new(finish_rx);
    app.news.source = std::sync::Arc::new(move |path: &std::path::Path| {
        assert!(path.ends_with("news-history.json"));
        started_tx.send(()).unwrap();
        finish_rx.lock().unwrap().recv().unwrap();
        history()
    });
    app.selected_section = Section::News;
    app.focus_pane = FocusPane::ChatList;
    key(&mut app, KeyCode::Char('r'));
    started_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    key(&mut app, KeyCode::Char('r'));
    assert!(started_rx.try_recv().is_err());
    finish_tx.send(()).unwrap();
    let AppInput::App(event) = app
        .rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap()
    else {
        panic!("expected App event")
    };
    app.handle_updater_event(event);
    assert!(!app.news.loading);
    assert_eq!(app.news.history.as_ref().unwrap().releases.len(), 2);
}
