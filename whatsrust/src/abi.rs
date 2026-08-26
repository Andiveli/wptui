use std::ffi::c_char;
use std::ffi::c_void;

use strum::FromRepr;

pub(crate) type CJID = *const c_char;

#[repr(C)]
pub(super) struct CMessageInfo {
    pub(super) id: *const c_char,
    pub(super) chat: CJID,
    pub(super) sender: CJID,
    pub(super) push_name: *const c_char,
    pub(super) mentions_self: bool,
    pub(super) timestamp: i64,
    pub(super) is_from_me: bool,
    pub(super) quote_id: *const c_char,
    pub(super) read_by: u16,
    pub(super) is_forwarded: bool,
    pub(super) forwarding_score: u32,
}

#[repr(C)]
pub(super) struct CMentionRange {
    pub(super) start: usize,
    pub(super) end: usize,
}

#[repr(C)]
pub(super) struct CIncomingTextMessage {
    pub(super) text: *const c_char,
    pub(super) mention_ranges: *const CMentionRange,
    pub(super) mention_range_count: usize,
}

#[repr(C)]
pub(super) struct CTextMessage {
    pub(super) text: *const c_char,
    pub(super) mentioned_jids: *const CJID,
    pub(super) mentioned_count: usize,
}

#[repr(C)]
pub(super) struct CFileMessage {
    pub(super) kind: u8,
    pub(super) path: *const c_char,
    pub(super) file_id: *const c_char,
    pub(super) caption: *const c_char,
    pub(super) mentioned_jids: *const CJID,
    pub(super) mentioned_count: usize,
    pub(super) mention_ranges: *const CMentionRange,
    pub(super) mention_range_count: usize,
}

#[repr(C)]
pub(super) struct CMessage {
    pub(super) info: CMessageInfo,
    pub(super) message_type: u8,
    pub(super) message: *const c_void,
    pub(super) forward_source: *const u8,
    pub(super) forward_source_len: usize,
}

#[repr(C)]
pub(super) struct CReceipt {
    pub(super) kind: u8,
    pub(super) chat: CJID,
    pub(super) message_ids: *const *const c_char,
    pub(super) count: u32,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub(super) struct CEvent {
    pub(super) event_type: u8,
    pub(super) data: *const c_void,
}

#[repr(C)]
pub(super) struct CReactionEvent {
    pub(super) chat: CJID,
    pub(super) target_message_id: *const c_char,
    pub(super) participant: CJID,
    pub(super) text: *const c_char,
    pub(super) is_from_me: bool,
}

#[repr(C)]
pub(super) struct CMessageActionEvent {
    pub(super) action_id: *const c_char,
    pub(super) chat: CJID,
    pub(super) sender: CJID,
    pub(super) target_message_id: *const c_char,
    pub(super) replacement: *const c_char,
    pub(super) occurred_at: i64,
    pub(super) arrival_order: u64,
    pub(super) kind: u8,
}

#[repr(C)]
pub(super) struct CChatEvent {
    pub(super) chat: CJID,
    pub(super) last_message_time: i64,
}

#[repr(C)]
pub(super) struct CMarkChatAsReadEvent {
    pub(super) chat: CJID,
    pub(super) message_id: *const c_char,
    pub(super) read: bool,
    pub(super) timestamp: i64,
    pub(super) from_me: bool,
    pub(super) participant: CJID,
}

#[repr(C)]
pub(super) struct CLogoutResultEvent {
    pub(super) status: u8,
}

#[derive(FromRepr)]
#[repr(u8)]
pub(super) enum MessageType {
    Text = 0,
    File = 1,
    ViewOnceUnavailable = 2,
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

pub(super) fn file_kind_discriminant(kind: &FileKind) -> u8 {
    kind.clone() as u8
}

#[derive(Clone, Debug, FromRepr)]
#[repr(u8)]
pub(super) enum EventType {
    SyncProgress = 0,
    AppStateSyncComplete = 1,
    Receipt = 2,
    Reaction = 3,
    // Event type 4 is reserved (removed multiplexed Presence event on Go side)
    Connected = 5,
    MessageAction = 6,
    Chat = 7,
    LogoutResult = 8,
    MarkChatAsRead = 9,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u8)]
pub enum ReceiptKind {
    Read = 0,
    ReadSelf = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u8)]
pub enum LogoutStatus {
    LoggedOut = 0,
    NotLoggedIn = 1,
    Failed = 2,
    /// Remote revocation failed, but the local sign-out succeeded. The device
    /// remains linked on the phone until removed manually.
    LocalOnly = 3,
}

#[repr(C)]
pub(crate) struct CContact {
    pub(crate) found: bool,
    pub(crate) first_name: *const c_char,
    pub(crate) full_name: *const c_char,
    pub(crate) push_name: *const c_char,
    pub(crate) business_name: *const c_char,
}

#[repr(C)]
pub(crate) struct CContactEntry {
    pub(crate) jid: CJID,
    pub(crate) name: *const c_char,
}

#[repr(C)]
pub(crate) struct CCommunityEntry {
    pub(crate) jid: CJID,
    pub(crate) name: *const c_char,
    pub(crate) parent_jid: CJID,
    pub(crate) is_parent: bool,
    pub(crate) is_joined: bool,
    pub(crate) is_default_subgroup: bool,
    // Stable C encoding: 0 unknown, 1 no, 2 yes.
    pub(crate) announcement: u8,
    // Stable C encoding: -1 unknown, otherwise a known signed count.
    pub(crate) participant_count: i64,
}

#[repr(C)]
pub(crate) struct CGetContactsResult {
    pub(crate) entries: *const CContactEntry,
    pub(crate) size: u32,
}

#[repr(C)]
pub(crate) struct CGetCommunitiesResult {
    pub(crate) entries: *const CCommunityEntry,
    pub(crate) size: u32,
    pub(crate) status: u8,
}

#[repr(C)]
pub(crate) struct CProfilePictureResult {
    pub(crate) status: u8,
    pub(crate) picture_id: *mut c_char,
    pub(crate) picture_type: *mut c_char,
    pub(crate) data: *mut u8,
    pub(crate) size: u32,
}

#[repr(C)]
pub(crate) struct CChatSettings {
    pub(crate) found: bool,
    pub(crate) muted_until: i64,
    pub(crate) pinned: bool,
    pub(crate) archived: bool,
}

#[repr(C)]
pub(crate) struct CGroupInfoResult {
    pub(crate) status: u8,
    pub(crate) is_announce: bool,
    pub(crate) is_admin: bool,
}

#[repr(C)]
pub(crate) struct CGroupParticipantEntry {
    pub(crate) jid: CJID,
    pub(crate) phone_number: CJID,
    pub(crate) name: *const c_char,
}

#[repr(C)]
pub(crate) struct CGroupParticipantsResult {
    pub(crate) entries: *const CGroupParticipantEntry,
    pub(crate) size: u32,
}
