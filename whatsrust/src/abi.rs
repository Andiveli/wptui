use std::ffi::c_char;
use std::ffi::c_void;

use strum::FromRepr;

pub(crate) type CJID = *const c_char;

pub(super) type CLogCallback = extern "C" fn(*const c_char, u8, *mut c_void);
pub(super) type CQrCallback = extern "C" fn(*const c_char, *mut c_void);
pub(super) type CMessageCallback = extern "C" fn(*const CMessage, bool, *mut c_void);
pub(super) type COptimisticTextSentCallback = extern "C" fn(u64, *const CMessage, *mut c_void);
pub(super) type CEventCallback = extern "C" fn(*const CEvent, *mut c_void);
pub(super) type CPresenceCallback = extern "C" fn(CJID, bool, i64, *mut c_void);

#[repr(C)]
pub(super) struct CForwardResult {
    pub(super) succeeded: u32,
    pub(super) failed: u32,
    pub(super) failure: u8,
}

unsafe extern "C" {
    #[cfg(test)]
    pub(super) fn C_TestEmitPresenceEvent(from: *const c_char, unavailable: bool, last_seen: i64);
    #[cfg(test)]
    pub(super) fn C_TestEmitPresenceEventsConcurrently(from: *const c_char, count: u32);
    pub(super) fn C_NewClient(db_path: *const c_char);
    pub(super) fn C_Connect(qr_cb: CQrCallback, data: *mut c_void);
    pub(super) fn C_SendMessage(
        jid: CJID,
        message_type: u8,
        message_content: *const c_void,
        quote_id: *const c_char,
        quote_sender: CJID,
        quote_chat: CJID,
        quote_message_type: u8,
        quote_message_content: *const c_void,
    );
    pub(super) fn C_SendTextMessage(
        jid: CJID,
        message_content: *const c_void,
        quote_id: *const c_char,
        quote_sender: CJID,
        quote_chat: CJID,
        quote_message_type: u8,
        quote_message_content: *const c_void,
        local_send_id: u64,
    ) -> u8;
    pub(super) fn C_ForwardMessage(
        source_id: *const c_char,
        source_chat: CJID,
        source_sender: CJID,
        source_is_from_me: bool,
        destinations: *const *const c_char,
        destination_count: usize,
        forward_source: *const u8,
        forward_source_len: usize,
    ) -> CForwardResult;
    pub(super) fn C_GetContacts() -> CGetContactsResult;
    pub(super) fn C_FreeContacts(result: CGetContactsResult);
    pub(super) fn C_GetCommunities() -> CGetCommunitiesResult;
    pub(super) fn C_FreeCommunities(result: CGetCommunitiesResult);
    pub(super) fn C_GetProfilePicture(jid: CJID) -> CProfilePictureResult;
    pub(super) fn C_GetCommunityProfilePicture(jid: CJID) -> CProfilePictureResult;
    pub(super) fn C_FreeProfilePicture(result: CProfilePictureResult);
    pub(super) fn C_GetChatSettings(jid: CJID) -> CChatSettings;
    pub(super) fn C_GetGroupInfo(jid: CJID) -> CGroupInfoResult;
    pub(super) fn C_GetGroupParticipants(jid: CJID) -> CGroupParticipantsResult;
    pub(super) fn C_FreeGroupParticipants(result: CGroupParticipantsResult);
    pub(super) fn C_ResolveDmChatId(jid: CJID) -> *mut c_char;
    pub(super) fn C_FreeResolveDmChatId(value: *mut c_char);
    pub(super) fn C_Disconnect();
    pub(super) fn C_Logout() -> u8;
    pub(super) fn C_DrainRawPresenceDiagnostics() -> *mut c_char;
    pub(super) fn C_FreeRawPresenceDiagnostics(report: *mut c_char);
    pub(super) fn C_SubscribePresence(jid: CJID) -> u8;
    pub(super) fn C_PairPhone(phone: *const c_char) -> *const c_char;
    pub(super) fn C_FreePairPhoneResult(result: *const c_char);
    pub(super) fn C_DownloadFile(file_id: *const c_char, base_path: *const c_char) -> u8;
    pub(super) fn C_ReactToMessage(
        target_jid: CJID,
        destination_jid: CJID,
        sender_jid: CJID,
        message_id: *const c_char,
        reaction: *const c_char,
    ) -> u8;
    pub(super) fn C_EditMessage(
        chat_jid: CJID,
        message_id: *const c_char,
        replacement: *const c_char,
    ) -> u8;
    pub(super) fn C_RevokeMessage(
        chat_jid: CJID,
        sender_jid: CJID,
        message_id: *const c_char,
    ) -> u8;
    pub(super) fn C_MarkAsRead(msg_id: *const c_char, chat_jid: CJID, sender_jid: CJID) -> i32;
    pub(super) fn C_MarkChatReadSync(
        chat_jid: CJID,
        msg_id: *const c_char,
        timestamp: i64,
        from_me: bool,
        participant_jid: CJID,
    ) -> i32;
    pub(super) fn C_SetMessageHandler(message_cb: CMessageCallback, data: *mut c_void);
    pub(super) fn C_SetOptimisticTextSentHandler(
        callback: COptimisticTextSentCallback,
        data: *mut c_void,
    );
    pub(super) fn C_SetEventHandler(event_cb: CEventCallback, data: *mut c_void);
    pub(super) fn C_SetPresenceHandler(presence_cb: CPresenceCallback, data: *mut c_void);
    pub(super) fn C_SetLogHandler(log_fn: CLogCallback, data: *mut c_void);
}

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
