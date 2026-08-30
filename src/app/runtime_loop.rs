use std::sync::mpsc::{self, Sender};

use log::error;
use whatsrust as wr;

use crate::app::App;
use crate::app::download_worker::Worker as DownloadWorker;
use crate::app::events::{AppEvent, AppEventFamily, AppInput, DrawSource};
use crate::app::media_jobs::MediaJobOwner;
use crate::app::terminal_session::TerminalSession;
use crate::ui;

type DownloadSender = Sender<(wr::MessageId, wr::FileId)>;

fn dispatch_app_event(
    app: &mut App<'_>,
    event: AppEvent,
    download_tx: &DownloadSender,
    media_jobs: &mut MediaJobOwner,
) -> bool {
    match event.family() {
        AppEventFamily::Send => app.handle_send_event(event),
        AppEventFamily::ReadReceipt => app.handle_read_receipt_event(event),
        AppEventFamily::Avatar => app.handle_avatar_event(event),
        AppEventFamily::Updater => app.handle_updater_event(event),
        AppEventFamily::MediaViewer => {
            app.handle_media_viewer_event(event, download_tx, media_jobs)
        }
    }
}

fn should_draw_for_source(app: &App<'_>, source: DrawSource) -> bool {
    match source {
        DrawSource::Ordinary => true,
        DrawSource::GoLog => app.show_logs,
    }
}

fn refresh_composer_viewport_width(app: &mut App<'_>, terminal_session: &mut TerminalSession) {
    let Ok(area) = terminal_session.terminal_mut().size() else {
        return;
    };
    let width = crate::ui::composer_viewport_width(
        area.width,
        area.height,
        app.pane_visibility,
        app.show_logs,
    );
    app.set_composer_viewport_width(width);
}

#[derive(Debug, PartialEq)]
enum TerminalInitializationFailureTeardown {
    StopDownloadWorker,
    StopReadReceiptWorker,
    StopReadSyncWorker,
    Disconnect,
    FinalizeDiagnostics,
}

fn finish_terminal_initialization_failure(
    mut teardown: impl FnMut(TerminalInitializationFailureTeardown),
) {
    teardown(TerminalInitializationFailureTeardown::StopDownloadWorker);
    teardown(TerminalInitializationFailureTeardown::StopReadReceiptWorker);
    teardown(TerminalInitializationFailureTeardown::StopReadSyncWorker);
    teardown(TerminalInitializationFailureTeardown::Disconnect);
    teardown(TerminalInitializationFailureTeardown::FinalizeDiagnostics);
}

/// Owns the terminal runtime: input pumping, event dispatch, redraws, and shutdown.
///
/// Bootstrap stays in `App::run`; this phase owns the already-created download
/// worker and delegates each event family to its focused runtime owner.
pub(crate) fn run(app: &mut App<'_>, mut download_worker: DownloadWorker) {
    let download_tx = download_worker.sender();
    let mut terminal_session = match TerminalSession::try_new() {
        Ok(session) => session,
        Err(e) => {
            error!("Failed to initialize terminal UI: {e}");
            eprintln!("Failed to initialize terminal UI: {e}");
            let _ = app
                .message_action_diagnostics
                .write_report(std::io::stderr());
            finish_terminal_initialization_failure(|step| match step {
                TerminalInitializationFailureTeardown::StopDownloadWorker => {
                    download_worker.shutdown();
                }
                TerminalInitializationFailureTeardown::StopReadReceiptWorker => {
                    app.shutdown_read_receipt_worker();
                }
                TerminalInitializationFailureTeardown::StopReadSyncWorker => {
                    app.shutdown_read_sync_worker();
                }
                TerminalInitializationFailureTeardown::Disconnect => wr::disconnect(),
                TerminalInitializationFailureTeardown::FinalizeDiagnostics => {
                    app.finalize_runtime_diagnostics();
                }
            });
            return;
        }
    };

    let mut media_jobs = MediaJobOwner::new();
    terminal_session.start_input_reader(&mut app.input_reader, app.tx.clone());

    app.sync_selected_presence();
    refresh_composer_viewport_width(app, &mut terminal_session);
    let initial_draw_started = app.runtime_diagnostics.draw_started();
    if let Err(error) = terminal_session
        .terminal_mut()
        .draw(|frame| ui::draw(frame, app))
    {
        error!("Failed to draw terminal UI: {error}");
        app.shutdown_avatar_runtime();
        media_jobs.shutdown();
        download_worker.shutdown();
        app.shutdown_read_receipt_worker();
        app.shutdown_read_sync_worker();
        terminal_session.stop_input_reader(&mut app.input_reader);
        terminal_session.restore();
        wr::disconnect();
        let _ = app
            .message_action_diagnostics
            .write_report(std::io::stderr());
        app.finalize_runtime_diagnostics();
        return;
    }
    app.runtime_diagnostics.record_should_draw();
    if let Some(started) = initial_draw_started {
        app.runtime_diagnostics.record_draw_finished(started);
    }
    if let Ok(area) = terminal_session.terminal_mut().size() {
        app.schedule_avatar_viewport(area.into());
    }
    app.dispatch_read_receipts();

    loop {
        let now = app.now();
        let msg = match app.selected_presence.redraw_after(now) {
            Some(timeout) => match app.rx.recv_timeout(timeout) {
                Ok(input) => Ok(input),
                Err(mpsc::RecvTimeoutError::Timeout) => Ok(AppInput::Draw(DrawSource::Ordinary)),
                Err(mpsc::RecvTimeoutError::Disconnected) => Err(mpsc::RecvError),
            },
            None => app.rx.recv(),
        };
        // info!("Received message: {:?}", &msg);
        refresh_composer_viewport_width(app, &mut terminal_session);
        if let Ok(ref input) = msg {
            app.runtime_diagnostics.record_input(input);
        }
        let should_draw = match msg {
            Ok(AppInput::App(event)) => {
                dispatch_app_event(app, event, &download_tx, &mut media_jobs)
            }
            Ok(AppInput::WhatsApp(event)) => {
                if matches!(
                    &event,
                    wr::Event::LogoutResult(
                        wr::LogoutStatus::LoggedOut | wr::LogoutStatus::NotLoggedIn
                    )
                ) {
                    // This is terminal shutdown, not ordinary event handling: no job
                    // may access or publish into the media directory while logout clears it.
                    media_jobs.shutdown();
                    download_worker.shutdown();
                    app.shutdown_read_sync_worker();
                }
                app.handle_whatsapp_event(event)
            }
            Ok(AppInput::Message { message, is_sync }) => app.process_message(message, is_sync),
            Ok(AppInput::Presence(update)) => app.handle_presence_update(update),
            Ok(AppInput::Terminal(event)) => {
                app.on_terminal_event(event);
                true
            }
            Ok(AppInput::Draw(source)) => {
                app.runtime_diagnostics.record_draw_source(source);
                let should_draw = should_draw_for_source(app, source);
                if !should_draw && matches!(source, DrawSource::GoLog) {
                    app.runtime_diagnostics.record_go_log_draw_suppressed();
                }
                should_draw
            }
            Err(_) => {
                error!("Failed to receive input from channel");
                true
            }
        };

        app.sync_selected_presence();

        if should_draw {
            app.runtime_diagnostics.record_should_draw();
            let started = app.runtime_diagnostics.draw_started();
            if let Err(error) = terminal_session
                .terminal_mut()
                .draw(|frame| ui::draw(frame, app))
            {
                error!("Failed to draw terminal UI: {error}");
                app.set_read_receipt_readiness(crate::app::read_receipts::Readiness::Disconnected);
                break;
            }
            if let Some(started) = started {
                app.runtime_diagnostics.record_draw_finished(started);
            }
            if let Ok(area) = terminal_session.terminal_mut().size() {
                app.schedule_avatar_viewport(area.into());
            }
        }
        app.dispatch_read_receipts();

        if app.should_quit {
            break;
        }
    }

    app.shutdown_avatar_runtime();
    media_jobs.shutdown();
    download_worker.shutdown();
    app.shutdown_read_receipt_worker();
    app.shutdown_read_sync_worker();
    terminal_session.stop_input_reader(&mut app.input_reader);
    terminal_session.restore();
    app.set_read_receipt_readiness(crate::app::read_receipts::Readiness::Disconnected);
    wr::disconnect();
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    app.write_presence_diagnostics(&mut stderr);
    drop(stderr);
    let _ = app
        .message_action_diagnostics
        .write_report(std::io::stderr());
    app.finalize_runtime_diagnostics();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::events::AppEvent;
    use crate::app::test_support::TestApp;

    #[test]
    fn terminal_initialization_failure_stops_worker_disconnects_once_and_finalizes() {
        let mut events = Vec::new();

        finish_terminal_initialization_failure(|step| events.push(step));

        assert_eq!(
            events,
            [
                TerminalInitializationFailureTeardown::StopDownloadWorker,
                TerminalInitializationFailureTeardown::StopReadReceiptWorker,
                TerminalInitializationFailureTeardown::StopReadSyncWorker,
                TerminalInitializationFailureTeardown::Disconnect,
                TerminalInitializationFailureTeardown::FinalizeDiagnostics,
            ]
        );
    }

    #[test]
    fn send_events_route_to_the_send_handler() {
        let mut app = TestApp::new();
        let (download_tx, _download_rx) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();

        assert!(!dispatch_app_event(
            &mut app,
            AppEvent::OutboundSendFailed { local_send_id: 1 },
            &download_tx,
            &mut media_jobs,
        ));
    }

    #[test]
    fn read_receipt_events_route_to_the_read_receipt_handler() {
        let mut app = TestApp::new();
        app.read_receipts.set_enabled(true);
        let (download_tx, _download_rx) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();

        assert!(!dispatch_app_event(
            &mut app,
            AppEvent::ReadReceiptRestored(Ok(Vec::new())),
            &download_tx,
            &mut media_jobs,
        ));
    }

    #[test]
    fn updater_events_route_to_the_updater_handler() {
        let mut app = TestApp::new();
        let (download_tx, _download_rx) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();

        assert!(dispatch_app_event(
            &mut app,
            AppEvent::UpdateAvailable("1.2.3".to_owned()),
            &download_tx,
            &mut media_jobs,
        ));
        assert_eq!(app.update_notice.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn stale_media_viewer_requests_route_without_spawning_work() {
        let mut app = TestApp::new();
        let current = crate::app::events::ViewerPreviewKey::new("current.jpg", 20, 10);
        app.viewer_preview = Some(crate::app::events::ViewerPreviewState::Loading(
            current.clone(),
        ));
        let (download_tx, _download_rx) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();

        assert!(!dispatch_app_event(
            &mut app,
            AppEvent::LoadViewerPreview(crate::app::events::ViewerPreviewKey::new(
                "stale.jpg",
                20,
                10,
            )),
            &download_tx,
            &mut media_jobs,
        ));
        assert_eq!(app.viewer_preview.as_ref().unwrap().key(), &current);
    }

    #[test]
    fn stale_media_viewer_results_route_without_mutating_preview_state() {
        let mut app = TestApp::new();
        let current = crate::app::events::ViewerPreviewKey::new("current.jpg", 20, 10);
        app.viewer_preview = Some(crate::app::events::ViewerPreviewState::Loading(
            current.clone(),
        ));
        let (download_tx, _download_rx) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();

        assert!(!dispatch_app_event(
            &mut app,
            AppEvent::SetViewerPreview(
                crate::app::events::ViewerPreviewKey::new("stale.jpg", 20, 10),
                None,
            ),
            &download_tx,
            &mut media_jobs,
        ));
        assert_eq!(app.viewer_preview.as_ref().unwrap().key(), &current);
    }

    #[test]
    fn hidden_log_panel_suppresses_only_go_log_draws() {
        let mut app = TestApp::new();
        app.show_logs = false;

        assert!(!should_draw_for_source(&app, DrawSource::GoLog));
        assert!(should_draw_for_source(&app, DrawSource::Ordinary));
    }

    #[test]
    fn visible_log_panel_requests_go_log_draws() {
        let mut app = TestApp::new();
        app.show_logs = true;

        assert!(should_draw_for_source(&app, DrawSource::GoLog));
        assert!(should_draw_for_source(&app, DrawSource::Ordinary));
    }

    #[test]
    fn draw_event_order_is_preserved_while_only_hidden_go_logs_are_suppressed() {
        let mut app = TestApp::new();
        app.show_logs = false;

        let results = [DrawSource::GoLog, DrawSource::Ordinary, DrawSource::GoLog]
            .map(|source| should_draw_for_source(&app, source));

        assert_eq!(results, [false, true, false]);
    }
}
