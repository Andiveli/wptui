use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    sync::Condvar,
    sync::Mutex,
};
use std::{fs, thread};

pub mod actions;
pub mod chat_opening;
pub mod chat_ordering;
pub mod chat_projection;
pub mod chat_store;
pub mod community_bridge;
pub mod community_hierarchy;
pub mod composer;
pub mod contact_avatars;
pub mod events;
pub mod inputs;
pub mod media_cache;
pub mod media_support;
pub mod message_action_diagnostics;
pub mod message_actions;
pub mod message_ingestion;
pub mod notifications;
pub mod presence;
pub mod presence_bridge;
pub mod private_reply;
pub mod share_picker;
pub mod status_projection;
#[cfg(test)]
pub(crate) mod test_support;

pub use crate::app;
use crate::app::actions::{
    ActionNotice, ClipboardReader, ClipboardWriter, ConversationMode, FocusPane, MessageEditor,
    MessageForwarder, MessageMenuAction, MessageReactor, MessageRevoker, PaneVisibility, Section,
    SystemClipboardReader, SystemClipboardWriter, SystemUrlOpener, UnavailableClipboardReader,
    UnavailableClipboardWriter, UrlOpener, WhatsAppMessageEditor, WhatsAppMessageForwarder,
    WhatsAppMessageReactor, WhatsAppMessageRevoker,
};
pub use crate::app::chat_projection::ChatRow;
pub use crate::app::community_hierarchy::CommunityNode;
use crate::app::composer::Composer;
use crate::app::contact_avatars::ContactAvatars;
use crate::app::events::{AppEvent, AppInput, AttachmentViewerState, ViewerPreviewState};
use crate::app::media_support::{
    apply_video_play_marker, generate_video_thumbnail, has_decent_video_thumbnail,
};
pub use crate::app::media_support::{remove_owned_media_files, remove_status_media_files};
use crate::app::message_action_diagnostics::{MessageActionDiagnostics, identifier_for_log};
pub use crate::app::message_actions::{
    DELETED_MESSAGE_TEXT, MessageAction, MessageActionKind, MessageStatus,
};
pub use crate::app::notifications::{
    Clock, NotificationProjection, Notifier, NotifyRustNotifier, SystemClock, now_or, unix_now,
};
use crate::app::presence::{PresenceDiagnostics, SelectedPresence};
pub use crate::app::share_picker::SharePicker;
pub use crate::app::status_projection::STATUS_BROADCAST_CHAT;
use crate::db;
use crate::file_picker::FilePickerState;
use crate::key_handler::KeybindHandler;
use crate::media::MediaRoot;
use crate::ui;
// use crate::key_handler;

use arboard::Clipboard;
use db::{DatabaseHandler, MessageActionPersistence};
use directories::ProjectDirs;
use log::{debug, error, info, trace};
use ratatui::crossterm::event;
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, ResizeEncodeRender};
use ui::message_list::{IMAGE_HEIGHT, IMAGE_WIDTH, VIDEO_HEIGHT, VIDEO_WIDTH};
use ui::message_list::{MessageHeightCache, MessageListState};
use whatsrust as wr;

use crate::ui::text_input::TextInput;

pub const ADMIN_ONLY_GROUP_MESSAGE: &str = "Only group admins can send messages in this group.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputReaderState {
    Running,
    Stopped,
}

#[derive(Clone, Debug)]
pub struct Chat {
    pub jid: wr::JID,
    pub last_message_time: Option<i64>,
}

#[derive(Debug)]
pub enum FileMeta {
    Loaded,
    Loading,
    LoadFailed,
    Downloaded,
    Downloading,
    DownloadFailed,
}

pub enum Metadata {
    File(FileMeta),
}

pub struct App<'a> {
    pub db_handler: DatabaseHandler,
    pub media_path: PathBuf,
    pub whatsmeow_db: PathBuf,
    pub clock: Box<dyn Clock>,
    pub notifier: Box<dyn Notifier>,

    pub messages: HashMap<wr::MessageId, wr::Message>,
    pub message_actions: HashMap<wr::MessageId, Vec<MessageAction>>,
    local_action_sequence: u64,
    message_action_diagnostics: MessageActionDiagnostics,
    pub reactions: HashMap<wr::MessageId, HashMap<wr::JID, Arc<str>>>,
    pub chats: HashMap<wr::JID, Chat>,
    pub group_permissions: HashMap<wr::JID, wr::GroupInfo>,

    // Maps JID to display name
    pub contacts: HashMap<wr::JID, Arc<str>>,

    pub clipboard_reader: Box<dyn ClipboardReader>,
    pub clipboard_writer: Box<dyn ClipboardWriter>,

    pub chat_messages: HashMap<wr::JID, Vec<wr::MessageId>>,

    pub sorted_chats: Vec<wr::JID>,
    pub chat_list_state: ListState,
    /// The chat whose messages, composer, and presence are active. Only
    /// changes when the user presses Enter in the chat list; the list
    /// highlight (chat_list_state) moves independently.
    pub open_chat: Option<wr::JID>,
    /// Status section: the contact whose statuses are open in the right
    /// pane. Mirrors `open_chat` for the Chats section: set on Enter.
    pub open_status_contact: Option<wr::JID>,
    pub communities: Vec<CommunityNode>,
    pub communities_unavailable: bool,
    communities_loaded: bool,

    /// Contacts with posted statuses, sorted by latest-status recency (newest
    /// first). Derived from the `status@broadcast` chat.
    pub status_contacts: Vec<wr::JID>,
    pub status_selection: ListState,
    /// Latest status timestamp the user has viewed per contact. In-memory
    /// only: a restart resets it, so previously-read statuses reappear as
    /// unseen once.
    pub status_last_seen: HashMap<wr::JID, i64>,

    pub history_sync_percent: Option<u8>,
    pub selected_presence: SelectedPresence,
    presence_diagnostics: PresenceDiagnostics,

    pub composer: Composer<'a>,
    pub message_list_state: MessageListState,
    pub metadata: HashMap<wr::MessageId, Metadata>,
    pub image_cache: HashMap<Arc<str>, StatefulProtocol>,
    pub image_cache_order: VecDeque<Arc<str>>,
    /// Probed audio duration in seconds, keyed by file path. Populated lazily
    /// by a background thread once the file is on disk.
    pub audio_durations: HashMap<Arc<str>, u64>,
    pub message_height_cache: MessageHeightCache,
    pub default_protocol_type: ProtocolType,
    pub picker: Arc<Mutex<Picker>>,
    pub contact_avatars: ContactAvatars,

    pub focus_pane: FocusPane,
    pub pane_visibility: PaneVisibility,
    pub selected_section: Section,
    pub conversation_mode: ConversationMode,
    pub edit_message: Option<wr::Message>,
    pub message_editor: Box<dyn MessageEditor>,
    pub message_reactor: Box<dyn MessageReactor>,
    pub message_forwarder: Box<dyn MessageForwarder>,
    pub message_revoker: Box<dyn MessageRevoker>,
    pub action_notice: Option<ActionNotice>,
    pub message_menu: Option<(Vec<MessageMenuAction>, usize)>,
    pub reaction_picker: Option<(Vec<String>, usize)>,
    pub share_picker: Option<SharePicker>,
    pub url_picker: Option<(Vec<String>, usize)>,
    pub file_picker: Option<FilePickerState>,
    pub url_opener: Box<dyn UrlOpener>,
    pub attachment_viewer: Option<AttachmentViewerState>,
    pub viewer_preview: Option<ViewerPreviewState>,
    pub viewer_zoom: u16,

    pub kh: KeybindHandler,

    pub show_logs: bool,

    pub contact_search_active: bool,
    pub contact_search: TextInput,
    pub filtered_chats: Vec<wr::JID>,

    pub should_quit: bool,

    /// Set while a logout confirmation is showing. When pending, the next
    /// input key resolves the prompt (confirm/cancel) instead of normal keys.
    pub pending_logout: bool,

    /// Set once the user confirmed logout and the async bridge logout is
    /// running off the event loop. While true, further input is ignored and
    /// the status bar shows "Logging out…" until `Event::LogoutResult`
    /// resolves the flow (cleanup + quit, or error).
    pub logout_in_progress: bool,

    /// Whether the section rail cursor sits on the Logout slot (below the
    /// three content sections). When true the content pane shows a logout
    /// placeholder instead of a chat/status/community view, and Enter on the
    /// rail calls `begin_logout_confirmation()`. Independent of
    /// `selected_section`, which keeps driving content selection.
    pub rail_on_logout: bool,

    /// Selection inside the logout confirmation menu (0 = Confirm logout,
    /// 1 = Cancel), rendered as a menu strip at the top right like the
    /// message menu. j/k move it, Enter confirms, Esc/n cancels.
    pub logout_menu_index: usize,

    pub tx: mpsc::Sender<AppInput>,
    pub rx: mpsc::Receiver<AppInput>,
    input_reader_control: Arc<(Mutex<InputReaderState>, Condvar)>,
}

impl Default for App<'_> {
    fn default() -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|err| {
            // Fallback for non-interactive environments (e.g. CI, piped stdio).
            log::warn!(
                "Failed to query terminal image capabilities; falling back to halfblocks: {err}"
            );
            Picker::halfblocks()
        });
        let default_protocol_type = picker.protocol_type();

        let project_dirs = ProjectDirs::from("com", "nullptr", "wptui").unwrap();
        let data_dir = project_dirs.data_dir();
        let cache_dir = project_dirs.cache_dir();
        Self::with_data_dir_and_picker_and_ports(
            data_dir,
            cache_dir,
            picker,
            default_protocol_type,
            Box::new(SystemClock),
            Box::new(NotifyRustNotifier),
        )
    }
}

impl App<'_> {
    /// Opens the system clipboard, falling back to an unavailable clipboard
    /// when no display server is reachable (for example on a headless
    /// machine) so the app can still run and paste surfaces a clear error.
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

    /// Constructs the full app with explicit storage directories instead of
    /// the user's real data/cache dirs. `App::default()` keeps using the
    /// real directories; tests use this factory with a fresh tempdir so they
    /// never open or write the real user database
    /// (`~/.local/share/wptui/whatsapp.db`).
    pub fn with_data_dir(data_dir: &Path, cache_dir: &Path) -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|err| {
            // Fallback for non-interactive environments (e.g. CI, piped stdio).
            log::warn!(
                "Failed to query terminal image capabilities; falling back to halfblocks: {err}"
            );
            Picker::halfblocks()
        });
        let default_protocol_type = picker.protocol_type();
        Self::with_data_dir_and_picker_and_ports(
            data_dir,
            cache_dir,
            picker,
            default_protocol_type,
            Box::new(SystemClock),
            Box::new(NotifyRustNotifier),
        )
    }

    pub fn with_data_dir_and_ports(
        data_dir: &Path,
        cache_dir: &Path,
        clock: Box<dyn Clock>,
        notifier: Box<dyn Notifier>,
    ) -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|err| {
            log::warn!(
                "Failed to query terminal image capabilities; falling back to halfblocks: {err}"
            );
            Picker::halfblocks()
        });
        let default_protocol_type = picker.protocol_type();
        Self::with_data_dir_and_picker_and_ports(
            data_dir,
            cache_dir,
            picker,
            default_protocol_type,
            clock,
            notifier,
        )
    }

    fn with_data_dir_and_picker_and_ports(
        data_dir: &Path,
        cache_dir: &Path,
        picker: Picker,
        default_protocol_type: ProtocolType,
        clock: Box<dyn Clock>,
        notifier: Box<dyn Notifier>,
    ) -> Self {
        fs::create_dir_all(data_dir).unwrap();

        let (tx, rx) = mpsc::channel::<AppInput>();

        let (clipboard_reader, clipboard_writer) = Self::open_clipboard_pair();

        Self {
            db_handler: DatabaseHandler::new(&data_dir.join("whatsapp.db")),
            media_path: data_dir.join("media"),
            whatsmeow_db: data_dir.join("whatsmeow.db"),
            clock,
            notifier,

            clipboard_reader,
            clipboard_writer,

            messages: HashMap::new(),
            message_actions: HashMap::new(),
            local_action_sequence: 0,
            message_action_diagnostics: MessageActionDiagnostics::new(false),
            reactions: HashMap::new(),
            chats: HashMap::new(),
            group_permissions: HashMap::new(),
            contacts: HashMap::new(),
            chat_messages: HashMap::new(),

            sorted_chats: Vec::new(),
            chat_list_state: ListState::default(),
            open_chat: None,
            open_status_contact: None,
            communities: Vec::new(),
            communities_unavailable: false,
            communities_loaded: false,

            status_contacts: Vec::new(),
            status_selection: ListState::default(),
            status_last_seen: HashMap::new(),

            message_list_state: MessageListState::default(),
            metadata: HashMap::new(),
            history_sync_percent: None,
            selected_presence: SelectedPresence::default(),
            presence_diagnostics: PresenceDiagnostics::default(),
            image_cache: HashMap::new(),
            image_cache_order: VecDeque::new(),
            audio_durations: HashMap::new(),
            message_height_cache: MessageHeightCache::default(),
            default_protocol_type,
            composer: Composer::default(),
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
            message_menu: None,
            reaction_picker: None,
            share_picker: None,
            url_picker: None,
            file_picker: None,
            url_opener: Box::new(SystemUrlOpener),
            attachment_viewer: None,
            viewer_preview: None,
            viewer_zoom: 100,

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
            input_reader_control: Arc::new((Mutex::new(InputReaderState::Running), Condvar::new())),
        }
    }
}

impl App<'_> {
    pub fn enable_message_action_diagnostics(&mut self, enabled: bool) {
        self.message_action_diagnostics = MessageActionDiagnostics::new(enabled);
    }

    pub fn enable_presence_diagnostics(&mut self, enabled: bool) {
        self.presence_diagnostics = PresenceDiagnostics::new(enabled);
    }

    pub fn handle_whatsapp_event(&mut self, event: wr::Event) -> bool {
        match event {
            wr::Event::AppStateSyncComplete => {
                self.get_contacts();
                self.load_communities();
                self.sort_chats();
                true
            }
            wr::Event::Chat {
                jid,
                last_message_time,
            } => {
                // History sync reports chats that may carry no messages. Keep
                // them so the chat list reflects the full account, not only
                // conversations that shipped a message in the sync batch.
                self.add_or_update_chat(
                    Chat {
                        jid,
                        last_message_time: (last_message_time > 0).then_some(last_message_time),
                    },
                    |chat| {
                        if last_message_time > 0 && Some(last_message_time) > chat.last_message_time
                        {
                            chat.last_message_time = Some(last_message_time);
                        }
                    },
                );
                self.sort_chats();
                true
            }
            wr::Event::LogoutResult(status) => match status {
                wr::LogoutStatus::LoggedOut | wr::LogoutStatus::NotLoggedIn => {
                    self.finish_logout();
                    true
                }
                wr::LogoutStatus::LocalOnly => {
                    // Remote revocation failed, so WhatsApp on the phone still
                    // lists this device. The local session is already gone;
                    // surface it instead of silently quitting, and let the
                    // user retry (a second logout resolves as NotLoggedIn and
                    // finishes) or remove the device manually.
                    log::warn!(
                        "Logout: device was not unlinked on the phone; remove it manually in WhatsApp → Linked devices"
                    );
                    self.pending_logout = false;
                    self.logout_in_progress = false;
                    self.logout_menu_index = 0;
                    self.unavailable(
                        "Logged out locally, but the device is still linked on the phone — remove it in WhatsApp (Settings → Linked devices), then log out again to finish",
                    );
                    true
                }
                wr::LogoutStatus::Failed => {
                    // Even the local cleanup failed. Surface it and keep running.
                    self.pending_logout = false;
                    self.logout_in_progress = false;
                    self.logout_menu_index = 0;
                    self.unavailable("Could not log out");
                    true
                }
            },
            wr::Event::SyncProgress(percent) => {
                self.history_sync_percent = Some(percent);
                true
            }
            wr::Event::Receipt {
                kind,
                chat,
                message_ids,
            } => {
                debug!(
                    "Received receipt: {:?} for chat: {:?} with messages: {:?}",
                    kind, chat, message_ids
                );
                for msg_id in message_ids {
                    if let Some(message) = self.messages.get_mut(&msg_id) {
                        message.info.read_by += 1;
                        self.db_handler.add_message(message);
                    }
                }
                true
            }
            wr::Event::Reaction {
                target_message_id,
                participant,
                text,
                ..
            } => {
                self.apply_reaction(&target_message_id, participant, text);
                true
            }
            wr::Event::Connected => {
                // Connected is emitted again after reconnects.  The first
                // probe may race group metadata hydration, so AppStateSyncComplete
                // below remains the authoritative follow-up refresh.
                self.load_communities();
                self.mark_presence_ready();
                true
            }
            wr::Event::MessageAction {
                action_id,
                target_message_id,
                chat,
                sender,
                kind,
                occurred_at,
                arrival_order,
            } => {
                self.apply_message_action(MessageAction {
                    action_id,
                    target_message_id,
                    chat,
                    sender,
                    kind: match kind {
                        wr::MessageActionKind::Edit { replacement } => {
                            MessageActionKind::Edit { replacement }
                        }
                        wr::MessageActionKind::Delete => MessageActionKind::Delete,
                    },
                    occurred_at,
                    arrival_order,
                });
                true
            }
        }
    }

    pub fn run(&mut self, phone: Option<String>) {
        self.db_handler.init();
        // Statuses expire 24h after posting (server-side). Prune the local
        // copies at startup so the DB and media dir do not accumulate them.
        let purged_status_media = self.db_handler.purge_expired_statuses(unix_now());
        remove_status_media_files(&self.media_path, &purged_status_media);
        if !purged_status_media.is_empty() {
            info!(
                "Purged {} expired status media files",
                purged_status_media.len()
            );
        }
        self.load_data_from_db();
        self.sort_chats();

        wr::new_client(self.whatsmeow_db.to_str().unwrap());

        {
            let tx = self.tx.clone();
            let diagnostics = self.message_action_diagnostics.clone();
            wr::set_log_handler(move |msg, level| {
                let level = match level {
                    0 => log::Level::Error,
                    1 => log::Level::Warn,
                    2 => log::Level::Info,
                    3 => log::Level::Debug,
                    _ => log::Level::Trace,
                };
                diagnostics.record_go_log(&msg);
                log::log!(level, "{msg}");
                tx.send(AppInput::Draw).unwrap();
            });
        }
        {
            let tx = self.tx.clone();
            wr::set_event_handler(move |event| {
                tx.send(AppInput::WhatsApp(event)).unwrap();
            })
        }
        {
            let tx = self.tx.clone();
            wr::set_presence_handler(move |update| {
                tx.send(AppInput::Presence(update)).unwrap();
            });
        }
        {
            let tx = self.tx.clone();
            wr::set_message_handler(move |message, is_sync| {
                tx.send(AppInput::Message { message, is_sync }).unwrap();
            });
        }

        // Single dedicated thread for all CGo downloads. Calling Go from many Rust-spawned
        // threads can crash even with a mutex; one long-lived worker avoids that.
        let (download_tx, download_rx) = mpsc::channel::<(wr::MessageId, wr::FileId)>();
        let media_path = self.media_path.to_owned();
        let app_tx = self.tx.clone();
        thread::spawn(move || {
            for (message_id, file_id) in download_rx {
                let result = wr::download_file(&file_id, &media_path);
                let state = if result.is_err() {
                    FileMeta::DownloadFailed
                } else {
                    FileMeta::Downloaded
                };
                app_tx
                    .send(AppInput::App(AppEvent::SetFileState(message_id, state)))
                    .unwrap();
            }
        });

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

        let mut terminal = match ratatui::try_init() {
            Ok(terminal) => terminal,
            Err(e) => {
                error!("Failed to initialize terminal UI: {e}");
                eprintln!("Failed to initialize terminal UI: {e}");
                let _ = self
                    .message_action_diagnostics
                    .write_report(std::io::stderr());
                return;
            }
        };

        {
            let tx = self.tx.clone();
            let input_reader_control = Arc::clone(&self.input_reader_control);
            thread::spawn(move || {
                loop {
                    {
                        let (state_lock, _) = &*input_reader_control;
                        let state = state_lock.lock().unwrap();
                        if *state == InputReaderState::Stopped {
                            return;
                        }
                    }

                    match event::poll(Duration::from_millis(50)) {
                        Ok(true) => match event::read() {
                            Ok(event) => {
                                if let Err(e) = tx.send(AppInput::Terminal(event)) {
                                    error!("Failed to send terminal event: {:?}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Failed to read terminal event: {e}");
                                thread::sleep(Duration::from_millis(50));
                            }
                        },
                        Ok(false) => {}
                        Err(e) => {
                            error!("Failed to poll terminal events: {e}");
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                }
            });
        }

        self.sync_selected_presence();
        if let Err(error) = terminal.draw(|frame| ui::draw(frame, self)) {
            error!("Failed to draw terminal UI: {error}");
            self.stop_input_reader();
            ratatui::restore();
            wr::disconnect();
            let _ = self
                .message_action_diagnostics
                .write_report(std::io::stderr());
            return;
        }

        loop {
            let now = self.now();
            let msg = match self.selected_presence.redraw_after(now) {
                Some(timeout) => match self.rx.recv_timeout(timeout) {
                    Ok(input) => Ok(input),
                    Err(mpsc::RecvTimeoutError::Timeout) => Ok(AppInput::Draw),
                    Err(mpsc::RecvTimeoutError::Disconnected) => Err(mpsc::RecvError),
                },
                None => self.rx.recv(),
            };
            // info!("Received message: {:?}", &msg);
            let should_draw = match msg {
                Ok(AppInput::App(event)) => match event {
                    AppEvent::SetFilePreview(message_id, file_path, img) => {
                        self.cache_file_preview(message_id.clone(), file_path, img);
                        if let Some(viewer) = self.attachment_viewer.as_mut()
                            && viewer.message_id == message_id
                        {
                            viewer.status = crate::app::events::ViewerStatus::Ready;
                        }

                        trace!("Set file preview for message: {:?}", message_id);

                        true
                    }
                    AppEvent::LoadViewerPreview(key) => {
                        if self
                            .viewer_preview
                            .as_ref()
                            .is_none_or(|state| state.key() != &key)
                        {
                            false
                        } else {
                            let tx = self.tx.clone();
                            let media_path = self.media_path.clone();
                            let picker = Arc::clone(&self.picker);
                            thread::spawn(move || {
                                let protocol = MediaRoot::new(&media_path)
                                    .and_then(|root| {
                                        root.media_file(std::path::Path::new(
                                            key.preview_path().as_ref(),
                                        ))
                                    })
                                    .ok()
                                    .and_then(|path| image::ImageReader::open(path).ok())
                                    .and_then(|reader| reader.decode().ok())
                                    .map(|image| {
                                        let mut protocol =
                                            picker.lock().unwrap().new_resize_protocol(image);
                                        protocol.resize_encode(
                                            &Resize::Scale(None),
                                            Rect::new(0, 0, key.width, key.height),
                                        );
                                        protocol
                                    });
                                let _ = tx
                                    .send(AppInput::App(AppEvent::SetViewerPreview(key, protocol)));
                            });
                            false
                        }
                    }
                    AppEvent::SetViewerPreview(key, protocol) => {
                        if self
                            .viewer_preview
                            .as_ref()
                            .is_some_and(|state| state.key() == &key)
                        {
                            self.viewer_preview = Some(match protocol {
                                Some(protocol) => ViewerPreviewState::Ready {
                                    key,
                                    protocol: Box::new(protocol),
                                },
                                None => ViewerPreviewState::Failed(key),
                            });
                            true
                        } else {
                            false
                        }
                    }
                    AppEvent::LoadFilePreview(message_id) => {
                        if !matches!(
                            self.metadata.get(&message_id),
                            Some(Metadata::File(FileMeta::Loading))
                        ) {
                            self.metadata
                                .insert(message_id.clone(), Metadata::File(FileMeta::Loading));
                            self.message_height_cache.invalidate(&message_id);

                            let tx = self.tx.clone();
                            let media_path = self.media_path.to_owned();
                            let picker = Arc::clone(&self.picker);

                            let file = match &self.messages.get(&message_id).unwrap().message {
                                wr::MessageContent::File(f) => Some(f.clone()),
                                _ => None,
                            };
                            if let Some(file) = file {
                                thread::spawn(move || {
                                    // For videos, generate a real thumbnail frame with ffmpeg
                                    // (videos/<id>.jpg) when the sidecar is missing or is still
                                    // the tiny embedded WhatsApp thumbnail, then load it.
                                    let preview_path = match file.kind {
                                        wr::FileKind::Video => {
                                            let video_rel = Path::new(file.path.as_ref());
                                            let sidecar_rel = video_rel.with_extension("jpg");
                                            let sidecar_abs = media_path.join(&sidecar_rel);
                                            if !has_decent_video_thumbnail(&sidecar_abs) {
                                                let video_abs = media_path.join(video_rel);
                                                generate_video_thumbnail(&video_abs, &sidecar_abs);
                                            }
                                            sidecar_rel.to_string_lossy().to_string()
                                        }
                                        _ => file.path.to_string(),
                                    };
                                    let path = std::path::Path::new(&preview_path);
                                    let image_res = MediaRoot::new(&media_path)
                                        .and_then(|root| root.media_file(path))
                                        .ok()
                                        .and_then(|path| image::ImageReader::open(path).ok())
                                        .and_then(|reader| reader.decode().ok());

                                    if let Some(mut image_src) = image_res {
                                        if matches!(file.kind, wr::FileKind::Video) {
                                            apply_video_play_marker(&mut image_src);
                                        }
                                        let mut img =
                                            picker.lock().unwrap().new_resize_protocol(image_src);
                                        let (preview_width, preview_height) =
                                            if matches!(file.kind, wr::FileKind::Video) {
                                                (VIDEO_WIDTH, VIDEO_HEIGHT)
                                            } else {
                                                (IMAGE_WIDTH, IMAGE_HEIGHT)
                                            };
                                        img.resize_encode(
                                            &Resize::Scale(None),
                                            Rect {
                                                x: 0,
                                                y: 0,
                                                width: preview_width as u16,
                                                height: preview_height as u16,
                                            },
                                        );

                                        tx.send(AppInput::App(AppEvent::SetFilePreview(
                                            message_id.clone(),
                                            file.path.clone(),
                                            img,
                                        )))
                                        .unwrap();
                                    } else if matches!(file.kind, wr::FileKind::Video) {
                                        // No thumbnail sidecar (e.g. old messages before the
                                        // feature existed) — show the 🎬 placeholder instead
                                        // of a failure message.
                                        tx.send(AppInput::App(AppEvent::SetFileState(
                                            message_id.clone(),
                                            FileMeta::Loaded,
                                        )))
                                        .unwrap();
                                    } else {
                                        tx.send(AppInput::App(AppEvent::SetFileState(
                                            message_id.clone(),
                                            FileMeta::LoadFailed,
                                        )))
                                        .unwrap();
                                    }
                                });
                            } else {
                                error!("Expected a file message for preview");
                            }
                        }
                        false // We will redraw after the preview is loaded
                    }
                    AppEvent::SetFileState(message_id, state) => {
                        if let Some(viewer) = self.attachment_viewer.as_mut()
                            && viewer.message_id == message_id
                        {
                            viewer.status = match &state {
                                FileMeta::Loaded | FileMeta::Downloaded => {
                                    crate::app::events::ViewerStatus::Ready
                                }
                                FileMeta::Loading | FileMeta::Downloading => {
                                    crate::app::events::ViewerStatus::Downloading
                                }
                                FileMeta::LoadFailed | FileMeta::DownloadFailed => {
                                    crate::app::events::ViewerStatus::Failed
                                }
                            };
                        }
                        self.metadata
                            .insert(message_id.clone(), Metadata::File(state));
                        self.message_height_cache.invalidate(&message_id);

                        if matches!(
                            self.metadata.get(&message_id),
                            Some(Metadata::File(FileMeta::Downloaded | FileMeta::Loaded))
                        ) {
                            self.spawn_audio_duration_probe_if_missing(&message_id);
                        }

                        true
                    }
                    AppEvent::SetAudioDuration(_message_id, path, duration) => {
                        if let Some(duration) = duration {
                            self.audio_durations.insert(path, duration);
                        }
                        true
                    }
                    AppEvent::ContactAvatar(result) => self.contact_avatars.apply(result),
                    AppEvent::ContactAvatarRefreshed { generation, jid } => {
                        self.contact_avatars.mark_refreshed(generation, jid)
                    }
                    AppEvent::DownloadFile(message_id, file_id) => {
                        if matches!(
                            self.metadata.get(&message_id),
                            Some(Metadata::File(FileMeta::Downloading))
                        ) {
                            false
                        } else {
                            self.metadata
                                .insert(message_id.clone(), Metadata::File(FileMeta::Downloading));
                            self.message_height_cache.invalidate(&message_id);
                            download_tx.send((message_id, file_id)).unwrap();
                            false
                        }
                    }
                    AppEvent::DownloadFileDone(message_id, state) => {
                        self.metadata
                            .insert(message_id.clone(), Metadata::File(state));
                        self.message_height_cache.invalidate(&message_id);
                        true
                    }
                },
                Ok(AppInput::WhatsApp(event)) => self.handle_whatsapp_event(event),
                Ok(AppInput::Message {
                    message: msg,
                    is_sync,
                }) => self.process_message(msg, is_sync),
                Ok(AppInput::Presence(update)) => self.handle_presence_update(update),
                Ok(AppInput::Terminal(event)) => {
                    self.on_terminal_event(event);
                    true
                }
                Ok(AppInput::Draw) => true,
                Err(_) => {
                    error!("Failed to receive input from channel");
                    true
                }
            };

            self.sync_selected_presence();

            if should_draw {
                if let Err(error) = terminal.draw(|frame| ui::draw(frame, self)) {
                    error!("Failed to draw terminal UI: {error}");
                    break;
                }
            }

            if self.should_quit {
                break;
            }
        }

        self.stop_input_reader();
        ratatui::restore();
        wr::disconnect();
        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        self.write_presence_diagnostics(&mut stderr);
        drop(stderr);
        let _ = self
            .message_action_diagnostics
            .write_report(std::io::stderr());
    }

    pub(crate) fn now(&self) -> i64 {
        now_or(0, &*self.clock)
    }

    fn stop_input_reader(&self) {
        let (state_lock, state_changed) = &*self.input_reader_control;
        let mut state = state_lock.lock().unwrap();
        *state = InputReaderState::Stopped;
        state_changed.notify_all();
    }

    /// The status contact currently open in the Status section's right
    /// pane (set by pressing Enter on a contact), mirroring `open_chat`.
    pub fn open_status_contact(&self) -> Option<wr::JID> {
        self.open_status_contact.clone()
    }

    pub fn apply_message_action(&mut self, action: MessageAction) {
        let target = action.target_message_id.clone();
        let base_exists = self.messages.contains_key(&target);
        let pending_local = self.pending_local_action_for(&action);
        let reconciliation_attempted = pending_local.is_some();
        let persistence = if let Some(local_action_id) = pending_local.as_deref() {
            self.db_handler
                .reconcile_message_action(local_action_id, &action)
        } else {
            self.db_handler.record_message_action(&action)
        };
        match persistence {
            MessageActionPersistence::Inserted => {
                self.message_actions
                    .entry(target.clone())
                    .or_default()
                    .push(action.clone());
            }
            MessageActionPersistence::Reconciled => {
                let local_action_id = pending_local
                    .as_ref()
                    .expect("reconciliation requires local action");
                let actions = self.message_actions.entry(target.clone()).or_default();
                actions.retain(|existing| existing.action_id.as_ref() != local_action_id.as_ref());
                actions.push(action.clone());
            }
            MessageActionPersistence::DuplicateActionID => {}
        }
        if matches!(action.kind, MessageActionKind::Delete)
            && !matches!(persistence, MessageActionPersistence::DuplicateActionID)
        {
            self.message_actions
                .entry(target.clone())
                .and_modify(|actions| {
                    actions.retain(|existing| matches!(existing.kind, MessageActionKind::Delete))
                });
        }
        let projection = (!matches!(persistence, MessageActionPersistence::DuplicateActionID))
            .then(|| self.refresh_message_projection(&target))
            .flatten()
            .unwrap_or("unchanged");
        let action_count = self.message_actions.get(&target).map_or(0, Vec::len);
        let kind = match &action.kind {
            MessageActionKind::Edit { .. } => "edit",
            MessageActionKind::Delete => "delete",
        };
        self.message_action_diagnostics.record(|| {
            format!(
                "source=rust kind={kind} action_id={} target_id={} base_exists={base_exists} persistence={persistence:?} reconciliation={} action_count={action_count} projection={projection}",
                identifier_for_log(&action.action_id),
                identifier_for_log(&target),
                if reconciliation_attempted {
                    "attempted"
                } else {
                    "none"
                },
            )
        });
        info!(
            "message action action_id={} target_id={} base_exists={} persistence={:?} action_count={}",
            action.action_id, target, base_exists, persistence, action_count
        );
    }

    pub(crate) fn record_local_message_edit(
        &mut self,
        message: &wr::Message,
        replacement: Arc<str>,
    ) {
        self.local_action_sequence = self.local_action_sequence.saturating_add(1);
        let occurred_at = now_or(message.info.timestamp, &*self.clock);
        self.apply_message_action(MessageAction {
            action_id: format!(
                "local-edit:{}:{}",
                message.info.id, self.local_action_sequence
            )
            .into(),
            target_message_id: message.info.id.clone(),
            chat: message.info.chat.clone(),
            sender: message.info.sender.clone(),
            kind: MessageActionKind::Edit { replacement },
            occurred_at,
            arrival_order: self.local_action_sequence,
        });
    }

    pub(crate) fn record_local_message_delete(&mut self, message: &wr::Message) {
        self.local_action_sequence = self.local_action_sequence.saturating_add(1);
        let occurred_at = now_or(message.info.timestamp, &*self.clock);
        self.apply_message_action(MessageAction {
            action_id: format!(
                "local-delete:{}:{}",
                message.info.id, self.local_action_sequence
            )
            .into(),
            target_message_id: message.info.id.clone(),
            chat: message.info.chat.clone(),
            sender: message.info.sender.clone(),
            kind: MessageActionKind::Delete,
            occurred_at,
            arrival_order: self.local_action_sequence,
        });
    }

    pub fn select_chat(&mut self, jid: Option<wr::JID>) {
        let target_list = self.visible_chat_rows();

        if let Some(jid) = jid
            && let Some(index) = target_list
                .iter()
                .position(|row| row.target == jid || row.members.contains(&jid))
        {
            self.chat_list_state.select(Some(index));
        } else if !target_list.is_empty() {
            self.chat_list_state.select(Some(0));
        } else {
            self.chat_list_state.select(None);
        }
    }

    fn update_filtered_chats(&mut self) {
        self.filtered_chats = self
            .visible_chat_rows()
            .into_iter()
            .map(|row| row.target)
            .collect();

        if !self.filtered_chats.is_empty() {
            self.chat_list_state.select(Some(0));
        } else {
            self.chat_list_state.select(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::notifications::notification_is_muted;
    use crate::app::presence::PresenceMarker;

    #[derive(Debug)]
    struct FixedClock(Option<i64>);

    impl FixedClock {
        fn new(value: i64) -> Self {
            Self(Some(value))
        }
    }

    impl Clock for FixedClock {
        fn unix_seconds(&self) -> Option<i64> {
            self.0
        }
    }

    #[derive(Clone)]
    struct MutableClock(Arc<Mutex<Option<i64>>>);

    impl MutableClock {
        fn new(value: Option<i64>) -> Self {
            Self(Arc::new(Mutex::new(value)))
        }

        fn set(&self, value: Option<i64>) {
            *self.0.lock().unwrap() = value;
        }
    }

    impl Clock for MutableClock {
        fn unix_seconds(&self) -> Option<i64> {
            *self.0.lock().unwrap()
        }
    }

    #[derive(Clone, Default)]
    struct RecordingNotifier {
        notifications: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl Notifier for RecordingNotifier {
        fn show(&self, notification: &NotificationProjection) -> Result<(), String> {
            self.notifications
                .lock()
                .unwrap()
                .push((notification.summary.to_string(), notification.body.clone()));
            Ok(())
        }
    }

    /// Test-only App wrapper: points every storage path at a fresh
    /// tempdir (so tests never open the real user database) and stops
    /// the DatabaseHandler background writer thread on drop (so a leaked
    /// thread can never panic while holding the process-global write lock
    /// and poison later tests).
    struct TestApp {
        app: App<'static>,
        _dir: tempfile::TempDir,
    }

    impl TestApp {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let app: App<'static> = App::with_data_dir(dir.path(), dir.path());
            // Full schema up front: the background writer thread drains the
            // queues asynchronously and must never hit a missing table.
            app.db_handler.init();
            Self { app, _dir: dir }
        }

        fn with_database(path: &std::path::Path) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut app: App<'static> = App::with_data_dir(dir.path(), dir.path());
            app.db_handler.init();
            std::mem::replace(
                &mut app.db_handler,
                DatabaseHandler::new(&path.join("app.db")),
            )
            .stop();
            app.db_handler.init();
            Self { app, _dir: dir }
        }
    }

    impl Drop for TestApp {
        fn drop(&mut self) {
            self.app.db_handler.stop();
        }
    }

    fn app_with_ports<C, N>(clock: C, notifier: N) -> TestApp
    where
        C: Clock + 'static,
        N: Notifier + 'static,
    {
        let dir = tempfile::tempdir().unwrap();
        let app = App::with_data_dir_and_ports(
            dir.path(),
            dir.path(),
            Box::new(clock),
            Box::new(notifier),
        );
        app.db_handler.init();
        TestApp { app, _dir: dir }
    }

    impl std::ops::Deref for TestApp {
        type Target = App<'static>;
        fn deref(&self) -> &Self::Target {
            &self.app
        }
    }

    impl std::ops::DerefMut for TestApp {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.app
        }
    }

    #[test]
    fn actions_replay_in_stable_order_and_delete_wins_the_display_status() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = app_with_database(directory.path());
        let chat = wr::JID::from("chat@example.test".to_owned());
        app.add_message(message(&chat, "target", 1));
        for (id, replacement, order) in [("edit-2", "second", 2), ("edit-1", "first", 1)] {
            app.apply_message_action(MessageAction {
                action_id: id.into(),
                target_message_id: "target".into(),
                chat: chat.clone(),
                sender: chat.clone(),
                kind: MessageActionKind::Edit {
                    replacement: replacement.into(),
                },
                occurred_at: 2,
                arrival_order: order,
            });
        }
        app.apply_message_action(MessageAction {
            action_id: "delete".into(),
            target_message_id: "target".into(),
            chat: chat.clone(),
            sender: chat,
            kind: MessageActionKind::Delete,
            occurred_at: 3,
            arrival_order: 3,
        });

        assert!(
            matches!(&app.messages["target"].message, wr::MessageContent::Text(text) if text.as_ref() == "This message was deleted."),
            "a deleted message must not retain its latest effective body"
        );
        assert_eq!(
            app.message_status(&"target".into()),
            MessageStatus {
                edited: false,
                deleted: true
            }
        );
        assert_eq!(
            app.sorted_message_actions(&"target".into())
                .iter()
                .map(|action| action.action_id.as_ref())
                .collect::<Vec<_>>(),
            ["delete"]
        );
        app.db_handler.stop();
    }

    #[test]
    fn action_before_base_message_is_applied_when_the_base_arrives() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = app_with_database(directory.path());
        let chat = wr::JID::from("chat@example.test".to_owned());
        app.apply_message_action(MessageAction {
            action_id: "edit".into(),
            target_message_id: "target".into(),
            chat: chat.clone(),
            sender: chat.clone(),
            kind: MessageActionKind::Edit {
                replacement: "replacement".into(),
            },
            occurred_at: 2,
            arrival_order: 1,
        });
        app.add_message(message(&chat, "target", 1));

        assert!(
            matches!(&app.messages["target"].message, wr::MessageContent::Text(text) if text.as_ref() == "replacement")
        );
        app.db_handler.stop();
    }

    #[test]
    fn local_edit_is_projected_persisted_and_not_duplicated_by_its_inbound_echo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.db");
        let chat = wr::JID::from("chat@example.test".to_owned());
        let mut original = message(&chat, "target", 1);
        original.info.is_from_me = true;
        let mut app = app_with_database(directory.path());
        app.db_handler.add_message(&original);
        app.add_message(original.clone());

        app.record_local_message_edit(&original, "replacement".into());
        assert!(matches!(
            &app.messages["target"].message,
            wr::MessageContent::Text(text) if text.as_ref() == "replacement"
        ));
        assert!(app.message_status(&"target".into()).edited);
        assert_eq!(app.message_actions["target"].len(), 1);

        app.apply_message_action(MessageAction {
            action_id: "server-edit".into(),
            target_message_id: "target".into(),
            chat: chat.clone(),
            sender: chat.clone(),
            kind: MessageActionKind::Edit {
                replacement: "replacement".into(),
            },
            occurred_at: 2,
            arrival_order: 2,
        });
        assert_eq!(app.message_actions["target"].len(), 1);
        app.db_handler.stop();

        let mut reloaded = TestApp::new();
        std::mem::replace(&mut reloaded.db_handler, DatabaseHandler::new(&path)).stop();
        reloaded.load_data_from_db();
        assert!(matches!(
            &reloaded.messages["target"].message,
            wr::MessageContent::Text(text) if text.as_ref() == "replacement"
        ));
        assert!(reloaded.message_status(&"target".into()).edited);
        assert_eq!(reloaded.message_actions["target"].len(), 1);
        reloaded.db_handler.stop();
    }

    fn message(chat: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
        wr::Message {
            info: wr::MessageInfo {
                id: id.into(),
                chat: chat.clone(),
                sender: chat.clone(),
                timestamp,
                forwarding: Default::default(),
                is_from_me: false,
                quote_id: None,
                read_by: 0,
            },
            message: wr::MessageContent::Text(id.into()),
        }
    }

    fn app_with_database(path: &std::path::Path) -> TestApp {
        TestApp::with_database(path)
    }

    #[test]
    fn local_message_actions_use_injected_clock_and_message_timestamp_fallback() {
        let chat = wr::JID::from("chat@g.us".to_owned());
        let message = message(&chat, "local-action", 77);
        let mut app = app_with_ports(FixedClock::new(1_700_000_000), RecordingNotifier::default());
        app.record_local_message_edit(&message, "edited".into());
        assert_eq!(
            app.message_actions[&message.info.id][0].occurred_at,
            1_700_000_000
        );

        let mut app = app_with_ports(FixedClock::new(1_700_000_000), RecordingNotifier::default());
        app.record_local_message_delete(&message);
        assert_eq!(
            app.message_actions[&message.info.id][0].occurred_at,
            1_700_000_000
        );

        let mut app = app_with_ports(FixedClock(None), RecordingNotifier::default());
        app.record_local_message_delete(&message);
        assert_eq!(app.message_actions[&message.info.id][0].occurred_at, 77);
    }

    #[test]
    fn injected_clock_preserves_mute_boundary_and_presence_timing() {
        let chat = wr::JID::from("alice@s.whatsapp.net".to_owned());
        let clock = MutableClock::new(Some(1_000));
        let mut app = app_with_ports(clock.clone(), RecordingNotifier::default());
        app.open_chat = Some(chat.clone());
        let now = app.now();
        app.selected_presence.select(Some(chat.clone()), now);
        app.selected_presence.update(&chat, true, 0, now);

        assert!(!notification_is_muted(true, 1_000, app.now()));
        let now = app.now();
        assert_eq!(
            app.selected_presence.marker(Some(&chat), now),
            Some(PresenceMarker::RecentlyOffline)
        );
        assert_eq!(
            app.selected_presence.redraw_after(app.now()),
            Some(Duration::from_secs(300))
        );

        clock.set(Some(1_299));
        let now = app.now();
        assert_eq!(
            app.selected_presence.marker(Some(&chat), now),
            Some(PresenceMarker::RecentlyOffline)
        );
        assert_eq!(
            app.selected_presence.redraw_after(app.now()),
            Some(Duration::from_secs(1))
        );
        clock.set(Some(1_300));
        assert!(notification_is_muted(true, 1_301, app.now()));
        assert!(!notification_is_muted(true, 1_300, app.now()));
        let now = app.now();
        assert_eq!(
            app.selected_presence.marker(Some(&chat), now),
            Some(PresenceMarker::Offline)
        );
        assert_eq!(app.selected_presence.redraw_after(app.now()), None);

        clock.set(None);
        assert_eq!(app.now(), 0);
    }

    #[test]
    fn injected_clock_reaches_presence_and_ui_marker_path() {
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};

        let chat = wr::JID::from("alice@s.whatsapp.net".to_owned());
        let mut app = app_with_ports(FixedClock::new(1_700_000_000), RecordingNotifier::default());
        app.contacts.insert(chat.clone(), "Alice".into());
        app.open_chat = Some(chat.clone());
        let now = app.now();
        app.selected_presence.select(Some(chat.clone()), now);
        app.selected_presence.update(&chat, true, 0, now);

        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        terminal
            .draw(|frame| crate::ui::render_chats(frame, &mut app, Rect::new(0, 0, 40, 8)))
            .unwrap();
        let row = terminal
            .backend()
            .buffer()
            .content()
            .chunks(40)
            .next()
            .unwrap();
        let title = row.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(title.contains("● Alice"), "rendered title: {title:?}");
    }

    #[test]
    fn status_broadcast_chat_never_appears_in_the_chat_list() {
        let mut app = TestApp::new();
        let chat = wr::JID::from("chat@example.test".to_owned());
        let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
        app.add_message(message(&chat, "c1", 100));
        let mut status = message(&alice, "s1", 200);
        status.info.chat = wr::JID::from(STATUS_BROADCAST_CHAT.to_owned());
        app.add_message(status);
        app.sort_chats();

        assert!(app.sorted_chats.contains(&chat));
        assert!(
            !app.sorted_chats
                .iter()
                .any(|jid| jid.0.as_ref() == STATUS_BROADCAST_CHAT)
        );
    }

    #[test]
    fn section_rail_navigates_through_sections_into_logout_and_back() {
        use crate::app::FocusPane;
        use crate::app::actions::{AppAction, Section};

        let mut app = TestApp::new();
        app.focus_pane = FocusPane::SectionRail;
        app.selected_section = Section::Chats;
        app.rail_on_logout = false;

        // j: Chats -> Status -> Communities -> Logout (flag set) -> wraps to Chats.
        app.dispatch_action(AppAction::SelectNext);
        assert_eq!(app.selected_section, Section::Status);
        assert!(!app.rail_on_logout);
        app.dispatch_action(AppAction::SelectNext);
        assert_eq!(app.selected_section, Section::Communities);
        assert!(!app.rail_on_logout);
        app.dispatch_action(AppAction::SelectNext);
        assert!(app.rail_on_logout);
        app.dispatch_action(AppAction::SelectNext);
        assert_eq!(app.selected_section, Section::Chats);
        assert!(!app.rail_on_logout);

        // k: Chats wraps backward up to Logout.
        app.dispatch_action(AppAction::SelectPrevious);
        assert!(app.rail_on_logout);
        app.dispatch_action(AppAction::SelectPrevious);
        assert_eq!(app.selected_section, Section::Communities);
        assert!(!app.rail_on_logout);

        // G (jump_bottom) lands on Logout; gg (jump_top) returns to Chats.
        app.dispatch_action(AppAction::JumpBottom);
        assert!(app.rail_on_logout);
        app.dispatch_action(AppAction::JumpTop);
        assert_eq!(app.selected_section, Section::Chats);
        assert!(!app.rail_on_logout);
    }

    #[test]
    fn pressing_enter_on_the_rail_logout_slot_begins_confirmation() {
        use crate::app::FocusPane;
        use crate::app::actions::{AppAction, Section};
        use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

        let mut app = TestApp::new();
        app.focus_pane = FocusPane::SectionRail;
        app.selected_section = Section::Chats;
        app.rail_on_logout = false;

        // Enter on a normal section does not start logout confirmation.
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        app.on_terminal_event(Event::Key(enter));
        assert!(!app.pending_logout);

        // Move the rail to the Logout slot and press Enter: confirmation starts.
        app.dispatch_action(AppAction::JumpBottom);
        assert!(app.rail_on_logout);
        app.on_terminal_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(app.pending_logout);
        assert_eq!(app.focus_pane, FocusPane::SectionRail);
        assert_eq!(app.selected_section, Section::Communities);
    }

    #[test]
    fn logout_confirmation_menu_navigates_and_cancels() {
        use crate::app::FocusPane;
        use crate::app::actions::AppAction;
        use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

        let mut app = TestApp::new();
        app.focus_pane = FocusPane::SectionRail;
        app.selected_section = crate::app::actions::Section::Chats;
        app.dispatch_action(AppAction::JumpBottom);
        assert!(app.rail_on_logout);

        // Enter on the rail Logout slot opens the confirmation menu.
        app.on_terminal_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        assert!(app.pending_logout);
        assert_eq!(app.logout_menu_index, 0);

        // j / k move the selection between Confirm and Cancel (2 items).
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

        // Esc cancels without starting logout (no bridge call).
        app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(!app.pending_logout);
        assert!(!app.logout_in_progress);
        assert_eq!(app.logout_menu_index, 0);
    }
}
