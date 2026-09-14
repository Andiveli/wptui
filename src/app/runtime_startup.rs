use log::info;

use crate::app::App;
use crate::app::media_support::remove_status_media_files;
use crate::app::{PurgeExpiredStatuses, unix_now};
use crate::runtime_callbacks::register as register_runtime_callbacks;

#[cfg(test)]
mod tests;

/// Owns the explicit application composition-root startup sequence.
///
/// This keeps storage preparation, bridge assembly, connection setup, and the
/// handoff to the already-running event loop in their original order.
pub(crate) fn run(app: &mut App<'_>, phone: Option<String>) {
    crate::updater::startup_check(app.tx.clone());
    app.db_handler.init();
    prepare_persisted_state(app);

    let tx = app.tx.clone();
    let diagnostics = app.message_action_diagnostics.clone();
    let download_worker = start_lifecycle(
        app,
        phone,
        move || register_runtime_callbacks(tx, diagnostics),
        |data| qr2term::print_qr(data).unwrap(),
        |code| println!("Pairing code: {}", code),
    );
    info!("Connected, initializing terminal UI");
    crate::app::runtime_loop::run(app, download_worker);
}

fn start_lifecycle(
    app: &mut App<'_>,
    phone: Option<String>,
    register_callbacks: impl FnOnce(),
    mut present_qr: impl FnMut(String) + 'static,
    mut present_pairing: impl FnMut(String) + 'static,
) -> crate::app::download_worker::Worker {
    let lifecycle_control = app.lifecycle_control.clone();
    lifecycle_control.new_client(app.whatsmeow_db.to_str().unwrap());
    register_callbacks();
    let download_worker = app.take_media_download_worker();

    info!("Connecting to WhatsApp Web");
    let qr_lifecycle_control = lifecycle_control.clone();
    lifecycle_control.connect(Box::new(move |qr| {
        present_qr(qr);
        if let Some(phone) = phone.as_ref() {
            let code = qr_lifecycle_control.pair_phone(phone);
            present_pairing(code);
        }
    }));

    download_worker
}

pub(crate) fn prepare_persisted_state(app: &mut App<'_>) {
    // Statuses expire 24h after posting (server-side). Prune the local
    // copies at startup so the DB and media dir do not accumulate them.
    let purged_statuses = app
        .status_retention
        .purge_expired_statuses(PurgeExpiredStatuses { now: unix_now() })
        .unwrap_or_else(|error| panic!("Could not purge expired statuses: {error}"));
    remove_status_media_files(&app.media_path, &purged_statuses.media_paths);
    if !purged_statuses.media_paths.is_empty() {
        info!(
            "Purged {} expired status media files",
            purged_statuses.media_paths.len()
        );
    }
    app.load_data_from_db();
    app.sort_chats();
}
