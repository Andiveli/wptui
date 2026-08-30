use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    sync::Mutex,
};

pub mod action_dispatch;
pub mod actions;
pub mod attachment_viewer;
pub mod bootstrap;
pub mod chat_navigation;
pub mod chat_opening;
pub mod chat_ordering;
pub mod chat_projection;
pub mod chat_search_input;
pub mod chat_store;
pub mod community_bridge;
pub mod community_hierarchy;
pub mod composer;
pub mod composer_input_mapping;
pub mod composer_input_paste;
pub mod composer_integration;
pub mod contact_avatars;
pub mod contextual_actions;
pub mod contextual_activation;
pub mod contextual_routing;
pub mod download_worker;
pub mod events;
pub mod file_picker_input;
pub mod input_mapping;
pub mod input_reader;
pub mod input_router;
pub mod inputs;
pub mod leader_menu;
pub mod lifecycle_settings_dispatch;
pub mod log_toggle;
pub mod logout;
pub mod media_cache;
pub mod media_jobs;
pub mod media_support;
pub mod message_action_diagnostics;
pub mod message_actions;
pub mod message_ingestion;
pub mod message_interactions;
pub mod message_menu;
pub mod message_navigation;
pub mod message_opening;
pub mod navigation_conversation_dispatch;
pub mod notifications;
pub mod optimistic_text_send;
pub mod preferences;
pub mod presence;
pub mod presence_bridge;
pub mod private_reply;
pub mod reaction_picker;
pub mod read_receipts;
pub mod runtime_avatar_events;
pub mod runtime_callbacks;
pub mod runtime_diagnostics;
pub mod runtime_loop;
pub mod runtime_media_viewer_events;
pub mod runtime_read_receipt_events;
pub mod runtime_send_events;
pub mod runtime_startup;
pub mod runtime_updater_events;
pub mod share_picker;
pub mod share_picker_input;
pub mod status_actions;
pub mod status_input;
pub mod status_projection;
pub mod terminal_input_translation;
pub mod terminal_session;
#[cfg(test)]
pub(crate) mod test_support;
pub mod unread_messages;
pub mod whatsapp_events;

pub use crate::app;
use crate::app::actions::{
    ActionNotice, ClipboardReader, ClipboardWriter, ConversationMode, FocusPane, MessageEditor,
    MessageForwarder, MessageMenuAction, MessageReactor, MessageRevoker, PaneVisibility, Section,
    SystemClipboardReader, SystemClipboardWriter, SystemUrlOpener, UnavailableClipboardReader,
    UnavailableClipboardWriter, UrlOpener, WhatsAppMessageEditor, WhatsAppMessageForwarder,
    WhatsAppMessageReactor, WhatsAppMessageRevoker,
};
pub use crate::app::chat_projection::{ChatRow, ContactRow};
pub use crate::app::community_hierarchy::{CommunityNavigationRow, CommunityNode};
use crate::app::composer::Composer;
use crate::app::contact_avatars::ContactAvatars;
use crate::app::events::{AppEvent, AppInput, AttachmentViewerState, ViewerPreviewState};
use crate::app::input_reader::InputReader;
pub use crate::app::media_support::{remove_owned_media_files, remove_status_media_files};
use crate::app::message_action_diagnostics::MessageActionDiagnostics;
pub use crate::app::message_actions::{
    DELETED_MESSAGE_TEXT, MessageAction, MessageActionKind, MessageStatus,
};
pub use crate::app::notifications::{
    Clock, NotificationProjection, Notifier, NotifyRustNotifier, SystemClock, now_or, unix_now,
};
use crate::app::preferences::ComposerDirection;
use crate::app::presence::{PresenceDiagnostics, SelectedPresence};
use crate::app::read_receipts::Coordinator as ReadReceiptCoordinator;
use crate::app::runtime_diagnostics::{MessageListCounts, Phase, RuntimeDiagnostics};
pub use crate::app::share_picker::SharePicker;
pub use crate::app::status_projection::STATUS_BROADCAST_CHAT;
use crate::db;
use crate::file_picker::FilePickerState;
use crate::key_handler::KeybindHandler;
// use crate::key_handler;

use crate::ui::message_list::{MessageHeightCache, MessageListState, MessageSequenceCache};
use db::DatabaseHandler;
use ratatui::widgets::ListState;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use whatsrust as wr;

use crate::ui::text_input::TextInput;

pub const ADMIN_ONLY_GROUP_MESSAGE: &str = "Only group admins can send messages in this group.";

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
    pub community_detail: Option<wr::JID>,
    pub communities_unavailable: bool,
    communities_loaded: bool,

    /// Contacts with posted statuses, sorted by latest-status recency (newest
    /// first). Derived from the `status@broadcast` chat.
    pub status_contacts: Vec<wr::JID>,
    pub status_selection: ListState,
    /// Latest status timestamp the user has viewed per contact, restored from
    /// the status_read_cursors table at startup.
    pub status_last_seen: HashMap<wr::JID, i64>,

    pub history_sync_percent: Option<u8>,
    pub selected_presence: SelectedPresence,
    presence_diagnostics: PresenceDiagnostics,

    pub composer: Composer<'a>,
    pub(crate) composer_direction: ComposerDirection,
    pub(crate) composer_viewport_width: u16,
    pub(crate) preferences_path: PathBuf,
    pub message_list_state: MessageListState,
    pub timeline: unread_messages::Timeline,
    pub metadata: HashMap<wr::MessageId, Metadata>,
    pub image_cache: HashMap<Arc<str>, StatefulProtocol>,
    pub image_cache_order: VecDeque<Arc<str>>,
    /// Probed audio duration in seconds, keyed by file path. Populated lazily
    /// by a background thread once the file is on disk.
    pub audio_durations: HashMap<Arc<str>, u64>,
    pub message_height_cache: MessageHeightCache,
    pub(crate) message_sequence_cache: HashMap<wr::JID, MessageSequenceCache>,
    pub(crate) message_sequence_revisions: HashMap<wr::JID, u64>,
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
    pub update_notice: Option<String>,
    pub message_menu: Option<(Vec<MessageMenuAction>, usize)>,
    pub contextual_menu: Option<(
        Vec<crate::app::contextual_actions::ContextualMenuRow>,
        usize,
    )>,
    pub leader_menu: Option<(Vec<crate::app::leader_menu::LeaderMenuRow>, usize)>,
    pub shortcut_popup: bool,
    pub reaction_picker: Option<(Vec<String>, usize)>,
    pub share_picker: Option<SharePicker>,
    pub url_picker: Option<(Vec<String>, usize)>,
    pub file_picker: Option<FilePickerState>,
    pub url_opener: Box<dyn UrlOpener>,
    pub attachment_viewer: Option<AttachmentViewerState>,
    pub viewer_preview: Option<ViewerPreviewState>,
    pub viewer_zoom: u16,
    pub read_receipts: ReadReceiptCoordinator,
    pub read_receipt_worker: read_receipts::worker::Worker,
    pub optimistic_text_send_worker: optimistic_text_send::Worker,
    pub pending_outgoing_text: HashMap<u64, optimistic_text_send::TextSendRequest>,
    pub completed_text_send_ids: VecDeque<u64>,
    pub next_local_send_id: u64,

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
    input_reader: InputReader,
    pub(crate) runtime_diagnostics: RuntimeDiagnostics,
    pub(crate) chat_list_view: Option<chat_projection::ChatListViewModel>,
    pub(crate) chat_list_revision: u64,
    pub(crate) chat_list_mutation_depth: usize,
    pub(crate) chat_list_mutation_pending: bool,
}

impl Default for App<'_> {
    fn default() -> Self {
        bootstrap::default_app()
    }
}

impl App<'_> {
    /// Constructs the full app with explicit storage directories instead of
    /// the user's real data/cache dirs. `App::default()` keeps using the
    /// real directories; tests use this factory with a fresh tempdir so they
    /// never open or write the real user database
    /// (`~/.local/share/wptui/whatsapp.db`).
    pub fn with_data_dir(data_dir: &Path, cache_dir: &Path) -> Self {
        bootstrap::with_data_dir(data_dir, cache_dir)
    }

    pub fn with_data_dir_and_ports(
        data_dir: &Path,
        cache_dir: &Path,
        clock: Box<dyn Clock>,
        notifier: Box<dyn Notifier>,
    ) -> Self {
        bootstrap::with_data_dir_and_ports(data_dir, cache_dir, clock, notifier)
    }

    pub(crate) fn toggle_composer_direction(&mut self) {
        let next = self.composer_direction.toggle();
        if let Err(error) =
            crate::app::preferences::save_composer_direction(&self.preferences_path, next)
        {
            self.unavailable(&format!("Could not persist composer direction: {error}"));
            return;
        }
        self.composer_direction = next;
    }
}

impl App<'_> {
    pub fn enable_message_action_diagnostics(&mut self, enabled: bool) {
        self.message_action_diagnostics = MessageActionDiagnostics::new(enabled);
    }

    #[cfg(test)]
    pub(crate) fn write_message_action_diagnostics(
        &self,
        output: impl std::io::Write,
    ) -> std::io::Result<()> {
        self.message_action_diagnostics.write_report(output)
    }

    pub fn enable_presence_diagnostics(&mut self, enabled: bool) {
        self.presence_diagnostics = PresenceDiagnostics::new(enabled);
    }

    pub fn handle_whatsapp_event(&mut self, event: wr::Event) -> bool {
        crate::app::whatsapp_events::handle(self, event)
    }

    pub fn run(&mut self, phone: Option<String>) {
        runtime_startup::run(self, phone);
    }

    pub(crate) fn record_phase<T>(&mut self, phase: Phase, work: impl FnOnce(&mut Self) -> T) -> T {
        let started = self.runtime_diagnostics.phase_started();
        let result = work(self);
        if let Some(started) = started {
            self.runtime_diagnostics
                .record_phase_finished(phase, started);
        }
        result
    }

    pub(crate) fn message_list_phase_started(&mut self) -> Option<u64> {
        self.runtime_diagnostics.phase_started()
    }

    pub(crate) fn finish_message_list_phase(&mut self, phase: Phase, started: Option<u64>) {
        if let Some(started) = started {
            self.runtime_diagnostics
                .record_phase_finished(phase, started);
        }
    }

    pub(crate) fn record_message_list_counts(&mut self, counts: MessageListCounts) {
        self.runtime_diagnostics.record_message_list_counts(counts);
    }

    pub(crate) fn invalidate_message_sequence(&mut self, chat: &wr::JID) {
        *self
            .message_sequence_revisions
            .entry(chat.clone())
            .or_default() += 1;
        self.message_sequence_cache
            .entry(chat.clone())
            .or_default()
            .invalidate();
    }

    pub(crate) fn invalidate_message_sequences_containing(&mut self, id: &wr::MessageId) {
        let chats = self
            .message_sequence_cache
            .iter()
            .filter(|(_, cache)| {
                cache
                    .ids
                    .as_ref()
                    .is_some_and(|ids| ids.iter().any(|cached| cached == id))
            })
            .map(|(chat, _)| chat.clone())
            .collect::<Vec<_>>();
        for chat in chats {
            self.invalidate_message_sequence(&chat);
        }
    }

    /// Explicit invalidation hook for fixture code that mutates public stores directly.
    pub fn invalidate_message_sequence_for_test(&mut self, chat: &wr::JID) {
        self.invalidate_message_sequence(chat);
    }

    pub(crate) fn message_sequence_started(&mut self) -> Option<u64> {
        self.runtime_diagnostics.phase_started()
    }

    pub(crate) fn record_message_sequence_finished(&mut self, started: Option<u64>) {
        if let Some(started) = started {
            self.runtime_diagnostics
                .record_message_sequence_rebuild_finished(started);
        }
    }

    pub(crate) fn finalize_runtime_diagnostics(&mut self) {
        let _ = self.runtime_diagnostics.finalize();
    }

    pub(crate) fn now(&self) -> i64 {
        now_or(0, &*self.clock)
    }

    /// The status contact currently open in the Status section's right
    /// pane (set by pressing Enter on a contact), mirroring `open_chat`.
    pub fn open_status_contact(&self) -> Option<wr::JID> {
        self.open_status_contact.clone()
    }
}
