use std::sync::mpsc::{self, Sender};

use log::error;
use whatsrust as wr;

use crate::app::App;
use crate::app::events::AppInput;
use crate::app::terminal_session::TerminalSession;
use crate::ui;

type DownloadSender = Sender<(wr::MessageId, wr::FileId)>;

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
            return;
        }
    };

    terminal_session.start_input_reader(&mut app.input_reader, app.tx.clone());

    app.sync_selected_presence();
    if let Err(error) = terminal_session
        .terminal_mut()
        .draw(|frame| ui::draw(frame, app))
    {
        error!("Failed to draw terminal UI: {error}");
        terminal_session.stop_input_reader(&mut app.input_reader);
        terminal_session.restore();
        wr::disconnect();
        let _ = app
            .message_action_diagnostics
            .write_report(std::io::stderr());
        return;
    }

    loop {
        let now = app.now();
        let msg = match app.selected_presence.redraw_after(now) {
            Some(timeout) => match app.rx.recv_timeout(timeout) {
                Ok(input) => Ok(input),
                Err(mpsc::RecvTimeoutError::Timeout) => Ok(AppInput::Draw),
                Err(mpsc::RecvTimeoutError::Disconnected) => Err(mpsc::RecvError),
            },
            None => app.rx.recv(),
        };
        // info!("Received message: {:?}", &msg);
        let should_draw = match msg {
            Ok(AppInput::App(event)) => app.handle_media_event(event, &download_tx),
            Ok(AppInput::WhatsApp(event)) => app.handle_whatsapp_event(event),
            Ok(AppInput::Message { message, is_sync }) => app.process_message(message, is_sync),
            Ok(AppInput::Presence(update)) => app.handle_presence_update(update),
            Ok(AppInput::Terminal(event)) => {
                app.on_terminal_event(event);
                true
            }
            Ok(AppInput::Draw) => true,
            Err(_) => {
                error!("Failed to receive input from channel");
                true
            }
        };

        app.sync_selected_presence();

        if should_draw {
            if let Err(error) = terminal_session
                .terminal_mut()
                .draw(|frame| ui::draw(frame, app))
            {
                error!("Failed to draw terminal UI: {error}");
                break;
            }
        }

        if app.should_quit {
            break;
        }
    }

    terminal_session.stop_input_reader(&mut app.input_reader);
    terminal_session.restore();
    wr::disconnect();
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    app.write_presence_diagnostics(&mut stderr);
    drop(stderr);
    let _ = app
        .message_action_diagnostics
        .write_report(std::io::stderr());
}
