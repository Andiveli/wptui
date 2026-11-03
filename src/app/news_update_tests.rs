use super::*;
use crate::app::{
    actions::{FocusPane, Section, StatusCompositionState},
    events::{AppEvent, AppInput},
    test_support::TestApp,
};
use crate::updater::news::{NewsHistory, installed_note};
use ratatui::crossterm::event::{Event, KeyCode as Code, KeyEvent, KeyModifiers};

fn key(app: &mut App<'_>, code: Code) {
    let modifiers = if matches!(code, Code::Char(c) if c.is_ascii_uppercase()) {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    app.on_terminal_event(Event::Key(KeyEvent::new(code, modifiers)));
}
fn fixture() -> TestApp {
    let mut app = TestApp::new();
    app.selected_section = Section::News;
    app.focus_pane = FocusPane::ChatList;
    let mut note = installed_note();
    note.tag = "v99.1.0".into();
    note.version = "99.1.0".parse().unwrap();
    app.handle_updater_event(AppEvent::NewsLoaded(NewsHistory {
        releases: vec![note],
        stale: false,
        incomplete: false,
        error: None,
        cache_error: None,
    }));
    app.news.select(1);
    app
}
fn complete(app: &mut App<'_>, result: Result<PathBuf, String>) {
    let target = app.news_update.working.clone().unwrap();
    app.handle_updater_event(AppEvent::NewsUpdateFinished { target, result });
}
#[test]
fn installed_notes_cannot_install_and_confirmation_cancels_safely() {
    let mut app = fixture();
    app.news.select(0);
    key(&mut app, Code::Char('u'));
    assert!(app.news_update.confirm.is_none());
    assert!(app.news_update.working.is_none());
    app.news.select(1);
    key(&mut app, Code::Char('u'));
    assert!(app.news_update.status.contains("99.1.0"));
    key(&mut app, Code::Enter); // Cancel is the default.
    assert!(app.news_update.working.is_none());
    key(&mut app, Code::Char('u'));
    key(&mut app, Code::Esc);
    assert!(app.news_update.confirm.is_none());
    assert_eq!(app.news.installed.version, installed_note().version);
}
#[test]
fn confirmation_is_bound_across_refresh_and_worker_is_single_flight() {
    let mut app = fixture();
    let (tx, rx) = std::sync::mpsc::channel();
    app.news_update.install = Arc::new(move |target| {
        tx.send(target.clone()).unwrap();
        Ok(PathBuf::from("/new/wp-tui"))
    });
    key(&mut app, Code::Char('u'));
    // A refresh can remove the selected note, but cannot retarget confirmation.
    app.handle_updater_event(AppEvent::NewsLoaded(NewsHistory {
        releases: vec![],
        stale: false,
        incomplete: false,
        error: None,
        cache_error: None,
    }));
    key(&mut app, Code::Up);
    key(&mut app, Code::Enter);
    assert_eq!(
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap()
            .tag,
        "v99.1.0"
    );
    key(&mut app, Code::Char('u'));
    assert!(rx.try_recv().is_err());
    let AppInput::App(event) = app
        .rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap()
    else {
        panic!("expected completion")
    };
    app.handle_updater_event(event);
    assert!(app.news_update.restart_prompt);
    assert!(!app.should_quit);
    key(&mut app, Code::Enter); // Later is the default.
    assert!(!app.news_update.restart_prompt);
    assert!(!app.should_quit);
    assert_eq!(
        app.news_update.installed_path.as_deref(),
        Some(std::path::Path::new("/new/wp-tui"))
    );
    key(&mut app, Code::Char('R'));
    key(&mut app, Code::Up);
    key(&mut app, Code::Enter);
    assert!(app.should_quit);
    assert_eq!(app.take_restart_path(), Some(PathBuf::from("/new/wp-tui")));
}
#[test]
fn ordinary_quit_remains_available_in_both_confirmation_prompts() {
    for restart_prompt in [false, true] {
        let mut app = fixture();
        if restart_prompt {
            app.news_update.installed_path = Some(PathBuf::from("/new/wp-tui"));
            key(&mut app, Code::Char('R'));
        } else {
            key(&mut app, Code::Char('u'));
        }
        app.on_terminal_event(Event::Key(KeyEvent::new(
            Code::Char('q'),
            KeyModifiers::CONTROL,
        )));
        assert!(app.should_quit);
        assert!(app.take_restart_path().is_none());
    }
}

#[test]
fn shutdown_joins_injected_install_and_preserves_ordinary_quit_without_restart() {
    let mut app = fixture();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release_rx = std::sync::Mutex::new(release_rx);
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_finished = Arc::clone(&finished);
    app.news_update.install = Arc::new(move |_| {
        started_tx.send(()).unwrap();
        release_rx.lock().unwrap().recv().unwrap();
        worker_finished.store(true, std::sync::atomic::Ordering::SeqCst);
        Err("fixture install failed".into())
    });
    key(&mut app, Code::Char('u'));
    key(&mut app, Code::Up);
    key(&mut app, Code::Enter);
    started_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    app.dispatch_action(crate::app::actions::AppAction::Quit);
    assert!(app.should_quit);
    assert!(!finished.load(std::sync::atomic::Ordering::SeqCst));
    release_tx.send(()).unwrap();
    app.join_news_workers();
    assert!(finished.load(std::sync::atomic::Ordering::SeqCst));
    let AppInput::App(event) = app
        .rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap()
    else {
        panic!("expected completion")
    };
    app.handle_updater_event(event);
    assert!(app.news_update.status.contains("fixture install failed"));
    assert!(app.take_restart_path().is_none());
}

#[test]
fn unrelated_completion_cannot_offer_restart_or_retarget_active_install() {
    let mut app = fixture();
    let target = UpdateTarget {
        tag: "v99.1.0".into(),
        version: "99.1.0".parse().unwrap(),
    };
    app.news_update.working = Some(target.clone());
    app.handle_updater_event(AppEvent::NewsUpdateFinished {
        target: UpdateTarget {
            tag: "v99.2.0".into(),
            version: "99.2.0".parse().unwrap(),
        },
        result: Ok(PathBuf::from("/wrong/wp-tui")),
    });
    assert_eq!(app.news_update.working.as_ref(), Some(&target));
    assert!(app.news_update.installed_path.is_none());
    assert!(!app.news_update.restart_prompt);
    assert!(!app.should_quit);
}

#[test]
fn failures_are_visible_and_do_not_offer_restart() {
    let mut app = fixture();
    app.news_update.working = Some(UpdateTarget {
        tag: "v99.1.0".into(),
        version: "99.1.0".parse().unwrap(),
    });
    complete(&mut app, Err("digest rejected".into()));
    assert!(app.news_update.status.contains("digest rejected"));
    assert_eq!(app.news.installed.version, installed_note().version);
    assert!(!app.news_update.restart_prompt);
    assert!(!app.should_quit);
    assert!(app.take_restart_path().is_none());
}
#[test]
fn restart_is_deferred_for_drafts_attachments_status_context_and_pending_sends() {
    for case in [
        "text",
        "attachment",
        "status",
        "pending status",
        "quote",
        "pending text",
        "pending attachment",
    ] {
        let mut app = fixture();
        app.news_update.installed_path = Some(PathBuf::from("/new/wp-tui"));
        match case {
            "text" => app.composer.insert_text("unsent"),
            "attachment" => app
                .composer
                .queue_attachment("unused".into(), whatsrust::FileKind::Image),
            "status" => app.status_composition = StatusCompositionState::Navigating,
            "pending status" => app.pending_status_sends = 1,
            "quote" => {
                app.composer.insert_text("draft");
                app.composer.begin_status_context();
                app.status_composition = StatusCompositionState::Authoring;
            }
            "pending text" | "pending attachment" => {
                struct FakeSend;
                impl crate::app::optimistic_text_send::TextSendPort for FakeSend {
                    fn send(
                        &mut self,
                        _: &crate::app::optimistic_text_send::TextSendRequest,
                    ) -> Result<(), whatsrust::OutboundSendFailure> {
                        Err(whatsrust::OutboundSendFailure::TransportFailed)
                    }
                }
                app.optimistic_text_send_worker = crate::app::optimistic_text_send::Worker::new(
                    app.tx.clone(),
                    Box::new(FakeSend),
                );
                let content = if case == "pending text" {
                    whatsrust::MessageContent::Text("pending".into())
                } else {
                    whatsrust::MessageContent::File(whatsrust::FileContent {
                        kind: whatsrust::FileKind::Image,
                        path: "unused".into(),
                        file_id: "".into(),
                        caption: None,
                    })
                };
                assert!(app.stage_outbound_batch(
                    "test@s.whatsapp.net".to_string().into(),
                    vec![content],
                    None,
                    vec![],
                    vec![]
                ));
            }
            _ => unreachable!(),
        }
        key(&mut app, Code::Char('R'));
        key(&mut app, Code::Up);
        key(&mut app, Code::Enter);
        assert!(!app.should_quit, "{case}");
        assert!(app.take_restart_path().is_none(), "{case}");
        assert!(
            app.news_update.status.contains("drafts or pending sends"),
            "{case}"
        );
        assert!(app.news_update.installed_path.is_some());
    }
}
