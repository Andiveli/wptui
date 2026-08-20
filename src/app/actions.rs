use crate::input_key::Key;
use whatsrust as wr;

pub const COMMON_REACTIONS: [&str; 6] = ["👍", "❤️", "😂", "😮", "😢", "🙏"];
/// The only reaction WhatsApp allows on statuses.
pub const STATUS_REACTION: &str = "💚";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppAction {
    Quit,
    Logout,
    ToggleLogs,
    ToggleSectionRail,
    ToggleChatList,
    FocusPane(FocusPane),
    OpenContextualActions,
    ToggleShortcutPopup,
    PlannedLeaderAction(&'static str),
    FocusNext,
    FocusPrevious,
    SelectNext,
    SelectPrevious,
    JumpTop,
    JumpBottom,
    HalfPageDown,
    HalfPageUp,
    InsertMode,
    CopyMessage,
    ViewMessage,
    ReactMessage,
    ShareMessage,
    ReplyMessage,
    ReplyPrivately,
    DeleteMessage,
    EditMessage,
    OpenChat,
    OpenMessage,
    DownloadMessage,
    ViewerNext,
    ViewerPrevious,
    ViewerZoomIn,
    ViewerZoomOut,
    ViewerOpenExternal,
    CloseAttachmentViewer,
    CloseStatusPane,
    OpenMessageMenu,
    MenuNext,
    MenuPrevious,
    ConfirmMessageMenu,
    CancelMessageMenu,
    ReactionPrev,
    ReactionNext,
    ConfirmReaction,
    CancelReaction,
    SharePickerPrevious,
    SharePickerNext,
    ToggleShareRecipient,
    ConfirmShare,
    CancelShare,
    ShareSearchBackspace,
    ShareSearchCharacter(char),
    UrlPickerPrevious,
    UrlPickerNext,
    ConfirmUrlPicker,
    CancelUrlPicker,
    AttachFile,
    FilePickerPrevious,
    FilePickerNext,
    FilePickerParent,
    FilePickerDescend,
    FilePickerToggle,
    FilePickerConfirm,
    FilePickerEnterSearch,
    FilePickerEndSearch,
    FilePickerBackspace,
    FilePickerCharacter(char),
    CancelFilePicker,
    GoToReference,
    Composer(ComposerAction),
}

pub trait UrlOpener {
    fn open(&mut self, plan: &crate::url::UrlLaunchPlan) -> std::io::Result<()>;
}

pub struct SystemUrlOpener;

impl UrlOpener for SystemUrlOpener {
    fn open(&mut self, plan: &crate::url::UrlLaunchPlan) -> std::io::Result<()> {
        crate::url::execute_url_launch(plan)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionNotice {
    CopiedText(String),
    SenderDetails(String),
    ReactedUsers(Vec<String>),
    EditedMessage,
    DeletedMessage,
    Reacted,
    Forwarded {
        succeeded: usize,
        failed: usize,
        failure: wr::ForwardFailure,
    },
    ReplyPrivatelyNamed(String),
    Unavailable(String),
    Unauthorized(String),
    Unsupported(String),
    Cancelled,
}

pub trait ClipboardWriter {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardWriteError>;
    fn written_text(&self) -> Option<&str> {
        None
    }
}

pub trait ClipboardReader {
    fn read_paste(
        &mut self,
    ) -> Result<crate::clipboard::ClipboardPaste, crate::clipboard::ClipboardError>;
}

pub struct SystemClipboardReader(pub arboard::Clipboard);

impl ClipboardReader for SystemClipboardReader {
    fn read_paste(
        &mut self,
    ) -> Result<crate::clipboard::ClipboardPaste, crate::clipboard::ClipboardError> {
        crate::clipboard::read_paste(&mut self.0)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ClipboardWriteError;

pub struct SystemClipboardWriter(pub arboard::Clipboard);

impl ClipboardWriter for SystemClipboardWriter {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardWriteError> {
        self.0.set_text(text).map_err(|_| ClipboardWriteError)
    }
}

/// Clipboard that reports the system clipboard as unavailable. Used when no
/// display server is reachable (for example on a headless machine), so the
/// app keeps working and paste surfaces a clear error instead of panicking.
pub struct UnavailableClipboardReader;

impl ClipboardReader for UnavailableClipboardReader {
    fn read_paste(
        &mut self,
    ) -> Result<crate::clipboard::ClipboardPaste, crate::clipboard::ClipboardError> {
        Err(crate::clipboard::ClipboardError::ClipboardUnavailable)
    }
}

pub struct UnavailableClipboardWriter;

impl ClipboardWriter for UnavailableClipboardWriter {
    fn write_text(&mut self, _text: &str) -> Result<(), ClipboardWriteError> {
        Err(ClipboardWriteError)
    }
}

pub trait MessageEditor {
    fn edit_message(
        &self,
        chat: &wr::JID,
        message_id: &wr::MessageId,
        replacement: &str,
    ) -> Result<(), wr::MessageActionFailed>;
}

pub struct WhatsAppMessageEditor;

impl MessageEditor for WhatsAppMessageEditor {
    fn edit_message(
        &self,
        chat: &wr::JID,
        message_id: &wr::MessageId,
        replacement: &str,
    ) -> Result<(), wr::MessageActionFailed> {
        wr::edit_message(chat, message_id, replacement)
    }
}

pub trait MessageReactor {
    fn react_to_message(
        &self,
        chat: &wr::JID,
        sender: &wr::JID,
        message_id: &wr::MessageId,
        reaction: &str,
    ) -> Result<(), wr::MessageActionFailed>;

    fn react_to_message_in_chat(
        &self,
        target: &wr::JID,
        destination: &wr::JID,
        sender: &wr::JID,
        message_id: &wr::MessageId,
        reaction: &str,
    ) -> Result<(), wr::MessageActionFailed> {
        if target != destination {
            return Err(wr::MessageActionFailed);
        }
        self.react_to_message(target, sender, message_id, reaction)
    }
}

pub struct WhatsAppMessageReactor;

impl MessageReactor for WhatsAppMessageReactor {
    fn react_to_message(
        &self,
        chat: &wr::JID,
        sender: &wr::JID,
        message_id: &wr::MessageId,
        reaction: &str,
    ) -> Result<(), wr::MessageActionFailed> {
        wr::react_to_message(chat, sender, message_id, reaction)
    }

    fn react_to_message_in_chat(
        &self,
        target: &wr::JID,
        destination: &wr::JID,
        sender: &wr::JID,
        message_id: &wr::MessageId,
        reaction: &str,
    ) -> Result<(), wr::MessageActionFailed> {
        wr::react_to_message_in_chat(target, destination, sender, message_id, reaction)
    }
}

pub trait MessageForwarder {
    fn forward_message(&self, source: &wr::Message, destinations: &[wr::JID]) -> wr::ForwardReport;
}

pub struct WhatsAppMessageForwarder;

impl MessageForwarder for WhatsAppMessageForwarder {
    fn forward_message(&self, source: &wr::Message, destinations: &[wr::JID]) -> wr::ForwardReport {
        wr::forward_message(source, destinations)
    }
}

pub trait MessageRevoker {
    fn revoke_message(
        &self,
        chat: &wr::JID,
        sender: &wr::JID,
        message_id: &wr::MessageId,
    ) -> Result<(), wr::MessageActionFailed>;
}

pub struct WhatsAppMessageRevoker;

impl MessageRevoker for WhatsAppMessageRevoker {
    fn revoke_message(
        &self,
        chat: &wr::JID,
        sender: &wr::JID,
        message_id: &wr::MessageId,
    ) -> Result<(), wr::MessageActionFailed> {
        wr::revoke_message(chat, sender, message_id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageMenuAction {
    CopyText,
    Reply,
    GoToReference,
    SenderDetails,
    ReactedUsers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationMode {
    MessageNavigation,
    ComposerEditing,
    EditingMessage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposerAction {
    StartEdit,
    Submit,
    InsertNewline,
    Paste,
    Edit(Key),
    RemoveLastAttachment,
    CancelReply,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusPane {
    SectionRail,
    ChatList,
    Conversation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaneVisibility {
    pub section_rail: bool,
    pub chat_list: bool,
}

impl Default for PaneVisibility {
    fn default() -> Self {
        Self {
            section_rail: true,
            chat_list: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Section {
    #[default]
    Chats,
    Status,
    Communities,
}

impl Section {
    pub const ALL: [Self; 3] = [Self::Chats, Self::Status, Self::Communities];

    pub fn next(self) -> Self {
        match self {
            Self::Chats => Self::Status,
            Self::Status => Self::Communities,
            Self::Communities => Self::Chats,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Chats => Self::Communities,
            Self::Status => Self::Chats,
            Self::Communities => Self::Status,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Chats => "Chats",
            Self::Status => "Status",
            Self::Communities => "Communities",
        }
    }
}

/// The Logout entry pinned at the BOTTOM of the section rail, below the three
/// content sections. It is deliberately NOT a `Section` variant: content
/// selection (`selected_section`) drives ChatList/Conversation rendering and
/// the "… is not available yet" placeholder, and a 4th section variant would
/// ripple into all of that. Instead the rail cursor is modeled as
/// `App.selected_section` (indices 0..3 content sections) plus a
/// `App.rail_on_logout` flag meaning "the rail is on the Logout slot" (index 3).
/// While the flag is set the content pane shows a dedicated logout placeholder
/// and Enter triggers the existing `begin_logout_confirmation()`.
pub const LOGOUT_RAIL_TITLE: &str = "Logout";

impl FocusPane {
    pub fn next(self, visibility: PaneVisibility) -> Self {
        let panes = visible_panes(visibility);
        let index = panes.iter().position(|pane| *pane == self).unwrap_or(0);
        panes[(index + 1) % panes.len()]
    }

    pub fn previous(self, visibility: PaneVisibility) -> Self {
        let panes = visible_panes(visibility);
        let index = panes.iter().position(|pane| *pane == self).unwrap_or(0);
        panes[(index + panes.len() - 1) % panes.len()]
    }
}

fn visible_panes(visibility: PaneVisibility) -> Vec<FocusPane> {
    let mut panes = Vec::with_capacity(3);
    if visibility.section_rail {
        panes.push(FocusPane::SectionRail);
    }
    if visibility.chat_list {
        panes.push(FocusPane::ChatList);
    }
    panes.push(FocusPane::Conversation);
    panes
}

pub fn focus_after_visibility_change(focus: FocusPane, visibility: PaneVisibility) -> FocusPane {
    match focus {
        FocusPane::SectionRail if !visibility.section_rail => {
            if visibility.chat_list {
                FocusPane::ChatList
            } else {
                FocusPane::Conversation
            }
        }
        FocusPane::ChatList if !visibility.chat_list => {
            if visibility.section_rail {
                FocusPane::SectionRail
            } else {
                FocusPane::Conversation
            }
        }
        _ => focus,
    }
}

pub fn focus_after(focus: FocusPane, action: &AppAction, visibility: PaneVisibility) -> FocusPane {
    match action {
        AppAction::FocusNext => focus.next(visibility),
        AppAction::FocusPrevious => focus.previous(visibility),
        _ => focus,
    }
}
