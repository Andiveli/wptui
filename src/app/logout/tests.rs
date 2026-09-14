use crate::app::actions::{AppAction, FocusPane, Section};
use std::sync::{Arc, Mutex};

use crate::app::test_support::{RecordingChatReadSyncPort, RecordingLifecycleControl, TestApp};
use crate::input_key::Key;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

#[test]
fn rail_logout_navigation_and_confirmation_preserve_focus_and_section() {
    let mut app = TestApp::new();
    app.focus_pane = FocusPane::SectionRail;
    app.selected_section = Section::Chats;

    app.dispatch_action(AppAction::JumpBottom);
    assert!(app.rail_on_logout);
    assert_eq!(app.selected_section, Section::Communities);
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));
    assert!(app.pending_logout);
    assert_eq!(app.logout_menu_index, 0);
    assert_eq!(app.focus_pane, FocusPane::SectionRail);

    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.logout_menu_index, 1);
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('k'),
        KeyModifiers::NONE,
    )));
    assert_eq!(app.logout_menu_index, 0);
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(!app.pending_logout);
    assert!(!app.logout_in_progress);
    assert_eq!(
        app.action_notice,
        Some(crate::app::actions::ActionNotice::Cancelled)
    );
}

#[test]
fn logout_confirmation_stops_read_sync_before_requesting_lifecycle_logout() {
    let mut app = TestApp::new();
    let trace = Arc::new(Mutex::new(Vec::new()));
    let read_sync = RecordingChatReadSyncPort::with_shutdown_trace(Arc::clone(&trace));
    let lifecycle = Arc::new(RecordingLifecycleControl::with_trace(Arc::clone(&trace)));
    app.set_chat_read_sync(Box::new(read_sync.clone()));
    app.set_lifecycle_control(Arc::clone(&lifecycle));
    app.begin_logout_confirmation();

    app.handle_logout_input(Key::k(crate::input_key::KeyCode::Enter));

    assert!(app.pending_logout);
    assert!(app.logout_in_progress);
    assert_eq!(*read_sync.shutdowns.lock().unwrap(), 1);
    assert_eq!(*lifecycle.logouts.lock().unwrap(), 1);
    assert_eq!(
        *trace.lock().unwrap(),
        ["read-sync:stop", "lifecycle:logout"]
    );
}

#[test]
fn logout_statuses_keep_local_only_and_failed_sessions_retryable() {
    let mut app = TestApp::new();
    app.pending_logout = true;
    app.logout_in_progress = true;
    assert!(app.handle_whatsapp_event(whatsrust::Event::LogoutResult(
        whatsrust::LogoutStatus::LocalOnly,
    )));
    assert!(!app.pending_logout);
    assert!(!app.logout_in_progress);
    assert!(matches!(
        app.action_notice,
        Some(crate::app::actions::ActionNotice::Unavailable(_))
    ));
    app.focus_pane = FocusPane::SectionRail;
    app.rail_on_logout = true;
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));
    assert!(app.pending_logout);

    app.pending_logout = true;
    app.logout_in_progress = true;
    assert!(app.handle_whatsapp_event(whatsrust::Event::LogoutResult(
        whatsrust::LogoutStatus::Failed,
    )));
    assert!(!app.pending_logout);
    assert!(!app.logout_in_progress);
    assert_eq!(app.logout_menu_index, 0);
}

#[test]
fn local_only_logout_restores_read_sync_once_for_cursor_transitions() {
    let mut app = TestApp::new();
    let read_sync = RecordingChatReadSyncPort::default();
    let chat = whatsrust::JID::from("chat@example.test".to_owned());
    app.set_chat_read_sync(Box::new(read_sync.clone()));
    app.stop_read_sync_for_logout();

    app.handle_logout_result(whatsrust::LogoutStatus::LocalOnly);

    app.add_message(crate::app::test_support::message(&chat, "latest", 42));
    assert!(app.mark_chat_read_at_latest(&chat));

    app.handle_logout_result(whatsrust::LogoutStatus::LocalOnly);
    app.handle_logout_result(whatsrust::LogoutStatus::LoggedOut);

    assert!(app.should_quit);
    assert_eq!(*read_sync.shutdowns.lock().unwrap(), 2);
    assert_eq!(*read_sync.restarts.lock().unwrap(), 1);
}

#[test]
fn failed_logout_restores_read_sync_for_cursor_transitions() {
    let mut app = TestApp::new();
    let read_sync = RecordingChatReadSyncPort::default();
    let chat = whatsrust::JID::from("chat@example.test".to_owned());
    app.set_chat_read_sync(Box::new(read_sync.clone()));
    app.stop_read_sync_for_logout();

    app.handle_logout_result(whatsrust::LogoutStatus::Failed);

    app.add_message(crate::app::test_support::message(&chat, "latest", 42));
    assert!(app.mark_chat_read_at_latest(&chat));
    assert_eq!(*read_sync.restarts.lock().unwrap(), 1);
}

#[test]
fn retryable_logout_restarts_the_read_sync_port_exactly_once() {
    let mut app = TestApp::new();
    let read_sync = RecordingChatReadSyncPort::default();
    app.set_chat_read_sync(Box::new(read_sync.clone()));

    app.stop_read_sync_for_logout();
    app.handle_logout_result(whatsrust::LogoutStatus::Failed);
    app.handle_logout_result(whatsrust::LogoutStatus::Failed);

    assert_eq!(*read_sync.shutdowns.lock().unwrap(), 1);
    assert_eq!(*read_sync.restarts.lock().unwrap(), 1);
}

#[test]
fn not_logged_in_is_a_successful_terminal_result() {
    let mut app = TestApp::new();
    app.handle_whatsapp_event(whatsrust::Event::LogoutResult(
        whatsrust::LogoutStatus::NotLoggedIn,
    ));
    assert!(app.should_quit);
}

#[test]
fn successful_logout_cleans_both_databases_and_media() {
    let mut app = TestApp::new();
    std::fs::write(&app.whatsmeow_db, b"db").unwrap();
    std::fs::write(app.whatsmeow_db.with_file_name("whatsapp.db"), b"db").unwrap();
    std::fs::create_dir_all(&app.media_path).unwrap();
    std::fs::write(app.media_path.join("attachment"), b"media").unwrap();

    app.handle_whatsapp_event(whatsrust::Event::LogoutResult(
        whatsrust::LogoutStatus::LoggedOut,
    ));

    assert!(app.should_quit);
    assert!(!app.whatsmeow_db.exists());
    assert!(!app.whatsmeow_db.with_file_name("whatsapp.db").exists());
    assert!(std::fs::read_dir(&app.media_path).unwrap().next().is_none());
}
