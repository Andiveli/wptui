use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};

use arboard::Clipboard;
use directories::ProjectDirs;
use ratatui::widgets::ListState;
use ratatui_image::picker::{Picker, ProtocolType};

use super::*;
use crate::app::preferences;

pub(crate) fn default_app() -> App<'static> {
    let picker = picker_from_terminal();
    let default_protocol_type = picker.protocol_type();
    let project_dirs = ProjectDirs::from("com", "nullptr", "wptui").unwrap();
    with_data_dir_and_picker_and_ports(
        project_dirs.data_dir(),
        project_dirs.cache_dir(),
        picker,
        default_protocol_type,
        Box::new(SystemClock),
        Box::new(NotifyRustNotifier),
    )
}

pub(crate) fn with_data_dir(data_dir: &Path, cache_dir: &Path) -> App<'static> {
    let picker = picker_from_terminal();
    let default_protocol_type = picker.protocol_type();
    with_data_dir_and_picker_and_ports(
        data_dir,
        cache_dir,
        picker,
        default_protocol_type,
        Box::new(SystemClock),
        Box::new(NotifyRustNotifier),
    )
}

pub(crate) fn with_data_dir_and_ports(
    data_dir: &Path,
    cache_dir: &Path,
    clock: Box<dyn Clock>,
    notifier: Box<dyn Notifier>,
) -> App<'static> {
    let picker = picker_from_terminal();
    let default_protocol_type = picker.protocol_type();
    with_data_dir_and_picker_and_ports(
        data_dir,
        cache_dir,
        picker,
        default_protocol_type,
        clock,
        notifier,
    )
}

fn picker_from_terminal() -> Picker {
    Picker::from_query_stdio().unwrap_or_else(|err| {
        // Fallback for non-interactive environments (e.g. CI, piped stdio).
        log::warn!(
            "Failed to query terminal image capabilities; falling back to halfblocks: {err}"
        );
        Picker::halfblocks()
    })
}

pub(crate) fn with_data_dir_and_picker_and_ports(
    data_dir: &Path,
    cache_dir: &Path,
    picker: Picker,
    default_protocol_type: ProtocolType,
    clock: Box<dyn Clock>,
    notifier: Box<dyn Notifier>,
) -> App<'static> {
    fs::create_dir_all(data_dir).unwrap();
    let (tx, rx) = mpsc::channel::<AppInput>();
    let (clipboard_reader, clipboard_writer) = open_clipboard_pair();
    let preferences_path = preferences::settings_path(data_dir);
    let composer_direction = preferences::load_composer_direction(&preferences_path);

    let db_path = data_dir.join("whatsapp.db");
    let db_handler = DatabaseHandler::new(&db_path);
    let chat_store_write = Box::new(db_handler.chat_store_writer());
    App {
        db_handler,
        chat_store_hydration: Box::new(crate::db::SqliteChatStoreHydration::new(&db_path)),
        chat_store_write,
        message_reaction_write: Box::new(crate::db::SqliteMessageReactionWriter::new(&db_path)),
        media_path: data_dir.join("media"),
        whatsmeow_db: data_dir.join("whatsmeow.db"),
        clock,
        notifier,
        messages: HashMap::new(),
        message_actions: HashMap::new(),
        local_action_sequence: 0,
        message_action_diagnostics: MessageActionDiagnostics::new(false),
        reactions: HashMap::new(),
        chats: HashMap::new(),
        group_permissions: HashMap::new(),
        contacts: HashMap::new(),
        clipboard_reader,
        clipboard_writer,
        chat_messages: HashMap::new(),
        sorted_chats: Vec::new(),
        chat_list_state: ListState::default(),
        open_chat: None,
        open_status_contact: None,
        communities: Vec::new(),
        community_detail: None,
        communities_unavailable: false,
        communities_loaded: false,
        status_contacts: Vec::new(),
        status_selection: ListState::default(),
        status_last_seen: HashMap::new(),
        message_list_state: MessageListState::default(),
        timeline: unread_messages::Timeline::default(),
        metadata: HashMap::new(),
        history_sync_percent: None,
        selected_presence: SelectedPresence::default(),
        presence_diagnostics: PresenceDiagnostics::default(),
        image_cache: HashMap::new(),
        image_cache_order: VecDeque::new(),
        audio_durations: HashMap::new(),
        message_height_cache: MessageHeightCache::default(),
        message_sequence_cache: HashMap::new(),
        message_sequence_revisions: HashMap::new(),
        default_protocol_type,
        composer: Composer::default(),
        composer_direction,
        composer_viewport_width: 80,
        preferences_path,
        picker: Arc::new(Mutex::new(picker)),
        contact_avatars: ContactAvatars::new(cache_dir.join("contact-avatars")),
        focus_pane: FocusPane::ChatList,
        pane_visibility: PaneVisibility::default(),
        selected_section: Section::default(),
        conversation_mode: ConversationMode::MessageNavigation,
        edit_message: None,
        message_editor: Box::new(WhatsAppMessageEditor),
        message_reactor: Box::new(WhatsAppMessageReactor),
        message_forwarder: Box::new(WhatsAppMessageForwarder),
        message_revoker: Box::new(WhatsAppMessageRevoker),
        action_notice: None,
        update_notice: None,
        message_menu: None,
        contextual_menu: None,
        leader_menu: None,
        shortcut_popup: false,
        reaction_picker: None,
        share_picker: None,
        url_picker: None,
        file_picker: None,
        url_opener: Box::new(SystemUrlOpener),
        launch_executor: Box::new(crate::media::CommandLaunchExecutor),
        attachment_viewer: None,
        viewer_preview: None,
        viewer_zoom: 100,
        read_receipts: ReadReceiptCoordinator::default(),
        read_receipt_worker: crate::app::read_receipts::worker::Worker::new(
            tx.clone(),
            Box::new(crate::app::read_receipts::whatsapp_adapter::WhatsAppAdapter),
            Box::new(
                crate::app::read_receipts::sqlite_repository::SqliteRepository::new(
                    data_dir.join("whatsapp.db"),
                ),
            ),
        ),
        read_sync_worker: wr::ReadSyncWorker::new(),
        read_sync_worker_stopped_for_logout: false,
        optimistic_text_send_worker: crate::app::optimistic_text_send::Worker::new(
            tx.clone(),
            Box::new(crate::app::optimistic_text_send::WhatsAppTextSendPort),
        ),
        pending_outgoing_text: HashMap::new(),
        completed_text_send_ids: VecDeque::new(),
        next_local_send_id: 1,
        kh: KeybindHandler::default(),
        contact_search_active: false,
        contact_search: TextInput::new(),
        filtered_chats: Vec::new(),
        show_logs: false,
        should_quit: false,
        pending_logout: false,
        logout_in_progress: false,
        rail_on_logout: false,
        logout_menu_index: 0,
        tx,
        rx,
        input_reader: InputReader::new(),
        runtime_diagnostics: RuntimeDiagnostics::from_environment(cache_dir),
        chat_list_view: None,
        chat_list_revision: 0,
        chat_list_mutation_depth: 0,
        chat_list_mutation_pending: false,
    }
}

/// Opens two independent system clipboard handles with headless fallbacks.
fn open_clipboard_pair() -> (Box<dyn ClipboardReader>, Box<dyn ClipboardWriter>) {
    match Clipboard::new() {
        Ok(reader_clipboard) => {
            let reader: Box<dyn ClipboardReader> =
                Box::new(SystemClipboardReader(reader_clipboard));
            let writer: Box<dyn ClipboardWriter> = match Clipboard::new() {
                Ok(writer_clipboard) => Box::new(SystemClipboardWriter(writer_clipboard)),
                Err(_) => Box::new(UnavailableClipboardWriter),
            };
            (reader, writer)
        }
        Err(_) => (
            Box::new(UnavailableClipboardReader),
            Box::new(UnavailableClipboardWriter),
        ),
    }
}
