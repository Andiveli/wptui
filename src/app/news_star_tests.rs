use super::*;
use crate::app::actions::{FocusPane, Section};
use crate::app::test_support::TestApp;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use std::sync::Mutex;

fn key(app: &mut App<'_>, ch: char) {
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char(ch),
        KeyModifiers::NONE,
    )));
}

fn draw(app: &mut App<'_>) -> String {
    let mut terminal = Terminal::new(TestBackend::new(96, 24)).unwrap();
    terminal
        .draw(|frame| {
            crate::ui::draw_with_plan(
                frame,
                app,
                &mut crate::app::events::MediaRenderPlan::default(),
                &mut crate::app::read_receipts::VisibilityPlan::default(),
            );
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

fn complete(app: &mut App<'_>) {
    let AppInput::App(event) = app.rx.recv_timeout(Duration::from_secs(2)).unwrap() else {
        panic!("expected completion");
    };
    app.handle_updater_event(event);
}

#[test]
fn only_confirmed_http_results_are_known() {
    for (success, response, expected) in [
        (true, "HTTP/2.0 204 No Content\r\n", Some(true)),
        (false, "HTTP/2.0 404 Not Found\r\n", Some(false)),
        (false, "HTTP/2.0 401 Unauthorized\r\n", None),
        (false, "HTTP/2.0 403 Forbidden\r\n", None),
        (false, "HTTP/2.0 500 Server Error\r\n", None),
        (false, "network error: 404", None),
        (true, "HTTP/2.0 200 OK\r\n", None),
        (false, "", None),
    ] {
        assert_eq!(classify(success, response), expected, "{response}");
    }
    assert_eq!(
        Operation::Check.args(),
        [
            "api",
            "--hostname",
            "github.com",
            "--include",
            "user/starred/Andiveli/wptui",
        ]
    );
    assert_eq!(
        Operation::Star.args(),
        [
            "api",
            "--hostname",
            "github.com",
            "--method",
            "PUT",
            "--include",
            "user/starred/Andiveli/wptui",
        ]
    );
}

#[test]
fn activation_checks_once_and_render_never_stars() {
    let mut app = TestApp::new();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&calls);
    app.news.star.runner = Arc::new(move |operation, _| {
        recorded.lock().unwrap().push(operation);
        Some(false)
    });
    app.activate_news_star();
    assert!(calls.lock().unwrap().is_empty());
    app.selected_section = Section::News;
    app.activate_news_star();
    complete(&mut app);
    assert_eq!(app.news.repository_starred, Some(false));
    for _ in 0..2 {
        assert!(draw(&mut app).contains("Enjoying wptui?"));
        app.activate_news_star();
    }
    app.selected_section = Section::Chats;
    app.activate_news_star();
    app.selected_section = Section::News;
    app.activate_news_star();
    assert_eq!(*calls.lock().unwrap(), [Operation::Check]);
}

#[test]
fn explicit_key_is_single_flight_pending_then_thanks_and_preserves_selection() {
    let mut app = TestApp::new();
    app.selected_section = Section::News;
    app.focus_pane = FocusPane::Conversation;
    app.news.repository_starred = Some(false);
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finish_tx, finish_rx) = std::sync::mpsc::channel();
    let finish_rx = Mutex::new(finish_rx);
    app.news.star.runner = Arc::new(move |operation, _| {
        assert_eq!(operation, Operation::Star);
        started_tx.send(()).unwrap();
        finish_rx.lock().unwrap().recv().unwrap();
        Some(true)
    });
    key(&mut app, 's');
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let pending = draw(&mut app);
    assert!(pending.contains("Starring…"));
    assert!(app.mouse_hit_map.news_star.is_empty());
    assert!(app.mouse_hit_map.news_hide.is_empty());
    key(&mut app, 's');
    key(&mut app, 'd');
    assert!(!app.news.star.dismissed);
    assert!(started_rx.try_recv().is_err());
    finish_tx.send(()).unwrap();
    complete(&mut app);
    assert!(draw(&mut app).contains("Thanks for starring Andiveli/wptui."));
    assert_eq!(app.news.repository_starred, Some(true));
    assert_eq!(app.news.selected, 0);
    assert_eq!(app.news.scroll, 0);
}

#[test]
fn unknown_action_result_hides_without_notification_or_retry() {
    let mut app = TestApp::new();
    app.selected_section = Section::News;
    app.news.repository_starred = Some(false);
    app.news.star.runner = Arc::new(|operation, _| {
        assert_eq!(operation, Operation::Star);
        None
    });
    key(&mut app, 's');
    complete(&mut app);
    let text = draw(&mut app);
    assert!(!text.contains("Enjoying wptui?"));
    assert!(!text.contains("Thanks for starring"));
    assert!(app.action_notice.is_none());
    assert!(app.news.star.pending.is_none());
    key(&mut app, 's');
    assert!(app.news.star.pending.is_none());
}

#[test]
fn dismissal_persists_and_stale_or_wrong_operation_cannot_resurface_it() {
    let mut app = TestApp::new();
    app.selected_section = Section::News;
    app.news.repository_starred = Some(false);
    app.news.star.pending = Some(Operation::Check);
    let generation = app.news.star.generation;
    app.handle_updater_event(AppEvent::NewsStarFinished {
        generation,
        operation: Operation::Star,
        result: Some(true),
    });
    assert_eq!(app.news.repository_starred, Some(false));
    app.handle_updater_event(AppEvent::NewsStarFinished {
        generation: generation + 1,
        operation: Operation::Check,
        result: Some(true),
    });
    assert_eq!(app.news.repository_starred, Some(false));
    assert_eq!(app.news.star.pending, Some(Operation::Check));
    key(&mut app, 'd');
    app.handle_updater_event(AppEvent::NewsStarFinished {
        generation,
        operation: Operation::Check,
        result: Some(false),
    });
    assert!(!draw(&mut app).contains("Enjoying wptui?"));
    assert!(RepositoryStar::new(app.news.star.path.clone()).dismissed);
    assert_eq!(fs::read(&app.news.star.path).unwrap(), b"dismissed\n");
}

#[test]
fn persistence_errors_fail_closed_without_touching_unrelated_data() {
    let directory = tempfile::tempdir().unwrap();
    let obstacle = directory.path().join("obstacle");
    fs::write(&obstacle, b"retained").unwrap();
    for path in [obstacle.join("state"), directory.path().to_path_buf()] {
        assert!(RepositoryStar::new(path).dismissed);
    }
    let mut app = TestApp::new();
    app.selected_section = Section::News;
    app.news.repository_starred = Some(false);
    app.news.star.path = obstacle.join("state");
    key(&mut app, 'd');
    app.handle_updater_event(AppEvent::NewsStarFinished {
        generation: 0,
        operation: Operation::Check,
        result: Some(false),
    });
    assert!(!draw(&mut app).contains("Enjoying wptui?"));
    assert_eq!(fs::read(obstacle).unwrap(), b"retained");
}

#[test]
fn mouse_star_and_hide_use_painted_controls_only() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    for hide in [false, true] {
        let mut app = TestApp::new();
        app.selected_section = Section::News;
        app.mouse_hit_map.section = Section::News;
        app.mouse_capture_enabled = true;
        app.news.repository_starred = Some(false);
        app.news.star.runner = Arc::new(|operation, _| {
            assert_eq!(operation, Operation::Star);
            Some(true)
        });
        draw(&mut app);
        let hit = if hide {
            app.mouse_hit_map.news_hide[0]
        } else {
            app.mouse_hit_map.news_star[0]
        };
        app.on_terminal_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: hit.x,
            row: hit.y,
            modifiers: KeyModifiers::NONE,
        }));
        if hide {
            assert!(app.news.star.dismissed);
            assert!(app.news.star.pending.is_none());
        } else {
            complete(&mut app);
            assert!(draw(&mut app).contains("Thanks for starring"));
        }
    }
}

#[test]
fn shutdown_cancels_and_joins_owned_worker() {
    let mut app = TestApp::new();
    app.selected_section = Section::News;
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    app.news.star.runner = Arc::new(move |_, cancel| {
        started_tx.send(()).unwrap();
        while !cancel.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(1));
        }
        None
    });
    app.activate_news_star();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    app.news.star.shutdown();
    assert!(app.news.star.worker.is_none());
    complete(&mut app);
    assert_eq!(app.news.repository_starred, None);
}

#[test]
fn sibling_share_and_delete_keys_keep_their_previous_empty_selection_errors() {
    use crate::app::actions::{ActionNotice, StatusCompositionState};
    for (section, ch, expected) in [
        (Section::Chats, 's', "Forward is not available"),
        (Section::Chats, 'd', "Delete is not available"),
        (Section::Status, 'd', "Delete is not available"),
    ] {
        let mut app = TestApp::new();
        app.selected_section = section;
        app.focus_pane = FocusPane::Conversation;
        if section == Section::Status {
            app.status_composition = StatusCompositionState::Navigating;
        }
        app.news.repository_starred = Some(false);
        app.news.star.runner = Arc::new(|_, _| panic!("sibling must never call gh"));
        key(&mut app, ch);
        assert!(matches!(app.action_notice.as_ref(),
            Some(ActionNotice::Unavailable(message)) if message == expected));
        assert!(!app.news.star.dismissed);
        assert!(app.news.star.pending.is_none());
        assert_eq!(app.news.repository_starred, Some(false));
    }
}

#[cfg(unix)]
#[test]
fn fake_executable_checks_real_process_adapter_without_environment_mutation() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("fake-gh");
    // Local test executable only; production Command never invokes a shell.
    fs::write(&executable, "#!/bin/sh\n[ \"$*\" = 'api --hostname github.com --include user/starred/Andiveli/wptui' ] || exit 2\nprintf 'HTTP/2.0 404 Not Found\\r\\n'\nexit 1\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let cancel = AtomicBool::new(false);
    assert_eq!(run_gh(&executable, Operation::Check, &cancel), Some(false));
    assert_eq!(
        run_gh(&directory.path().join("missing"), Operation::Check, &cancel),
        None
    );
    fs::write(&executable, "#!/bin/sh\n[ \"$*\" = 'api --hostname github.com --method PUT --include user/starred/Andiveli/wptui' ] || exit 2\nprintf 'HTTP/2.0 204 No Content\\r\\n'\n").unwrap();
    assert_eq!(run_gh(&executable, Operation::Star, &cancel), Some(true));
    fs::write(&executable, "#!/bin/sh\nwhile :; do :; done\n").unwrap();
    let started = Instant::now();
    assert_eq!(
        run_gh_with_timeout(
            &executable,
            Operation::Check,
            &cancel,
            Duration::from_millis(30)
        ),
        None
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(run_gh(&executable, Operation::Star, &cancel), None);
}
