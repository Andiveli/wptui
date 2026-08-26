use std::{
    ffi::{CStr, CString},
    sync::Arc,
};

use strum::{EnumIter, FromRepr};

use crate::abi::{CContact, CJID, LogoutStatus, ReceiptKind};
use crate::callbacks::CallbackTranslator;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct JID(pub Arc<str>);

impl From<JID> for Arc<str> {
    fn from(jid: JID) -> Self {
        jid.0
    }
}

impl From<String> for JID {
    fn from(jid: String) -> Self {
        JID(jid.into())
    }
}

impl From<&CJID> for JID {
    fn from(cjid: &CJID) -> Self {
        JID(unsafe { CStr::from_ptr(*cjid) }.to_string_lossy().into())
    }
}

impl From<&JID> for CJID {
    fn from(jid: &JID) -> Self {
        CString::new(jid.0.as_ref()).unwrap().into_raw()
    }
}

impl CallbackTranslator<CJID> for JID {
    unsafe fn to_rust(from: CJID) -> Self {
        (&from).into()
    }
}

pub const VIEW_ONCE_UNAVAILABLE_DESCRIPTION: &str =
    "View-once media is unavailable here. View it on your phone.";

pub type MessageId = Arc<str>;

#[derive(Clone, Debug)]
pub struct MessageInfo {
    pub id: MessageId,
    pub chat: JID,
    pub sender: JID,
    pub mentions_self: bool,
    pub timestamp: i64,
    pub is_from_me: bool,
    pub quote_id: Option<Arc<str>>,
    pub read_by: u16,
    pub forwarding: ForwardingInfo,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ForwardingInfo {
    pub is_forwarded: bool,
    pub score: u32,
}

#[derive(Clone, Debug, Default, FromRepr)]
#[repr(u8)]
pub enum FileKind {
    #[default]
    Image = 0,
    Video = 1,
    Audio = 2,
    Document = 3,
    Sticker = 4,
}

pub(crate) fn file_kind_discriminant(kind: &FileKind) -> u8 {
    kind.clone() as u8
}

pub type FileId = Arc<str>;

#[derive(Clone, Debug, Default)]
pub struct FileContent {
    pub kind: FileKind,
    pub path: Arc<str>,
    pub file_id: FileId,
    pub caption: Option<Arc<str>>,
}

#[derive(Clone, Debug, EnumIter)]
pub enum MessageContent {
    Text(Arc<str>),
    File(FileContent),
    ViewOnceUnavailable,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub info: MessageInfo,
    pub message: MessageContent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessageActionKind {
    Edit { replacement: Arc<str> },
    Delete,
}

#[derive(Clone, Debug)]
pub enum Event {
    SyncProgress(u8),
    AppStateSyncComplete,
    Receipt {
        kind: ReceiptKind,
        chat: JID,
        message_ids: Vec<MessageId>,
    },
    Reaction {
        chat: JID,
        target_message_id: MessageId,
        participant: JID,
        text: Arc<str>,
        is_from_me: bool,
    },
    Connected,
    MessageAction {
        action_id: Arc<str>,
        target_message_id: MessageId,
        chat: JID,
        sender: JID,
        kind: MessageActionKind,
        occurred_at: i64,
        arrival_order: u64,
    },
    /// A chat that exists even though the history sync batch carried no
    /// messages for it. Lets the app populate the full chat list instead of
    /// only the subset that shipped messages.
    Chat {
        jid: JID,
        last_message_time: i64,
    },
    /// Terminal outcome of an asynchronous logout. The Go bridge runs the
    /// remove-companion-device IQ off the event loop so `logout()` no longer
    /// blocks the UI; this event drives the local cleanup on completion.
    LogoutResult(LogoutStatus),
    MarkChatAsRead {
        chat: JID,
        message_id: MessageId,
        read: bool,
        timestamp: i64,
        from_me: bool,
        participant: Option<JID>,
    },
}

#[derive(Clone, Debug)]
pub struct PresenceUpdate {
    pub from: JID,
    pub unavailable: bool,
    pub last_seen: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mention {
    pub jid: JID,
    pub numeric_user: Arc<str>,
}

#[derive(Clone, Debug)]
pub struct Contact {
    pub found: bool,
    pub first_name: Arc<str>,
    pub full_name: Arc<str>,
    pub push_name: Arc<str>,
    pub business_name: Arc<str>,
}

impl From<&CContact> for Contact {
    fn from(ccontact: &CContact) -> Self {
        let first_name = unsafe { CStr::from_ptr(ccontact.first_name) }
            .to_string_lossy()
            .into_owned()
            .into();
        let full_name = unsafe { CStr::from_ptr(ccontact.full_name) }
            .to_string_lossy()
            .into_owned()
            .into();
        let push_name = unsafe { CStr::from_ptr(ccontact.push_name) }
            .to_string_lossy()
            .into_owned()
            .into();
        let business_name = unsafe { CStr::from_ptr(ccontact.business_name) }
            .to_string_lossy()
            .into_owned()
            .into();

        Contact {
            found: ccontact.found,
            first_name,
            full_name,
            push_name,
            business_name,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChatSettings {
    pub found: bool,
    pub muted_until: i64,
    pub pinned: bool,
    pub archived: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupInfo {
    pub jid: JID,
    pub name: Arc<str>,
    pub is_announce: bool,
    pub is_admin: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupParticipant {
    pub jid: JID,
    pub phone_number: JID,
    pub name: Arc<str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GroupInfoError {
    NotGroup,
    ClientUnavailable,
    RequestFailed,
    InvalidBridgeResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommunityInfo {
    pub jid: JID,
    pub name: Arc<str>,
    pub parent_jid: Option<JID>,
    pub is_parent: bool,
    pub is_joined: bool,
    pub is_default_subgroup: bool,
    /// `None` is unknown, `Some(false)` is no, and `Some(true)` is yes.
    pub is_announce: Option<bool>,
    /// Unknown or values outside `u32` are mapped to `None` without truncation.
    pub participant_count: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommunitiesError {
    BridgeUnavailable,
}

pub struct DownloadFailed;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogoutError {
    NotLoggedIn,
    Failed,
    InvalidBridgeResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageActionFailed;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfilePicture {
    pub id: Arc<str>,
    pub picture_type: Arc<str>,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfilePictureAvailability {
    Available(ProfilePicture),
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfilePictureError {
    InvalidJid,
    ClientUnavailable,
    RequestCancelled,
    Metadata,
    EmptyUrl,
    Download,
    Oversized,
    InvalidImage,
    InvalidBridgeResult,
}
