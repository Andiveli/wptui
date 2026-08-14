use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    sync::Condvar,
    sync::Mutex,
};
use std::{fs, thread};

pub mod actions;
pub mod composer;
pub mod contact_avatars;
pub mod events;
pub mod inputs;
pub mod message_action_diagnostics;
pub mod presence;

pub use crate::app;
use crate::app::actions::{
    ActionNotice, ClipboardReader, ClipboardWriter, ConversationMode, FocusPane, MessageEditor,
    MessageForwarder, MessageMenuAction, MessageReactor, MessageRevoker, PaneVisibility, Section,
    SystemClipboardReader, SystemClipboardWriter, SystemUrlOpener, UnavailableClipboardReader,
    UnavailableClipboardWriter, UrlOpener, WhatsAppMessageEditor, WhatsAppMessageForwarder,
    WhatsAppMessageReactor, WhatsAppMessageRevoker,
};
use crate::app::composer::Composer;
use crate::app::contact_avatars::ContactAvatars;
use crate::app::events::{AppEvent, AppInput, AttachmentViewerState, ViewerPreviewState};
use crate::app::message_action_diagnostics::{MessageActionDiagnostics, identifier_for_log};
use crate::app::presence::{PresenceDiagnostics, SelectedPresence, jid_for_log};
use crate::db;
use crate::file_picker::FilePickerState;
use crate::key_handler::KeybindHandler;
use crate::media::MediaRoot;
use crate::ui;
// use crate::key_handler;

use arboard::Clipboard;
use db::{DatabaseHandler, MessageActionPersistence};
use directories::ProjectDirs;
use log::{debug, error, info, trace, warn};
use notify_rust::Notification;
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

/// The synthetic chat that carries WhatsApp status broadcasts. Each message's
/// `info.sender` is the contact who posted the status.
pub const STATUS_BROADCAST_CHAT: &str = "status@broadcast";

pub trait Clock {
    fn unix_seconds(&self) -> Option<i64>;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn unix_seconds(&self) -> Option<i64> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs() as i64)
    }
}

pub struct NotificationProjection {
    pub summary: Arc<str>,
    pub body: String,
}

pub trait Notifier {
    fn show(&self, notification: &NotificationProjection) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct NotifyRustNotifier;

impl Notifier for NotifyRustNotifier {
    fn show(&self, notification: &NotificationProjection) -> Result<(), String> {
        Notification::new()
            .summary(&notification.summary)
            .body(&notification.body)
            .show()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessageActionKind {
    Edit { replacement: Arc<str> },
    Delete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageAction {
    pub action_id: Arc<str>,
    pub target_message_id: wr::MessageId,
    pub chat: wr::JID,
    pub sender: wr::JID,
    pub kind: MessageActionKind,
    pub occurred_at: i64,
    pub arrival_order: u64,
}

pub const DELETED_MESSAGE_TEXT: &str = "This message was deleted.";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MessageStatus {
    pub edited: bool,
    pub deleted: bool,
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

pub struct SharePicker {
    contacts: Vec<wr::JID>,
    labels: HashMap<wr::JID, String>,
    pub query: String,
    pub selected: usize,
    pub offset: usize,
    viewport_height: usize,
    selected_contacts: HashSet<wr::JID>,
}

impl SharePicker {
    pub fn new(
        mut contacts: Vec<wr::JID>,
        labels: HashMap<wr::JID, String>,
        recency: HashMap<wr::JID, i64>,
    ) -> Self {
        contacts.sort_by(|left, right| {
            recency
                .get(right)
                .unwrap_or(&i64::MIN)
                .cmp(recency.get(left).unwrap_or(&i64::MIN))
                .then_with(|| left.0.cmp(&right.0))
        });
        Self {
            contacts,
            labels,
            query: String::new(),
            selected: 0,
            offset: 0,
            viewport_height: 1,
            selected_contacts: HashSet::new(),
        }
    }
    pub fn visible_contacts(&self) -> Vec<&wr::JID> {
        let query = self.query.to_lowercase();
        self.contacts
            .iter()
            .filter(|jid| {
                query.is_empty()
                    || jid.0.to_lowercase().contains(&query)
                    || self
                        .labels
                        .get(*jid)
                        .is_some_and(|name| name.to_lowercase().contains(&query))
            })
            .collect()
    }
    pub fn selected_count(&self) -> usize {
        self.selected_contacts.len()
    }
    pub fn is_selected(&self, jid: &wr::JID) -> bool {
        self.selected_contacts.contains(jid)
    }
    pub fn destinations(&self) -> Vec<wr::JID> {
        self.contacts
            .iter()
            .filter(|jid| self.is_selected(jid))
            .cloned()
            .collect()
    }
    pub fn viewport(&self) -> std::ops::Range<usize> {
        let end = self.visible_contacts().len();
        let height = self.viewport_height.max(1).min(end);
        let start = self.offset.min(end.saturating_sub(height));
        start..start.saturating_add(height)
    }
    fn clamp_selection(&mut self) {
        self.selected = self
            .selected
            .min(self.visible_contacts().len().saturating_sub(1));
        self.offset = self.offset.min(self.selected);
    }
    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = height.max(1);
        self.keep_selected_visible();
    }
    pub fn keep_selected_visible(&mut self) {
        let height = self.viewport_height.max(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        }
        if self.selected >= self.offset.saturating_add(height) {
            self.offset = self.selected + 1 - height;
        }
    }
    pub fn reset_search_position(&mut self) {
        self.selected = 0;
        self.offset = 0;
    }
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
            contacts: HashMap::new(),
            chat_messages: HashMap::new(),

            sorted_chats: Vec::new(),
            chat_list_state: ListState::default(),
            open_chat: None,
            open_status_contact: None,

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

    pub fn touch_image_cache(&mut self, path: &Arc<str>) {
        if self.image_cache.contains_key(path) {
            self.image_cache_order.retain(|cached| cached != path);
            self.image_cache_order.push_back(path.clone());
        }
    }

    fn mark_evicted_preview_reloadable(&mut self, path: &Arc<str>) {
        for (id, message) in &self.messages {
            if matches!(&message.message, wr::MessageContent::File(file) if file.path == *path)
                && matches!(
                    self.metadata.get(id),
                    Some(Metadata::File(FileMeta::Loaded))
                )
            {
                self.metadata
                    .insert(id.clone(), Metadata::File(FileMeta::Downloaded));
                self.message_height_cache.invalidate(id);
            }
        }
    }

    /// Spawns a background probe for the audio duration of `message_id` once
    /// its file is on disk. No-op for non-audio messages and for paths that
    /// already have a cached duration.
    fn spawn_audio_duration_probe_if_missing(&self, message_id: &wr::MessageId) {
        let Some(file) = (match self
            .messages
            .get(message_id)
            .map(|message| &message.message)
        {
            Some(wr::MessageContent::File(file)) => Some(file.clone()),
            _ => None,
        }) else {
            return;
        };
        if !matches!(file.kind, wr::FileKind::Audio)
            || self.audio_durations.contains_key(file.path.as_ref())
        {
            return;
        }
        let tx = self.tx.clone();
        let media_path = self.media_path.to_owned();
        let message_id = message_id.clone();
        thread::spawn(move || {
            let absolute = media_path.join(file.path.as_ref());
            let duration = probe_audio_duration(&absolute);
            tx.send(AppInput::App(AppEvent::SetAudioDuration(
                message_id.clone(),
                file.path,
                duration,
            )))
            .unwrap();
        });
    }

    pub fn apply_reaction(&mut self, target: &wr::MessageId, participant: wr::JID, text: Arc<str>) {
        self.db_handler
            .record_reaction(target, participant.clone(), text.clone());
        if text.is_empty() {
            if let Some(reactions) = self.reactions.get_mut(target) {
                reactions.remove(&participant);
                if reactions.is_empty() {
                    self.reactions.remove(target);
                }
            }
        } else {
            self.reactions
                .entry(target.clone())
                .or_default()
                .insert(participant, text);
        }
        self.message_height_cache.invalidate(target);
    }

    pub fn handle_whatsapp_event(&mut self, event: wr::Event) -> bool {
        match event {
            wr::Event::AppStateSyncComplete => {
                self.get_contacts();
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
                self.selected_presence.ready();
                self.presence_diagnostics
                    .record(|| "self presence available: ready".to_owned());
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
                        const IMAGE_CACHE_CAPACITY: usize = 50;
                        if !self.image_cache.contains_key(&file_path)
                            && self.image_cache.len() >= IMAGE_CACHE_CAPACITY
                            && let Some(oldest) = self.image_cache_order.pop_front()
                        {
                            self.image_cache.remove(&oldest);
                            self.mark_evicted_preview_reloadable(&oldest);
                        }
                        self.image_cache.insert(file_path.clone(), img);
                        self.image_cache_order.retain(|path| path != &file_path);
                        self.image_cache_order.push_back(file_path.clone());
                        self.metadata
                            .insert(message_id.clone(), Metadata::File(FileMeta::Loaded));
                        self.message_height_cache.invalidate(&message_id);
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
                Ok(AppInput::Presence(wr::PresenceUpdate {
                    from,
                    unavailable,
                    last_seen,
                })) => self
                    .selected_presence
                    .update(&from, unavailable, last_seen, self.now()),
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
        let _ = self.presence_diagnostics.write_report(&mut stderr);
        let raw_report = wr::drain_raw_presence_diagnostics();
        let _ = self
            .presence_diagnostics
            .write_raw_report(&mut stderr, raw_report.as_deref());
        drop(stderr);
        let _ = self
            .message_action_diagnostics
            .write_report(std::io::stderr());
    }

    fn sync_selected_presence(&mut self) {
        let selected = self.open_chat.clone();
        let now = self.now();
        if self.selected_presence.select(selected, now) {
            let selected = self
                .selected_presence
                .selected()
                .map(jid_for_log)
                .unwrap_or_else(|| "none".to_owned());
            self.presence_diagnostics
                .record(|| format!("selected canonical jid={selected}"));
        }
        if let Some(jid) = self.selected_presence.subscription_due(now) {
            let diagnostic_jid = jid_for_log(&jid);
            self.presence_diagnostics
                .record(|| format!("presence subscription attempt: jid={diagnostic_jid}"));
            info!("Presence subscription attempt: jid={}", jid_for_log(&jid));
            let result = wr::subscribe_presence(&jid);
            let retry_delay = self
                .selected_presence
                .subscription_result(&jid, result, now);
            let diagnostic_jid = jid_for_log(&jid);
            self.presence_diagnostics.record(|| {
                if result == wr::SubscribePresenceResult::NoPrivacyToken {
                    format!(
                        "presence subscription result: jid={diagnostic_jid}, result=rejected: no privacy token"
                    )
                } else if let Some(delay) = retry_delay {
                    format!(
                        "presence subscription result: jid={diagnostic_jid}, result=rejected, retry_in={delay}s"
                    )
                } else {
                    format!(
                        "presence subscription result: jid={diagnostic_jid}, result=accepted"
                    )
                }
            });
            info!(
                "Presence subscription result: jid={}, result={}",
                jid_for_log(&jid),
                match result {
                    wr::SubscribePresenceResult::Accepted => "accepted",
                    wr::SubscribePresenceResult::NoPrivacyToken => {
                        "rejected: no privacy token"
                    }
                    wr::SubscribePresenceResult::Rejected => "rejected",
                }
            );
        }
    }

    pub fn load_data_from_db(&mut self) {
        info!("Reading database");
        for chat in self.db_handler.get_chats() {
            self.chats.insert(chat.jid.clone(), chat);
        }
        for (jid, name) in self.db_handler.get_contacts() {
            self.contacts.insert(jid, name);
        }

        for action in self.db_handler.get_message_actions() {
            self.local_action_sequence = self.local_action_sequence.max(
                action
                    .action_id
                    .rsplit_once(':')
                    .and_then(|(_, sequence)| sequence.parse().ok())
                    .unwrap_or_default(),
            );
            self.message_actions
                .entry(action.target_message_id.clone())
                .or_default()
                .push(action);
        }

        for message in self.db_handler.get_messages() {
            self.add_message_without_sort(message);
        }
        let chat_ids = self.chat_messages.keys().cloned().collect::<Vec<_>>();
        for chat_id in chat_ids {
            self.sort_chat_messages(chat_id);
        }
        for (message_id, participant, emoji) in self.db_handler.get_reactions() {
            self.reactions
                .entry(message_id)
                .or_default()
                .insert(participant, emoji);
        }
        warn!(
            "Finished reading database with {} chats and {} messages",
            self.chats.len(),
            self.messages.len()
        );
    }

    /// Display name for a JID (chat or sender). Falls back to the JID string if not in contacts.
    pub fn contact_name(&self, jid: &wr::JID) -> Arc<str> {
        self.contacts
            .get(jid)
            .cloned()
            .unwrap_or_else(|| jid.0.clone())
    }

    fn process_message(&mut self, message: wr::Message, is_sync: bool) -> bool {
        self.process_message_with_lookup(message, is_sync, wr::get_chat_settings)
    }

    fn process_message_with_lookup(
        &mut self,
        message: wr::Message,
        is_sync: bool,
        lookup: impl FnMut(&wr::JID) -> wr::ChatSettings,
    ) -> bool {
        if !is_sync {
            self.handle_notification_with_lookup(&message, lookup);
        }

        self.db_handler.add_message(&message);
        self.add_message(message);

        let chat_jid = self.get_selected_chat();
        self.sort_chats();
        self.select_chat(chat_jid);
        !is_sync
    }

    fn handle_notification_with_lookup(
        &self,
        message: &wr::Message,
        mut lookup: impl FnMut(&wr::JID) -> wr::ChatSettings,
    ) {
        if !self.should_notify(message) {
            return;
        }

        let chat_settings = lookup(&message.info.chat);
        info!(
            "Chat settings for {:?}: {:?}",
            message.info.chat, chat_settings
        );
        if chat_settings.found && notification_is_muted(true, chat_settings.muted_until, self.now())
        {
            return;
        }

        let notification =
            notification_projection(message, self.contact_name(&message.info.sender));
        if let Err(err) = self.notifier.show(&notification) {
            error!("Failed to show desktop notification: {err}");
        }
    }

    /// Desktop notifications are suppressed for the user's own messages and
    /// for status broadcasts (statuses surface in the Status section instead).
    fn should_notify(&self, message: &wr::Message) -> bool {
        notification_eligibility(message)
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

    pub fn get_selected_chat(&self) -> Option<wr::JID> {
        self.chat_list_state.selected().map(|index| {
            if self.contact_search.input.is_empty() {
                self.sorted_chats[index].clone()
            } else {
                self.filtered_chats[index].clone()
            }
        })
    }

    pub fn open_chat(&self) -> Option<wr::JID> {
        self.open_chat.clone()
    }

    /// The status contact currently open in the Status section's right
    /// pane (set by pressing Enter on a contact), mirroring `open_chat`.
    pub fn open_status_contact(&self) -> Option<wr::JID> {
        self.open_status_contact.clone()
    }

    /// Opens the currently highlighted chat: it becomes the rendered
    /// conversation, composer target, and presence subscription.
    pub fn open_selected_chat(&mut self) {
        if let Some(chat) = self.get_selected_chat() {
            self.open_chat = Some(chat.clone());
            self.sort_chat_messages(chat);
            self.message_list_state.reset();
        }
    }

    pub fn is_status_chat(jid: &wr::JID) -> bool {
        jid.0.as_ref() == STATUS_BROADCAST_CHAT
    }

    /// True for group conversations (JIDs of the form `number@g.us`).
    pub fn is_group_chat(jid: &wr::JID) -> bool {
        jid.0.as_ref().ends_with("@g.us")
    }

    /// Opens a direct conversation by JID, regardless of whether it already
    /// appears in the chat list. The in-memory entry is created here so the
    /// recipient shows up as a row; the database row is created on the first
    /// real message, so an empty conversation is never persisted.
    pub fn open_chat_by_jid(&mut self, jid: wr::JID) {
        self.chats.entry(jid.clone()).or_insert_with(|| Chat {
            jid: jid.clone(),
            last_message_time: None,
        });
        self.sort_chats();
        self.open_chat = Some(jid.clone());
        self.sort_chat_messages(jid);
        self.message_list_state.reset();
    }

    /// Jumps to a private conversation with the sender of the selected
    /// message. Only meaningful inside a group, on a message sent by
    /// someone else (the sender JID is the group participant).
    pub fn reply_privately(&mut self) {
        let Some(message) = self.selected_message().cloned() else {
            return self.unavailable("Reply in private is not available");
        };
        if message.info.is_from_me {
            return self.unavailable("Reply in private is not available for your own messages");
        }
        let Some(chat) = self.open_chat() else {
            return self.unavailable("Reply in private is not available");
        };
        if !Self::is_group_chat(&chat) {
            return self.unavailable("Reply in private is only available in groups");
        }
        // Group participants can be a LID while the real direct chat lives
        // under its phone number; resolve so we open/send to the stored chat
        // instead of an empty LID-keyed thread.
        let target = wr::resolve_dm_chat(&message.info.sender)
            .unwrap_or_else(|| message.info.sender.clone());
        let name = self.contact_name(&target).to_string();
        self.open_chat_by_jid(target);
        // Mirror the status "reply to DM" flow: switch to the Chats view,
        // focus the conversation, quote the original message and drop the
        // user straight into the composer for the private reply.
        self.selected_section = Section::Chats;
        self.composer.quote = Some(message);
        self.conversation_mode = ConversationMode::ComposerEditing;
        self.focus_pane = FocusPane::Conversation;
        self.action_notice = Some(ActionNotice::ReplyPrivatelyNamed(name));
    }

    /// Re-derives `status_contacts` from the `status@broadcast` chat and
    /// keeps the list selection valid. Runs on every message arrival.
    fn refresh_status_contacts(&mut self) {
        self.status_contacts = self.derive_status_contacts();
        self.clamp_status_selection();
    }

    fn derive_status_contacts(&self) -> Vec<wr::JID> {
        let mut latest: HashMap<wr::JID, i64> = HashMap::new();
        for id in self
            .chat_messages
            .get(&wr::JID::from(STATUS_BROADCAST_CHAT.to_owned()))
            .into_iter()
            .flatten()
        {
            if let Some(message) = self.messages.get(id)
                && message.info.timestamp
                    > latest
                        .get(&message.info.sender)
                        .copied()
                        .unwrap_or(i64::MIN)
            {
                latest.insert(message.info.sender.clone(), message.info.timestamp);
            }
        }
        let mut senders = latest.into_iter().collect::<Vec<_>>();
        senders.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| left.0.0.as_ref().cmp(right.0.0.as_ref()))
        });
        senders.into_iter().map(|(jid, _)| jid).collect()
    }

    /// Keeps the status-list highlight valid: always selects a row when the
    /// list is non-empty and never selects past the end.
    pub(crate) fn clamp_status_selection(&mut self) {
        match (self.status_contacts.len(), self.status_selection.selected()) {
            (0, _) => self.status_selection.select(None),
            (_, None) => self.status_selection.select(Some(0)),
            (len, Some(selected)) if selected >= len => self.status_selection.select(Some(len - 1)),
            _ => {}
        }
    }

    pub fn selected_status_contact(&self) -> Option<wr::JID> {
        self.status_selection
            .selected()
            .map(|index| self.status_contacts[index].clone())
    }

    /// The statuses of `contact` from the `status@broadcast` chat in
    /// ascending order (newest last, as `sort_chat_messages` leaves it).
    pub fn status_messages(&self, contact: &wr::JID) -> Vec<wr::MessageId> {
        self.chat_messages
            .get(&wr::JID::from(STATUS_BROADCAST_CHAT.to_owned()))
            .into_iter()
            .flatten()
            .filter(|id| {
                self.messages
                    .get(*id)
                    .is_some_and(|message| &message.info.sender == contact)
            })
            .cloned()
            .collect()
    }

    pub fn status_latest_time(&self, contact: &wr::JID) -> Option<i64> {
        self.chat_messages
            .get(&wr::JID::from(STATUS_BROADCAST_CHAT.to_owned()))
            .into_iter()
            .flatten()
            .filter_map(|id| self.messages.get(id))
            .filter(|message| &message.info.sender == contact)
            .map(|message| message.info.timestamp)
            .max()
    }

    pub fn has_unseen_statuses(&self, contact: &wr::JID) -> bool {
        self.status_latest_time(contact).is_some_and(|latest| {
            latest
                > self
                    .status_last_seen
                    .get(contact)
                    .copied()
                    .unwrap_or_default()
        })
    }

    /// Marks the selected contact's statuses as viewed by recording the
    /// latest status timestamp, and resets the message-list scroll state.
    pub fn open_selected_status(&mut self) {
        let Some(contact) = self.selected_status_contact() else {
            return;
        };
        self.open_status_contact = Some(contact.clone());
        if let Some(latest) = self.status_latest_time(&contact) {
            self.status_last_seen.insert(contact, latest);
        }
        self.message_list_state.reset();
    }

    pub fn selected_message(&self) -> Option<&wr::Message> {
        self.message_list_state
            .get_selected_message()
            .and_then(|message_id| self.messages.get(&message_id))
    }

    pub fn message_status(&self, message_id: &wr::MessageId) -> MessageStatus {
        self.message_actions
            .get(message_id)
            .map(|actions| MessageStatus {
                edited: actions
                    .iter()
                    .any(|action| matches!(action.kind, MessageActionKind::Edit { .. })),
                deleted: actions
                    .iter()
                    .any(|action| matches!(action.kind, MessageActionKind::Delete)),
            })
            .unwrap_or_default()
    }

    fn sorted_message_actions(&self, message_id: &wr::MessageId) -> Vec<MessageAction> {
        let mut actions = self
            .message_actions
            .get(message_id)
            .cloned()
            .unwrap_or_default();
        actions.sort_by(|left, right| {
            (left.occurred_at, left.arrival_order, &left.action_id).cmp(&(
                right.occurred_at,
                right.arrival_order,
                &right.action_id,
            ))
        });
        actions
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

    fn pending_local_action_for(&self, action: &MessageAction) -> Option<Arc<str>> {
        if action.action_id.starts_with("local-")
            || !self
                .messages
                .get(&action.target_message_id)
                .is_some_and(|message| message.info.is_from_me)
        {
            return None;
        }
        let prefix = match &action.kind {
            MessageActionKind::Edit { .. } => "local-edit:",
            MessageActionKind::Delete => "local-delete:",
        };
        let matches = self
            .message_actions
            .get(&action.target_message_id)?
            .iter()
            .filter(|local| {
                local.action_id.starts_with(prefix)
                    && local.chat == action.chat
                    && local.kind == action.kind
            });
        let mut matches = matches.map(|local| local.action_id.clone());
        let local = matches.next()?;
        matches.next().is_none().then_some(local)
    }

    fn refresh_message_projection(&mut self, id: &wr::MessageId) -> Option<&'static str> {
        let Some(current) = self.messages.get(id).cloned() else {
            return None;
        };
        let mut projected = current;
        if self.message_status(id).deleted {
            if let wr::MessageContent::File(file) = &projected.message {
                remove_owned_media_files(&self.media_path, &[PathBuf::from(file.path.as_ref())]);
            }
            projected.message = wr::MessageContent::Text(DELETED_MESSAGE_TEXT.into());
            projected.info.quote_id = None;
            projected.info.forwarding = Default::default();
            self.metadata.remove(id);
            self.image_cache.remove(id);
        } else if let wr::MessageContent::Text(body) = &mut projected.message {
            // Delete and edit mark the status; the displayed body remains
            // the current effective text, re-projected for live edits.
            for action in self.sorted_message_actions(id) {
                if let MessageActionKind::Edit { replacement } = action.kind
                    && !replacement.is_empty()
                {
                    *body = replacement;
                }
            }
        }
        self.messages.insert(id.clone(), projected.clone());
        self.message_height_cache.invalidate(id);
        // Persist the current effective body when the message has ever been
        // acted on, so a restart shows current content.
        if self
            .message_actions
            .get(id)
            .is_some_and(|actions| !actions.is_empty())
        {
            self.db_handler.add_message(&projected);
        }
        Some("refreshed")
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

    pub fn follow_selected_reference(&mut self) -> bool {
        let Some(reference_id) = self
            .selected_message()
            .and_then(|message| message.info.quote_id.clone())
            .filter(|reference_id| self.messages.contains_key(reference_id))
        else {
            return false;
        };

        self.message_list_state.set_selected_message(reference_id);
        self.message_list_state.update_selected = true;
        true
    }

    pub fn message_menu_actions(&self) -> Option<Vec<MessageMenuAction>> {
        self.message_menu
            .as_ref()
            .map(|(actions, _)| actions.clone())
    }

    pub fn select_chat(&mut self, jid: Option<wr::JID>) {
        let target_list = if self.contact_search.input.is_empty() {
            &self.sorted_chats
        } else {
            &self.filtered_chats
        };

        if let Some(jid) = jid
            && let Some(index) = target_list.iter().position(|chat_jid| chat_jid == &jid)
        {
            self.chat_list_state.select(Some(index));
        } else if !self.sorted_chats.is_empty() {
            self.chat_list_state.select(Some(0));
        } else {
            self.chat_list_state.select(None);
        }
    }

    fn update_filtered_chats(&mut self) {
        let query = self.contact_search.input.to_lowercase();
        self.filtered_chats = self
            .sorted_chats
            .iter()
            .filter(|chat| {
                let name = self.contact_name(chat).to_lowercase();
                name.contains(&query)
            })
            .cloned()
            .collect();

        if !self.filtered_chats.is_empty() {
            self.chat_list_state.select(Some(0));
        } else {
            self.chat_list_state.select(None);
        }
    }

    pub fn add_message(&mut self, message: wr::Message) {
        let chat_jid = message.info.chat.clone();
        let is_open_chat = self.open_chat.as_ref() == Some(&chat_jid);
        self.add_message_without_sort(message);
        self.sort_chat_messages(chat_jid.clone());
        if is_open_chat {
            self.reanchor_message_selection(&chat_jid);
        }
    }

    /// Keeps the highlighted message stable when a new message shifts the
    /// rendered list: re-derives the selected index from the ID anchor
    /// (selected_message) instead of trusting the now-stale index. The index
    /// is set directly so the viewport does not jump (no update_selected).
    fn reanchor_message_selection(&mut self, chat_jid: &wr::JID) {
        let Some(anchor) = self.message_list_state.get_selected_message() else {
            return;
        };
        let Some(messages) = self.chat_messages.get(chat_jid) else {
            return;
        };
        if let Some(index) = messages
            .iter()
            .rev()
            .filter(|id| self.messages.contains_key(*id))
            .position(|id| id == &anchor)
        {
            self.message_list_state.selected = Some(index);
        }
    }

    fn add_message_without_sort(&mut self, message: wr::Message) {
        let chat_jid = message.info.chat.clone();
        self.add_or_update_chat(
            Chat {
                jid: chat_jid.clone(),
                last_message_time: Some(message.info.timestamp),
            },
            |chat| {
                if Some(message.info.timestamp) > chat.last_message_time {
                    chat.last_message_time = Some(message.info.timestamp);
                }
            },
        );

        let id = message.info.id.clone();
        let is_new = !self.messages.contains_key(&id);
        let should_replace = self
            .messages
            .get(&id)
            .is_none_or(|existing| existing.info.timestamp < message.info.timestamp);
        if should_replace {
            self.messages.insert(id.clone(), message);
        }
        if is_new {
            self.chat_messages
                .entry(chat_jid.clone())
                .or_default()
                .push(id.clone());
        }
        self.refresh_message_projection(&id);
        self.refresh_status_contacts();
    }

    fn add_or_update_chat<F: FnOnce(&mut Chat)>(&mut self, chat: Chat, callback: F) {
        if let Some(existing_chat) = self.chats.get_mut(&chat.jid) {
            callback(existing_chat);
            self.db_handler.add_chat(existing_chat);
        } else {
            self.db_handler.add_chat(&chat);
            self.chats.insert(chat.jid.clone(), chat);
        }
    }

    fn get_contacts(&mut self) {
        for (jid, name) in wr::get_contacts() {
            self.contacts.insert(jid.clone(), name.clone());
            self.db_handler.add_contact(&jid, name.as_ref());
        }
    }

    pub fn sort_chats(&mut self) {
        let mut entries: Vec<_> = self.chats.values().cloned().collect();
        entries.sort_by(|a, b| {
            let a_time = a.last_message_time.unwrap_or_default();
            let b_time = b.last_message_time.unwrap_or_default();
            b_time.cmp(&a_time)
        });

        self.sorted_chats = entries
            .iter()
            .map(|chat| chat.jid.clone())
            .filter(|jid: &wr::JID| !jid.0.as_ref().ends_with("@broadcast"))
            .collect();
    }

    pub(crate) fn sort_chat_messages(&mut self, chat_jid: wr::JID) {
        if let Some(messages) = self.chat_messages.get_mut(&chat_jid) {
            messages.sort_by_cached_key(|msg_id| {
                (
                    self.messages
                        .get(msg_id)
                        .map(|m| m.info.timestamp)
                        .unwrap_or(i64::MIN),
                    msg_id.clone(),
                )
            });
        }
    }
}

/// WhatsApp embeds a tiny thumbnail (a few hundred bytes, ~72px) with video
/// messages; anything at least this large is a real extracted frame.
const VIDEO_THUMBNAIL_MIN_BYTES: u64 = 4096;

/// True when the sidecar already contains a usable video frame (not the tiny
/// embedded WhatsApp thumbnail that renders as a narrow sliver).
fn has_decent_video_thumbnail(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.len() >= VIDEO_THUMBNAIL_MIN_BYTES)
        .unwrap_or(false)
}

/// Extracts a real frame from a video file with ffmpeg into the `.jpg`
/// sidecar. Used so inline video previews show the actual frame instead of
/// WhatsApp's tiny embedded thumbnail. Best effort: on any failure the caller
/// falls back to the existing placeholder rendering.
fn generate_video_thumbnail(video_path: &Path, sidecar_path: &Path) {
    let attempts: [&[&str]; 2] = [
        // Seek 1s in so we skip the common black first frame.
        &["-y", "-loglevel", "error", "-ss", "1"],
        // Fallback for very short videos that start past 1s.
        &["-y", "-loglevel", "error"],
    ];
    for args in attempts {
        let status = Command::new("ffmpeg")
            .args(args)
            .arg("-i")
            .arg(video_path)
            .args(["-frames:v", "1", "-q:v", "4"])
            .arg(sidecar_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match status {
            Ok(status) if status.success() && has_decent_video_thumbnail(sidecar_path) => return,
            Ok(_) => continue,
            Err(err) => {
                warn!("ffmpeg thumbnail extraction failed: {err}");
                return;
            }
        }
    }
    warn!(
        "ffmpeg could not extract a thumbnail for {}",
        video_path.display()
    );
}

/// Reads the duration (in whole seconds) of an audio file with lofty.
/// Best effort: returns `None` for unreadable files, unsupported formats, or
/// files whose properties could not be resolved (lofty reports `Duration::ZERO`).
///
/// `guess_file_type()` sniffs the content (first 36 bytes) because the
/// extension map in lofty lacks `.oga` — the extension WhatsApp uses for
/// Opus voice notes — so extension-only probing would reject them.
fn probe_audio_duration(path: &Path) -> Option<u64> {
    use lofty::file::AudioFile;
    let duration = lofty::probe::Probe::open(path)
        .ok()?
        .guess_file_type()
        .ok()?
        .read()
        .ok()?
        .properties()
        .duration();
    (!duration.is_zero()).then_some(duration.as_secs())
}

/// Overlays a play-button marker (translucent circle + white triangle) on a
/// video thumbnail, matching Discord's in-app video preview styling.
fn apply_video_play_marker(image: &mut image::DynamicImage) {
    let mut rgba = image.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let min_dimension = width.min(height);
    if min_dimension < 24 {
        return;
    }

    let cx = width as f32 / 2.0 - 0.5;
    let cy = height as f32 / 2.0 - 0.5;
    let radius = (min_dimension as f32 * 0.14).clamp(10.0, 56.0);
    let radius_sq = radius * radius;

    // Translucent black circle
    for (x, y, pixel) in rgba.enumerate_pixels_mut() {
        let dx = x as f32 - cx;
        let dy = y as f32 - cy;
        if dx * dx + dy * dy <= radius_sq {
            blend_pixel(pixel, image::Rgba([0, 0, 0, 135]));
        }
    }

    // White play triangle
    let left = cx - radius * 0.24;
    let right = cx + radius * 0.42;
    let top = cy - radius * 0.42;
    let bottom = cy + radius * 0.42;
    let tri_min_x = left.floor().max(0.0) as u32;
    let tri_max_x = right.ceil().min(width.saturating_sub(1) as f32) as u32;
    let tri_min_y = top.floor().max(0.0) as u32;
    let tri_max_y = bottom.ceil().min(height.saturating_sub(1) as f32) as u32;

    for y in tri_min_y..=tri_max_y {
        let vertical = if y as f32 <= cy {
            ((y as f32 - top) / (cy - top)).clamp(0.0, 1.0)
        } else {
            ((bottom - y as f32) / (bottom - cy)).clamp(0.0, 1.0)
        };
        let row_left = left;
        let row_right = left + (right - left) * vertical;
        for x in tri_min_x..=tri_max_x {
            let xf = x as f32;
            if xf >= row_left && xf <= row_right {
                blend_pixel(rgba.get_pixel_mut(x, y), image::Rgba([245, 247, 250, 230]));
            }
        }
    }

    *image = image::DynamicImage::ImageRgba8(rgba);
}

fn blend_pixel(pixel: &mut image::Rgba<u8>, overlay: image::Rgba<u8>) {
    let alpha = u16::from(overlay.0[3]);
    let inverse_alpha = 255u16.saturating_sub(alpha);
    for channel in 0..3 {
        pixel.0[channel] = ((u16::from(overlay.0[channel]) * alpha
            + u16::from(pixel.0[channel]) * inverse_alpha
            + 127)
            / 255) as u8;
    }
    pixel.0[3] = pixel.0[3].max(overlay.0[3]);
}

/// Removes media files for purged status broadcasts, including the video
/// thumbnail sidecar (`videos/<id>.jpg`) that ffmpeg generates next to the
/// video. Missing files are ignored.
pub fn remove_owned_media_files(media_path: &Path, relative_paths: &[PathBuf]) {
    let Ok(root) = MediaRoot::new(media_path) else {
        return;
    };
    for rel in relative_paths {
        for candidate in [rel.clone(), rel.with_extension("jpg")] {
            if let Ok(path) = root.media_file(&candidate) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

pub fn remove_status_media_files(media_path: &Path, relative_paths: &[PathBuf]) {
    remove_owned_media_files(media_path, relative_paths);
}

fn notification_eligibility(message: &wr::Message) -> bool {
    !message.info.is_from_me && !App::is_status_chat(&message.info.chat)
}

fn notification_is_muted(found: bool, muted_until: i64, now: i64) -> bool {
    found && muted_until > now
}

fn notification_projection(message: &wr::Message, summary: Arc<str>) -> NotificationProjection {
    let body = match &message.message {
        wr::MessageContent::Text(text) => text.to_string(),
        wr::MessageContent::File(file) => {
            if let Some(caption) = &file.caption {
                caption.to_string()
            } else {
                match file.kind {
                    wr::FileKind::Image => "Sent an image".to_string(),
                    wr::FileKind::Video => "Sent a video".to_string(),
                    wr::FileKind::Audio => "Sent an audio message".to_string(),
                    wr::FileKind::Document => "Sent a document".to_string(),
                    wr::FileKind::Sticker => "Sent a sticker".to_string(),
                }
            }
        }
    };
    NotificationProjection { summary, body }
}

fn now_or(fallback: i64, clock: &dyn Clock) -> i64 {
    clock.unix_seconds().unwrap_or(fallback)
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
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

    impl RecordingNotifier {
        fn notifications(&self) -> Vec<(String, String)> {
            self.notifications.lock().unwrap().clone()
        }
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

    #[derive(Clone)]
    struct FailingNotifier {
        attempts: Arc<Mutex<usize>>,
    }

    impl Notifier for FailingNotifier {
        fn show(&self, _notification: &NotificationProjection) -> Result<(), String> {
            *self.attempts.lock().unwrap() += 1;
            Err("notification failed".to_owned())
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
    fn add_message_orders_out_of_order_history_for_newest_first_consumers() {
        let mut app = TestApp::new();
        let chat = wr::JID::from("chat@example.test".to_owned());

        app.add_message(message(&chat, "newest", 30));
        app.add_message(message(&chat, "oldest", 10));
        app.add_message(message(&chat, "middle", 20));

        let ids = &app.chat_messages[&chat];
        assert_eq!(
            ids.iter().map(|id| id.as_ref()).collect::<Vec<_>>(),
            ["oldest", "middle", "newest"]
        );
        assert_eq!(ids.iter().next_back().map(AsRef::as_ref), Some("newest"));

        app.add_message(message(&chat, "middle", 40));
        let ids = &app.chat_messages[&chat];
        assert_eq!(ids.last().map(AsRef::as_ref), Some("middle"));
    }

    #[test]
    fn is_group_chat_detects_group_jids() {
        assert!(App::is_group_chat(&wr::JID::from("123@g.us".to_owned())));
        assert!(!App::is_group_chat(&wr::JID::from(
            "123@s.whatsapp.net".to_owned()
        )));
        assert!(!App::is_group_chat(&wr::JID::from(
            "status@broadcast".to_owned()
        )));
    }

    #[test]
    fn open_chat_by_jid_registers_recipient_in_chat_list() {
        let mut app = TestApp::new();
        let recipient = wr::JID::from("alice@s.whatsapp.net".to_owned());

        app.open_chat_by_jid(recipient.clone());

        assert_eq!(app.open_chat(), Some(recipient.clone()));
        assert!(app.chats.contains_key(&recipient));
        assert!(app.sorted_chats.contains(&recipient));
    }

    #[test]
    fn reply_privately_from_group_jumps_to_sender_direct_chat() {
        let mut app = TestApp::new();
        let group = wr::JID::from("123@g.us".to_owned());
        let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
        let group_msg = wr::Message {
            info: wr::MessageInfo {
                id: "g1".into(),
                chat: group.clone(),
                sender: alice.clone(),
                timestamp: 100,
                forwarding: Default::default(),
                is_from_me: false,
                quote_id: None,
                read_by: 0,
            },
            message: wr::MessageContent::Text("hello group".into()),
        };
        app.open_chat = Some(group.clone());
        app.add_message(group_msg);
        app.message_list_state.set_selected_message("g1".into());

        app.reply_privately();

        assert_eq!(app.open_chat(), Some(alice.clone()));
        assert!(app.chats.contains_key(&alice));
        assert_eq!(app.selected_section, Section::Chats);
        assert_eq!(app.focus_pane, FocusPane::Conversation);
        assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
        assert_eq!(
            app.composer.quote.as_ref().map(|m| m.info.id.as_ref()),
            Some("g1")
        );
        assert!(matches!(
            app.action_notice,
            Some(ActionNotice::ReplyPrivatelyNamed(_))
        ));
    }

    #[test]
    fn reply_privately_is_refused_outside_groups() {
        let mut app = TestApp::new();
        let dm = wr::JID::from("alice@s.whatsapp.net".to_owned());
        let msg = wr::Message {
            info: wr::MessageInfo {
                id: "m1".into(),
                chat: dm.clone(),
                sender: dm.clone(),
                timestamp: 100,
                forwarding: Default::default(),
                is_from_me: false,
                quote_id: None,
                read_by: 0,
            },
            message: wr::MessageContent::Text("hi".into()),
        };
        app.open_chat = Some(dm.clone());
        app.add_message(msg);
        app.message_list_state.set_selected_message("m1".into());

        app.reply_privately();

        assert_eq!(app.open_chat(), Some(dm.clone()));
        assert!(matches!(
            app.action_notice,
            Some(ActionNotice::Unavailable(_))
        ));
    }

    #[test]
    fn reply_privately_is_refused_for_own_group_message() {
        let mut app = TestApp::new();
        let group = wr::JID::from("123@g.us".to_owned());
        let me = wr::JID::from("me@s.whatsapp.net".to_owned());
        let own_msg = wr::Message {
            info: wr::MessageInfo {
                id: "g1".into(),
                chat: group.clone(),
                sender: me.clone(),
                timestamp: 100,
                forwarding: Default::default(),
                is_from_me: true,
                quote_id: None,
                read_by: 0,
            },
            message: wr::MessageContent::Text("my message".into()),
        };
        app.open_chat = Some(group.clone());
        app.add_message(own_msg);
        app.message_list_state.set_selected_message("g1".into());

        app.reply_privately();

        assert_eq!(app.open_chat(), Some(group.clone()));
        assert!(matches!(
            app.action_notice,
            Some(ActionNotice::Unavailable(_))
        ));
    }

    #[test]
    fn new_message_preserves_selected_message_by_id() {
        let mut app = TestApp::new();
        let chat = wr::JID::from("chat@example.test".to_owned());
        app.open_chat = Some(chat.clone());

        app.add_message(message(&chat, "oldest", 10));
        app.add_message(message(&chat, "middle", 20));
        app.add_message(message(&chat, "newest", 30));

        // Rendered order (render_messages reverses) is [newest, middle,
        // oldest]; select "middle" and anchor the selection by ID.
        app.message_list_state.select(Some(1));
        app.message_list_state.set_selected_message("middle".into());

        app.add_message(message(&chat, "newest-2", 40));

        // The new message shifts the list, but the highlight stays on
        // "middle" (now rendered index 2), not on the stale index.
        assert_eq!(app.message_list_state.selected, Some(2));
        assert_eq!(
            app.message_list_state.get_selected_message(),
            Some("middle".into())
        );
    }

    #[test]
    fn new_message_in_other_chat_leaves_selection_untouched() {
        let mut app = TestApp::new();
        let chat = wr::JID::from("chat@example.test".to_owned());
        let other = wr::JID::from("other@example.test".to_owned());
        app.open_chat = Some(chat.clone());

        app.add_message(message(&chat, "oldest", 10));
        app.add_message(message(&chat, "middle", 20));
        app.add_message(message(&chat, "newest", 30));

        app.message_list_state.select(Some(1));
        app.message_list_state.set_selected_message("middle".into());
        // Simulate the post-render state: index and ID anchor are consistent.
        app.message_list_state.selected = Some(1);

        app.add_message(message(&other, "other-msg", 40));

        assert_eq!(app.message_list_state.selected, Some(1));
        assert_eq!(
            app.message_list_state.get_selected_message(),
            Some("middle".into())
        );
    }

    #[test]
    fn probe_audio_duration_reads_wav_seconds() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tone.wav");

        // Minimal RIFF/WAVE header: 16-bit PCM, mono, 8000 Hz, 2 seconds of
        // silence. Byte lengths must match the declared format for lofty.
        let sample_rate: u32 = 8000;
        let seconds: u32 = 2;
        let data_len: u32 = sample_rate * 2 * seconds;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.resize(44 + data_len as usize, 0);
        std::fs::write(&path, &wav).unwrap();

        assert_eq!(probe_audio_duration(&path), Some(2));
        assert_eq!(
            probe_audio_duration(&directory.path().join("missing.ogg")),
            None
        );
    }

    /// Hand-crafts a single Ogg page with a spec-compliant CRC
    /// (CRC-32, polynomial 0x04c11db7, non-reflected, CRC field zeroed).
    ///
    /// Header layout per RFC 3533: "OggS"(4) + version(1) + header_type(1) +
    /// granule(8) + serial(4) + sequence(4) + checksum(4) + nsegments(1) +
    /// segment table. Note the sequence number is 32 bits.
    fn ogg_page(
        header_type: u8,
        granule: u64,
        serial: u32,
        sequence: u32,
        segments: &[u8],
        payload: &[u8],
    ) -> Vec<u8> {
        let mut page = Vec::new();
        page.extend_from_slice(b"OggS");
        page.push(0); // version
        page.push(header_type);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&serial.to_le_bytes());
        page.extend_from_slice(&sequence.to_le_bytes());
        page.extend_from_slice(&[0; 4]); // CRC placeholder, patched below
        page.push(segments.len() as u8);
        page.extend_from_slice(segments);
        page.extend_from_slice(payload);

        let mut crc: u32 = 0;
        for &byte in &page {
            crc ^= u32::from(byte) << 24;
            for _ in 0..8 {
                if crc & 0x8000_0000 != 0 {
                    crc = (crc << 1) ^ 0x04c1_1db7;
                } else {
                    crc <<= 1;
                }
            }
        }
        page[22..26].copy_from_slice(&crc.to_le_bytes());
        page
    }

    /// Regression: WhatsApp stores voice notes as `.oga` (Ogg Opus), but lofty's
    /// extension map only knows `opus`/`ogg` — so probing must sniff the content
    /// instead of trusting the extension. This builds a minimal 2-page Ogg Opus
    /// (OpusHead + OpusTags, granule = 3s at 48 kHz) named `.oga`.
    #[test]
    fn probe_audio_duration_reads_ogg_opus_with_oga_extension() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("voice.oga");
        let serial: u32 = 0x1234_5678;

        let mut opus_head = Vec::new();
        opus_head.extend_from_slice(b"OpusHead");
        opus_head.push(1); // version
        opus_head.push(1); // channels (mono)
        opus_head.extend_from_slice(&0u16.to_le_bytes()); // pre-skip
        opus_head.extend_from_slice(&48000u32.to_le_bytes()); // input sample rate
        opus_head.extend_from_slice(&0u16.to_le_bytes()); // output gain
        opus_head.push(0); // channel mapping family

        let vendor = b"wp-tui-test";
        let mut opus_tags = Vec::new();
        opus_tags.extend_from_slice(b"OpusTags");
        opus_tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        opus_tags.extend_from_slice(vendor);
        opus_tags.extend_from_slice(&0u32.to_le_bytes()); // no user comments

        let page1 = ogg_page(0x02, 0, serial, 0, &[opus_head.len() as u8], &opus_head); // BOS
        let page2 = ogg_page(
            0x04,
            48000 * 3,
            serial,
            1,
            &[opus_tags.len() as u8],
            &opus_tags,
        ); // EOS, 3s

        let mut oga = Vec::new();
        oga.extend_from_slice(&page1);
        oga.extend_from_slice(&page2);
        std::fs::write(&path, &oga).unwrap();

        assert_eq!(probe_audio_duration(&path), Some(3));
    }

    #[test]
    fn probe_real_whatsapp_audio_diagnostic() {
        let Some(path) = std::env::var("WPTUI_PROBE_PATH").ok() else {
            return; // local-only diagnostic, skipped unless the env var is set
        };
        let path = std::path::Path::new(&path);
        let result = probe_audio_duration(path);
        eprintln!("probe({}) = {result:?}", path.display());
        {
            let opened = lofty::probe::Probe::open(path)
                .map(|p| p.file_type())
                .map(|t| format!("{t:?}"))
                .unwrap_or_else(|e| format!("open error: {e}"));
            eprintln!("extension-guessed file type: {opened}");
            let guessed = lofty::probe::Probe::open(path)
                .ok()
                .and_then(|p| p.guess_file_type().ok())
                .map(|p| format!("{:?}", p.file_type()))
                .unwrap_or_else(|| "guess error".to_string());
            eprintln!("content-guessed file type: {guessed}");
        }
        assert!(
            result.is_some(),
            "expected lofty to read {}",
            path.display()
        );
    }

    #[test]
    fn evicted_loaded_preview_becomes_reloadable() {
        let mut app = TestApp::new();
        let chat = wr::JID::from("chat@example.test".to_owned());
        let mut preview = message(&chat, "preview", 1);
        preview.message = wr::MessageContent::File(wr::FileContent {
            kind: wr::FileKind::Image,
            path: "old.png".into(),
            ..Default::default()
        });
        app.messages.insert("preview".into(), preview);
        app.metadata
            .insert("preview".into(), Metadata::File(FileMeta::Loaded));

        app.mark_evicted_preview_reloadable(&Arc::from("old.png"));

        assert!(matches!(
            app.metadata.get(&wr::MessageId::from("preview")),
            Some(Metadata::File(FileMeta::Downloaded))
        ));
    }

    #[test]
    fn add_message_breaks_equal_timestamp_ties_by_message_id() {
        let mut app = TestApp::new();
        let chat = wr::JID::from("chat@example.test".to_owned());

        app.add_message(message(&chat, "message-c", 10));
        app.add_message(message(&chat, "message-a", 10));
        app.add_message(message(&chat, "message-b", 10));

        assert_eq!(
            app.chat_messages[&chat]
                .iter()
                .map(|id| id.as_ref())
                .collect::<Vec<_>>(),
            ["message-a", "message-b", "message-c"]
        );
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

    /// A status broadcast: the chat is always `status@broadcast` and the
    /// sender is the contact who posted the status.
    fn status_message(sender: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
        wr::Message {
            info: wr::MessageInfo {
                id: id.into(),
                chat: wr::JID::from(STATUS_BROADCAST_CHAT.to_owned()),
                forwarding: Default::default(),
                sender: sender.clone(),
                timestamp: timestamp,
                is_from_me: false,
                quote_id: None,
                read_by: 0,
            },
            message: wr::MessageContent::Text(id.into()),
        }
    }

    #[test]
    fn status_contacts_are_sorted_by_latest_status_newest_first() {
        let mut app = TestApp::new();
        let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
        let bob = wr::JID::from("bob@s.whatsapp.net".to_owned());

        app.add_message(status_message(&alice, "a-old", 100));
        app.add_message(status_message(&bob, "b-status", 200));
        app.add_message(status_message(&alice, "a-new", 300));

        assert_eq!(app.status_contacts, vec![alice.clone(), bob.clone()]);
        assert_eq!(app.status_latest_time(&alice), Some(300));
        assert_eq!(
            app.status_messages(&alice)
                .iter()
                .map(|id| id.as_ref())
                .collect::<Vec<_>>(),
            ["a-old", "a-new"]
        );
    }

    #[test]
    fn status_contacts_break_equal_recency_ties_by_jid() {
        let mut app = TestApp::new();
        let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
        let bob = wr::JID::from("bob@s.whatsapp.net".to_owned());

        app.add_message(status_message(&bob, "b-status", 100));
        app.add_message(status_message(&alice, "a-status", 100));

        assert_eq!(app.status_contacts, vec![alice.clone(), bob.clone()]);
    }

    #[test]
    fn status_selection_defaults_to_first_contact_and_clamps_when_refreshed() {
        let mut app = TestApp::new();
        let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
        let bob = wr::JID::from("bob@s.whatsapp.net".to_owned());
        app.add_message(status_message(&alice, "a-status", 200));
        app.add_message(status_message(&bob, "b-status", 100));

        assert_eq!(app.status_selection.selected(), Some(0));

        app.status_selection.select(Some(5));
        app.add_message(status_message(&alice, "a-new", 300));
        assert_eq!(app.status_selection.selected(), Some(1));
    }

    #[test]
    fn opening_a_status_marks_the_latest_status_as_seen() {
        let mut app = TestApp::new();
        let alice = wr::JID::from("alice@s.whatsapp.net".to_owned());
        app.add_message(status_message(&alice, "a-old", 100));
        app.add_message(status_message(&alice, "a-new", 200));

        assert!(app.has_unseen_statuses(&alice));
        app.open_selected_status();
        assert!(!app.has_unseen_statuses(&alice));

        app.add_message(status_message(&alice, "a-newer", 300));
        assert!(app.has_unseen_statuses(&alice));
        app.open_selected_status();
        assert!(!app.has_unseen_statuses(&alice));
    }

    #[test]
    fn should_notify_skips_status_broadcast_and_own_messages() {
        let app = TestApp::new();
        let chat = wr::JID::from("chat@example.test".to_owned());
        let broadcast = wr::JID::from(STATUS_BROADCAST_CHAT.to_owned());

        let incoming = message(&chat, "incoming", 1);
        assert!(app.should_notify(&incoming));

        let mut own = message(&chat, "own", 2);
        own.info.is_from_me = true;
        assert!(!app.should_notify(&own));

        let status = status_message(&broadcast, "status", 3);
        assert!(!app.should_notify(&status));
    }

    #[test]
    fn notification_projection_preserves_untrusted_message_text() {
        let sender = wr::JID::from("alice@s.whatsapp.net".to_owned());
        let message = message(&sender, "ignored", 1);
        let projection = notification_projection(&message, Arc::from("Alice"));

        assert_eq!(projection.summary.as_ref(), "Alice");
        assert_eq!(projection.body, "ignored");

        let message = wr::Message {
            message: wr::MessageContent::Text("こんにちは\n\"quoted\"\u{0007}".into()),
            ..message
        };
        let projection = notification_projection(&message, Arc::from("名前\n\"sender\""));
        assert_eq!(projection.summary.as_ref(), "名前\n\"sender\"");
        assert_eq!(projection.body, "こんにちは\n\"quoted\"\u{0007}");
    }

    #[test]
    fn production_message_handler_covers_notification_policy_and_continuation() {
        let chat = wr::JID::from("chat@g.us".to_owned());
        let notifier = RecordingNotifier::default();
        let mut app = app_with_ports(FixedClock::new(2_000), notifier.clone());

        assert!(
            app.process_message_with_lookup(message(&chat, "ordinary", 1), false, |_| {
                wr::ChatSettings {
                    found: false,
                    ..Default::default()
                }
            },)
        );
        assert_eq!(
            notifier.notifications(),
            vec![("chat@g.us".to_owned(), "ordinary".to_owned())]
        );
        assert!(app.messages.contains_key("ordinary"));

        let muted = RecordingNotifier::default();
        let mut app = app_with_ports(FixedClock::new(2_000), muted.clone());
        assert!(
            app.process_message_with_lookup(message(&chat, "muted", 2), false, |_| {
                wr::ChatSettings {
                    found: true,
                    muted_until: 2_001,
                    ..Default::default()
                }
            },)
        );
        assert!(muted.notifications().is_empty());
        assert!(app.messages.contains_key("muted"));

        for (id, message) in [
            ("own", {
                let mut message = message(&chat, "own", 3);
                message.info.is_from_me = true;
                message
            }),
            ("status", status_message(&chat, "status", 4)),
        ] {
            let notifier = RecordingNotifier::default();
            let lookup_calls = Arc::new(Mutex::new(0));
            let calls = lookup_calls.clone();
            let mut app = app_with_ports(FixedClock::new(2_000), notifier.clone());
            assert!(app.process_message_with_lookup(message, false, |_| {
                *calls.lock().unwrap() += 1;
                Default::default()
            }));
            assert_eq!(*lookup_calls.lock().unwrap(), 0, "{id} lookup");
            assert!(notifier.notifications().is_empty(), "{id} notification");
            assert!(app.messages.contains_key(id));
        }

        let attempts = Arc::new(Mutex::new(0));
        let failing = FailingNotifier {
            attempts: attempts.clone(),
        };
        let mut app = app_with_ports(FixedClock::new(2_000), failing);
        assert!(
            app.process_message_with_lookup(message(&chat, "failure", 5), false, |_| {
                Default::default()
            },)
        );
        assert_eq!(*attempts.lock().unwrap(), 1);
        assert!(app.messages.contains_key("failure"));

        let sync = RecordingNotifier::default();
        let mut app = app_with_ports(FixedClock::new(2_000), sync.clone());
        assert!(
            !app.process_message_with_lookup(message(&chat, "sync", 6), true, |_| panic!(
                "sync messages must not look up chat settings"
            ),)
        );
        assert!(sync.notifications().is_empty());
        assert!(app.messages.contains_key("sync"));
    }

    #[test]
    fn production_default_handler_keeps_message_processing_usable() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_data_dir(dir.path(), dir.path());
        app.db_handler.init();
        let chat = wr::JID::from("chat@g.us".to_owned());
        assert!(
            app.process_message_with_lookup(message(&chat, "default", 7), false, |_| {
                Default::default()
            },)
        );
        assert!(app.messages.contains_key("default"));
        app.db_handler.stop();
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
        app.add_message(status_message(&alice, "s1", 200));
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
