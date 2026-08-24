use log::info;
use whatsrust as wr;

use crate::app::App;
use crate::app::download_worker::spawn as spawn_download_worker;
use crate::app::media_support::remove_status_media_files;
use crate::app::runtime_callbacks::register as register_runtime_callbacks;
use crate::app::unix_now;

/// Owns the explicit application composition-root startup sequence.
///
/// This keeps storage preparation, bridge assembly, connection setup, and the
/// handoff to the already-running event loop in their original order.
pub(crate) fn run(app: &mut App<'_>, phone: Option<String>) {
    crate::updater::startup_check(app.tx.clone());
    app.db_handler.init();
    // Statuses expire 24h after posting (server-side). Prune the local
    // copies at startup so the DB and media dir do not accumulate them.
    let purged_status_media = app.db_handler.purge_expired_statuses(unix_now());
    remove_status_media_files(&app.media_path, &purged_status_media);
    if !purged_status_media.is_empty() {
        info!(
            "Purged {} expired status media files",
            purged_status_media.len()
        );
    }
    app.load_data_from_db();
    app.sort_chats();

    wr::new_client(app.whatsmeow_db.to_str().unwrap());
    register_runtime_callbacks(app.tx.clone(), app.message_action_diagnostics.clone());

    let download_tx = spawn_download_worker(app.media_path.to_owned(), app.tx.clone());

    info!("Connecting to WhatsApp Web");
    // thread::spawn(|| {
    wr::connect(move |data| {
        qr2term::print_qr(data).unwrap();
        if let Some(phone) = phone.as_ref() {
            let code = wr::pair_phone(phone);
            println!("Pairing code: {}", code);
        }
    });
    // });
    info!("Connected, initializing terminal UI");
    crate::app::runtime_loop::run(app, download_tx);
}
