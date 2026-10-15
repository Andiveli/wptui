use std::sync::mpsc::Sender;

use log::Level;
use whatsrust as wr;

use crate::app::events::{AppEvent, AppInput, DrawSource};
use crate::app::message_action_diagnostics::MessageActionDiagnostics;

/// Register the process-lifetime callbacks that translate Go bridge activity into app inputs.
///
/// Each callback owns its sender clone, preserving the existing callback ingress and channel
/// ordering while keeping runtime setup separate from the event loop.
pub(crate) fn register(tx: Sender<AppInput>, diagnostics: MessageActionDiagnostics) {
    {
        let tx = tx.clone();
        wr::set_log_handler(move |msg, level| {
            let level = match level {
                0 => Level::Error,
                1 => Level::Warn,
                2 => Level::Info,
                3 => Level::Debug,
                _ => Level::Trace,
            };
            diagnostics.record_go_log(&msg);
            log::log!(level, "{msg}");
            tx.send(AppInput::Draw(DrawSource::GoLog)).unwrap();
        });
    }
    {
        let tx = tx.clone();
        wr::set_event_handler(move |event| {
            tx.send(AppInput::WhatsApp(event)).unwrap();
        });
    }
    {
        let tx = tx.clone();
        wr::set_presence_handler(move |update| {
            tx.send(AppInput::Presence(update)).unwrap();
        });
    }
    let optimistic_tx = tx.clone();
    wr::set_message_handler(move |message, is_sync| {
        tx.send(AppInput::Message { message, is_sync }).unwrap();
    });
    wr::set_optimistic_text_sent_handler(move |local_send_id, message| {
        optimistic_tx
            .send(AppInput::App(AppEvent::OutboundSendSucceeded {
                local_send_id,
                message,
            }))
            .unwrap();
    });
}
