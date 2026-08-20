use std::sync::mpsc::{self, Sender};

use log::error;
use whatsrust as wr;

use crate::app::App;
use crate::app::events::{AppInput, DrawSource};
use crate::app::terminal_session::TerminalSession;
use crate::ui;

type DownloadSender = Sender<(wr::MessageId, wr::FileId)>;

fn should_draw_for_source(app: &App<'_>, source: DrawSource) -> bool {
    match source {
        DrawSource::Ordinary => true,
        DrawSource::GoLog => app.show_logs,
    }
}

/// Owns the terminal runtime: input pumping, event dispatch, redraws, and shutdown.
///
/// Bootstrap stays in `App::run`; this phase consumes the already-created download
/// sender and delegates each event family to its focused runtime owner.
pub(crate) fn run(app: &mut App<'_>, download_tx: DownloadSender) {
    let mut terminal_session = match TerminalSession::try_new() {
        Ok(session) => session,
        Err(e) => {
            error!("Failed to initialize terminal UI: {e}");
            eprintln!("Failed to initialize terminal UI: {e}");
            let _ = app
                .message_action_diagnostics
                .write_report(std::io::stderr());
            app.shutdown_read_receipt_worker();
            app.finalize_runtime_diagnostics();
            return;
        }
    };

    terminal_session.start_input_reader(&mut app.input_reader, app.tx.clone());

    app.sync_selected_presence();
    let initial_draw_started = app.runtime_diagnostics.draw_started();
    if let Err(error) = terminal_session
        .terminal_mut()
        .draw(|frame| ui::draw(frame, app))
    {
        error!("Failed to draw terminal UI: {error}");
        app.shutdown_read_receipt_worker();
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
        if let Ok(ref input) = msg {
            app.runtime_diagnostics.record_input(input);
        }
        let should_draw = match msg {
            Ok(AppInput::App(event)) => app.handle_media_event(event, &download_tx),
            Ok(AppInput::WhatsApp(event)) => app.handle_whatsapp_event(event),
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
        }
        app.dispatch_read_receipts();

        if app.should_quit {
            break;
        }
    }

    app.shutdown_read_receipt_worker();
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
    use crate::app::test_support::TestApp;

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
