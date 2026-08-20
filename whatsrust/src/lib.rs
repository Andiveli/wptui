use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char, c_void},
    ops::Range,
    path::Path,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

#[macro_use]
mod callbacks;
use callbacks::CallbackTranslator;
use strum::{EnumIter, FromRepr};

type CJID = *const c_char;

static PRESENCE_CALLBACK_INGRESS: AtomicUsize = AtomicUsize::new(0);
static FORWARD_SOURCES: LazyLock<Mutex<HashMap<(Arc<str>, Arc<str>, Arc<str>), Vec<u8>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static MESSAGE_PUSH_NAMES: LazyLock<Mutex<HashMap<Arc<str>, Arc<str>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static MESSAGE_MENTION_RANGES: LazyLock<Mutex<HashMap<MessageId, (Arc<str>, Vec<Range<usize>>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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

#[repr(C)]
struct CContact {
    found: bool,
    first_name: *const c_char,
    full_name: *const c_char,
    push_name: *const c_char,
    business_name: *const c_char,
}

#[repr(C)]
struct CContactEntry {
    jid: CJID,
    name: *const c_char,
}

#[repr(C)]
struct CCommunityEntry {
    jid: CJID,
    name: *const c_char,
    parent_jid: CJID,
    is_parent: bool,
}

#[repr(C)]
struct CGetContactsResult {
    entries: *const CContactEntry,
    size: u32,
}

#[repr(C)]
struct CGetCommunitiesResult {
    entries: *const CCommunityEntry,
    size: u32,
    status: u8,
}

#[repr(C)]
struct CProfilePictureResult {
    status: u8,
    picture_id: *mut c_char,
    picture_type: *mut c_char,
    data: *mut u8,
    size: u32,
}

#[repr(C)]
struct CChatSettings {
    found: bool,
    muted_until: i64,
    pinned: bool,
    archived: bool,
}

#[repr(C)]
struct CGroupInfoResult {
    status: u8,
    is_announce: bool,
    is_admin: bool,
}

#[repr(C)]
struct CGroupParticipantEntry {
    jid: CJID,
    phone_number: CJID,
    name: *const c_char,
}

#[repr(C)]
struct CGroupParticipantsResult {
    entries: *const CGroupParticipantEntry,
    size: u32,
}

#[repr(C)]
struct CMessageInfo {
    id: *const c_char,
    chat: CJID,
    sender: CJID,
    push_name: *const c_char,
    mentions_self: bool,
    timestamp: i64,
    is_from_me: bool,
    quote_id: *const c_char,
    read_by: u16,
    is_forwarded: bool,
    forwarding_score: u32,
}

#[repr(C)]
struct CMentionRange {
    start: usize,
    end: usize,
}

#[repr(C)]
struct CIncomingTextMessage {
    text: *const c_char,
    mention_ranges: *const CMentionRange,
    mention_range_count: usize,
}

#[repr(C)]
struct CTextMessage {
    text: *const c_char,
    mentioned_jids: *const CJID,
    mentioned_count: usize,
}

#[repr(C)]
struct CFileMessage {
    kind: u8,
    path: *const c_char,
    file_id: *const c_char,
    caption: *const c_char,
    mentioned_jids: *const CJID,
    mentioned_count: usize,
    mention_ranges: *const CMentionRange,
    mention_range_count: usize,
}

#[repr(C)]
struct CMessage {
    info: CMessageInfo,
    message_type: u8,
    message: *const c_void,
    forward_source: *const u8,
    forward_source_len: usize,
}

#[repr(C)]
struct CReceipt {
    kind: u8,
    chat: CJID,
    message_ids: *const *const c_char,
    count: u32,
}

#[derive(Clone, Debug)]
#[repr(C)]
struct CEvent {
    event_type: u8,
    data: *const c_void,
}

#[repr(C)]
struct CReactionEvent {
    chat: CJID,
    target_message_id: *const c_char,
    participant: CJID,
    text: *const c_char,
    is_from_me: bool,
}

#[repr(C)]
struct CMessageActionEvent {
    action_id: *const c_char,
    chat: CJID,
    sender: CJID,
    target_message_id: *const c_char,
    replacement: *const c_char,
    occurred_at: i64,
    arrival_order: u64,
    kind: u8,
}

#[repr(C)]
struct CChatEvent {
    chat: CJID,
    last_message_time: i64,
}

#[repr(C)]
struct CLogoutResultEvent {
    status: u8,
}

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

#[cfg(test)]
mod message_push_name_tests {
    use super::{JID, MessageInfo, message_push_name, store_message_push_name};

    #[test]
    fn callback_message_push_name_is_available_to_sender_naming() {
        let info = MessageInfo {
            id: "push-name-message".into(),
            chat: JID::from("chat@example.test".to_owned()),
            sender: JID::from("sender@example.test".to_owned()),
            mentions_self: false,
            timestamp: 0,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        };
        store_message_push_name(&info.id, "WhatsApp Profile");
        assert_eq!(
            message_push_name(&info.id).as_deref(),
            Some("WhatsApp Profile")
        );
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ForwardingInfo {
    pub is_forwarded: bool,
    pub score: u32,
}

#[derive(FromRepr)]
#[repr(u8)]
enum MessageType {
    Text = 0,
    File = 1,
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

fn file_kind_discriminant(kind: &FileKind) -> u8 {
    kind.clone() as u8
}

#[cfg(test)]
mod file_kind_tests {
    use super::{FileKind, file_kind_discriminant};

    #[test]
    fn ffi_file_kind_discriminants_remain_stable_for_all_media_types() {
        let cases = [
            (FileKind::Image, 0),
            (FileKind::Video, 1),
            (FileKind::Audio, 2),
            (FileKind::Document, 3),
            (FileKind::Sticker, 4),
        ];

        for (kind, expected) in cases {
            assert_eq!(file_kind_discriminant(&kind), expected);
        }
    }
}

#[cfg(test)]
mod message_action_tests {
    use std::ffi::CString;

    use super::{
        CMessageActionEvent, CReactionEvent, Event, JID, MessageActionKind, edit_to_ffi,
        message_action_event_from_ffi, reaction_event_from_ffi, reaction_to_ffi, revoke_to_ffi,
    };

    #[test]
    fn reaction_mapping_preserves_every_ordinary_message_field() {
        let chat = JID::from("1234567890@s.whatsapp.net".to_owned());
        let sender = JID::from("0987654321@s.whatsapp.net".to_owned());
        let message_id = "message".into();
        let (target, destination, sender, id, reaction) =
            reaction_to_ffi(&chat, &chat, &sender, &message_id, "👍").unwrap();
        assert_eq!(target.to_str().unwrap(), "1234567890@s.whatsapp.net");
        assert_eq!(destination.to_str().unwrap(), "1234567890@s.whatsapp.net");
        assert_eq!(chat.0.as_ref(), "1234567890@s.whatsapp.net");
        assert_eq!(sender.to_str().unwrap(), "0987654321@s.whatsapp.net");
        assert_eq!(id.to_str().unwrap(), "message");
        assert_eq!(reaction.to_str().unwrap(), "👍");
    }

    #[test]
    fn reaction_mapping_rejects_nul_before_the_ffi_boundary() {
        let jid = JID::from("1234567890@s.whatsapp.net".to_owned());
        let invalid_id = "message\0id".into();
        let message_id = "message".into();

        assert!(reaction_to_ffi(&jid, &jid, &jid, &invalid_id, "👍").is_err());
        assert!(reaction_to_ffi(&jid, &jid, &jid, &message_id, "👍\0").is_err());
    }

    #[test]
    fn reaction_event_copies_reactor_and_empty_removal_payload() {
        let chat = CString::new("group@g.us").unwrap();
        let target = CString::new("target").unwrap();
        let reactor = CString::new("reactor@s.whatsapp.net").unwrap();
        for (text, expected) in [("👍", "👍"), ("", "")] {
            let text = CString::new(text).unwrap();
            let event = CReactionEvent {
                chat: chat.as_ptr(),
                target_message_id: target.as_ptr(),
                participant: reactor.as_ptr(),
                text: text.as_ptr(),
                is_from_me: false,
            };
            let Event::Reaction {
                chat,
                target_message_id,
                participant,
                text,
                is_from_me,
            } = (unsafe { reaction_event_from_ffi(&event) })
            else {
                panic!("expected reaction event");
            };
            assert_eq!(chat.0.as_ref(), "group@g.us");
            assert_eq!(target_message_id.as_ref(), "target");
            assert_eq!(participant.0.as_ref(), "reactor@s.whatsapp.net");
            assert_eq!(text.as_ref(), expected);
            assert!(!is_from_me);
        }
    }

    #[test]
    fn edit_mapping_preserves_fields_and_rejects_invalid_replacements() {
        let input_chat = JID::from("1234567890@s.whatsapp.net".to_owned());
        let message_id = "message".into();
        let (chat, id, replacement) = edit_to_ffi(&input_chat, &message_id, "replacement").unwrap();

        assert_eq!(chat.to_str().unwrap(), "1234567890@s.whatsapp.net");
        assert_eq!(id.to_str().unwrap(), "message");
        assert_eq!(replacement.to_str().unwrap(), "replacement");
        assert!(edit_to_ffi(&input_chat, &message_id, "").is_err());
        assert!(edit_to_ffi(&input_chat, &message_id, " \t").is_err());
        assert!(edit_to_ffi(&input_chat, &"message\0id".into(), "replacement").is_err());
    }

    #[test]
    fn revoke_mapping_preserves_every_field() {
        let chat = JID::from("1234567890@s.whatsapp.net".to_owned());
        let sender = JID::from("0987654321@s.whatsapp.net".to_owned());
        let message_id: super::MessageId = "message".into();
        let (cchat, csender, cid) = revoke_to_ffi(&chat, &sender, &message_id).unwrap();

        assert_eq!(cchat.to_str().unwrap(), "1234567890@s.whatsapp.net");
        assert_eq!(csender.to_str().unwrap(), "0987654321@s.whatsapp.net");
        assert_eq!(cid.to_str().unwrap(), "message");
    }

    #[test]
    fn revoke_mapping_rejects_nul_before_the_ffi_boundary() {
        let jid = JID::from("1234567890@s.whatsapp.net".to_owned());
        let invalid_id: super::MessageId = "message\0id".into();
        let valid_id: super::MessageId = "message".into();

        assert!(revoke_to_ffi(&jid, &jid, &invalid_id).is_err());
        assert!(revoke_to_ffi(&jid, &jid, &valid_id).is_ok());
    }

    #[test]
    fn message_action_event_copies_ffi_buffers_before_go_releases_them() {
        let action_id = CString::new("edit-1").unwrap();
        let chat = CString::new("chat@example.test").unwrap();
        let sender = CString::new("sender@example.test").unwrap();
        let target = CString::new("target").unwrap();
        let replacement = CString::new("new body").unwrap();
        let event = CMessageActionEvent {
            action_id: action_id.as_ptr(),
            chat: chat.as_ptr(),
            sender: sender.as_ptr(),
            target_message_id: target.as_ptr(),
            replacement: replacement.as_ptr(),
            occurred_at: 42,
            arrival_order: 7,
            kind: 0,
        };

        let Event::MessageAction {
            action_id,
            target_message_id,
            chat,
            sender,
            kind,
            occurred_at,
            arrival_order,
        } = (unsafe { message_action_event_from_ffi(&event) })
        else {
            panic!("expected message action");
        };
        assert_eq!(action_id.as_ref(), "edit-1");
        assert_eq!(target_message_id.as_ref(), "target");
        assert_eq!(chat.0.as_ref(), "chat@example.test");
        assert_eq!(sender.0.as_ref(), "sender@example.test");
        assert_eq!(
            kind,
            MessageActionKind::Edit {
                replacement: "new body".into()
            }
        );
        assert_eq!((occurred_at, arrival_order), (42, 7));
    }
}

#[cfg(test)]
mod presence_event_tests {
    use std::{
        ffi::CString,
        sync::{Mutex, mpsc},
    };

    use super::{
        C_TestEmitPresenceEvent, C_TestEmitPresenceEventsConcurrently, PresenceUpdate,
        set_presence_handler,
    };

    static PRESENCE_HANDLER_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn go_trampoline_reaches_dedicated_rust_presence_handler() {
        let _guard = PRESENCE_HANDLER_TEST_LOCK.lock().unwrap();
        let (sender, receiver) = mpsc::channel();
        set_presence_handler(move |update| sender.send(update).unwrap());
        let from = CString::new("alice@s.whatsapp.net").unwrap();

        unsafe { C_TestEmitPresenceEvent(from.as_ptr(), true, 42) };

        let PresenceUpdate {
            from,
            unavailable,
            last_seen,
        } = receiver.recv().unwrap();
        assert_eq!(from.0.as_ref(), "alice@s.whatsapp.net");
        assert!(unavailable);
        assert_eq!(last_seen, 42);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn concurrent_go_goroutines_reach_dedicated_rust_presence_handler() {
        let _guard = PRESENCE_HANDLER_TEST_LOCK.lock().unwrap();
        const COUNT: u32 = 64;
        let (sender, receiver) = mpsc::channel();
        set_presence_handler(move |update| sender.send(update).unwrap());
        let from = CString::new("alice@s.whatsapp.net").unwrap();

        unsafe { C_TestEmitPresenceEventsConcurrently(from.as_ptr(), COUNT) };

        let mut last_seen = (0..COUNT as i64).collect::<Vec<_>>();
        let mut received = (0..COUNT)
            .map(|_| receiver.recv().unwrap())
            .collect::<Vec<_>>();
        received.sort_by_key(|update| update.last_seen);
        for (update, expected) in received.iter().zip(last_seen.drain(..)) {
            assert_eq!(update.from.0.as_ref(), "alice@s.whatsapp.net");
            assert!(!update.unavailable);
            assert_eq!(update.last_seen, expected);
        }
    }
}

#[derive(Clone, Debug, FromRepr)]
#[repr(u8)]
enum EventType {
    SyncProgress = 0,
    AppStateSyncComplete = 1,
    Receipt = 2,
    Reaction = 3,
    // Event type 4 is reserved (removed multiplexed Presence event on Go side)
    Connected = 5,
    MessageAction = 6,
    Chat = 7,
    LogoutResult = 8,
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
        kind: u8,
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
}

#[derive(Clone, Debug)]
pub struct PresenceUpdate {
    pub from: JID,
    pub unavailable: bool,
    pub last_seen: i64,
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
}

#[derive(Clone, Debug)]
pub struct Message {
    pub info: MessageInfo,
    pub message: MessageContent,
}

fn validated_mention_ranges(text: &str, ranges: &[CMentionRange]) -> Vec<Range<usize>> {
    let mut result = ranges
        .iter()
        .filter_map(|range| {
            (range.start < range.end
                && range.end <= text.len()
                && text.is_char_boundary(range.start)
                && text.is_char_boundary(range.end))
            .then_some(range.start..range.end)
        })
        .collect::<Vec<_>>();
    result.sort_by_key(|range| (range.start, range.end));
    result.dedup();
    result
}

pub fn store_message_mention_ranges(id: &MessageId, text: &str, ranges: Vec<Range<usize>>) {
    let mut stored = MESSAGE_MENTION_RANGES.lock().unwrap();
    if ranges.is_empty() {
        stored.remove(id);
    } else {
        stored.insert(id.clone(), (text.into(), ranges));
    }
}

pub fn message_mention_ranges(id: &MessageId, text: &str) -> Vec<Range<usize>> {
    MESSAGE_MENTION_RANGES
        .lock()
        .unwrap()
        .get(id)
        .filter(|(stored_text, _)| stored_text.as_ref() == text)
        .map(|(_, ranges)| ranges.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod mention_range_tests {
    use super::{CMentionRange, validated_mention_ranges};

    #[test]
    fn ffi_ranges_are_utf8_validated_and_invalid_ranges_are_dropped() {
        let text = "café @阿丽";
        let ranges = validated_mention_ranges(
            text,
            &[
                CMentionRange { start: 6, end: 13 },
                CMentionRange { start: 8, end: 13 },
                CMentionRange { start: 0, end: 99 },
            ],
        );
        assert_eq!(ranges, vec![6..13]);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ForwardReport {
    pub succeeded: usize,
    pub failed: usize,
    pub failure: ForwardFailure,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, FromRepr)]
#[repr(u8)]
pub enum ForwardFailure {
    #[default]
    None = 0,
    SourceUnavailable = 1,
    InvalidSource = 2,
    InvalidDestination = 3,
    SendFailed = 4,
}

impl ForwardReport {
    pub fn with_reason(succeeded: usize, failed: usize, failure: ForwardFailure) -> Self {
        Self {
            succeeded,
            failed,
            failure,
        }
    }
}

pub fn store_message_push_name(id: &MessageId, push_name: &str) {
    if push_name.trim().is_empty() {
        return;
    }
    MESSAGE_PUSH_NAMES
        .lock()
        .unwrap()
        .insert(id.clone(), push_name.trim().into());
}

pub fn message_push_name(id: &MessageId) -> Option<Arc<str>> {
    MESSAGE_PUSH_NAMES.lock().unwrap().get(id).cloned()
}

pub fn store_forward_source(info: &MessageInfo, source: Vec<u8>) {
    if !source.is_empty() {
        FORWARD_SOURCES.lock().unwrap().insert(
            (info.chat.0.clone(), info.sender.0.clone(), info.id.clone()),
            source,
        );
    }
}

pub fn forward_source(info: &MessageInfo) -> Option<Vec<u8>> {
    FORWARD_SOURCES
        .lock()
        .unwrap()
        .get(&(info.chat.0.clone(), info.sender.0.clone(), info.id.clone()))
        .cloned()
}

pub fn remove_forward_source(chat: &JID, id: &MessageId) {
    FORWARD_SOURCES
        .lock()
        .unwrap()
        .retain(|(source_chat, _, source_id), _| {
            source_chat.as_ref() != chat.0.as_ref() || source_id.as_ref() != id.as_ref()
        });
}

#[derive(Clone, Debug)]
pub struct Contact {
    pub found: bool,
    pub first_name: Arc<str>,
    pub full_name: Arc<str>,
    pub push_name: Arc<str>,
    pub business_name: Arc<str>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mention {
    pub jid: JID,
    pub numeric_user: Arc<str>,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommunitiesError {
    BridgeUnavailable,
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

type CLogCallback = extern "C" fn(*const c_char, u8, *mut c_void);
type CQrCallback = extern "C" fn(*const c_char, *mut c_void);
type CMessageCallback = extern "C" fn(*const CMessage, bool, *mut c_void);
type CEventCallback = extern "C" fn(*const CEvent, *mut c_void);
type CPresenceCallback = extern "C" fn(CJID, bool, i64, *mut c_void);

#[repr(C)]
struct CForwardResult {
    succeeded: u32,
    failed: u32,
    failure: u8,
}

unsafe extern "C" {
    #[cfg(test)]
    fn C_TestEmitPresenceEvent(from: *const c_char, unavailable: bool, last_seen: i64);
    #[cfg(test)]
    fn C_TestEmitPresenceEventsConcurrently(from: *const c_char, count: u32);
    fn C_NewClient(db_path: *const c_char);
    fn C_Connect(qr_cb: CQrCallback, data: *mut c_void);
    fn C_SendMessage(
        jid: CJID,
        message_type: u8,
        message_content: *const c_void,
        quote_id: *const c_char,
        quote_sender: CJID,
        quote_chat: CJID,
        quote_message_type: u8,
        quote_message_content: *const c_void,
    );
    fn C_ForwardMessage(
        source_id: *const c_char,
        source_chat: CJID,
        source_sender: CJID,
        source_is_from_me: bool,
        destinations: *const *const c_char,
        destination_count: usize,
        forward_source: *const u8,
        forward_source_len: usize,
    ) -> CForwardResult;
    fn C_GetContacts() -> CGetContactsResult;
    fn C_GetCommunities() -> CGetCommunitiesResult;
    fn C_FreeCommunities(result: CGetCommunitiesResult);
    fn C_GetProfilePicture(jid: CJID) -> CProfilePictureResult;
    fn C_FreeProfilePicture(result: CProfilePictureResult);
    fn C_GetChatSettings(jid: CJID) -> CChatSettings;
    fn C_GetGroupInfo(jid: CJID) -> CGroupInfoResult;
    fn C_GetGroupParticipants(jid: CJID) -> CGroupParticipantsResult;
    fn C_FreeGroupParticipants(result: CGroupParticipantsResult);
    fn C_ResolveDmChatId(jid: CJID) -> *mut c_char;
    fn C_FreeResolveDmChatId(value: *mut c_char);
    fn C_Disconnect();
    fn C_Logout() -> u8;
    fn C_DrainRawPresenceDiagnostics() -> *mut c_char;
    fn C_FreeRawPresenceDiagnostics(report: *mut c_char);
    fn C_SubscribePresence(jid: CJID) -> u8;
    fn C_PairPhone(phone: *const c_char) -> *const c_char;
    fn C_DownloadFile(file_id: *const c_char, base_path: *const c_char) -> u8;
    fn C_ReactToMessage(
        target_jid: CJID,
        destination_jid: CJID,
        sender_jid: CJID,
        message_id: *const c_char,
        reaction: *const c_char,
    ) -> u8;
    fn C_EditMessage(chat_jid: CJID, message_id: *const c_char, replacement: *const c_char) -> u8;
    fn C_RevokeMessage(chat_jid: CJID, sender_jid: CJID, message_id: *const c_char) -> u8;

    fn C_MarkAsRead(msg_id: *const c_char, chat_jid: CJID, sender_jid: CJID) -> i32;

    fn C_SetMessageHandler(message_cb: CMessageCallback, data: *mut c_void);
    fn C_SetEventHandler(event_cb: CEventCallback, data: *mut c_void);
    fn C_SetPresenceHandler(presence_cb: CPresenceCallback, data: *mut c_void);
    fn C_SetLogHandler(log_fn: CLogCallback, data: *mut c_void);
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

fn profile_picture_from_parts(
    status: u8,
    picture_id: &str,
    picture_type: &str,
    bytes: Vec<u8>,
) -> Result<ProfilePictureAvailability, ProfilePictureError> {
    match status {
        0 if !bytes.is_empty() => Ok(ProfilePictureAvailability::Available(ProfilePicture {
            id: picture_id.into(),
            picture_type: picture_type.into(),
            bytes,
        })),
        0 => Err(ProfilePictureError::InvalidBridgeResult),
        1 => Ok(ProfilePictureAvailability::Unavailable),
        2 => Err(ProfilePictureError::InvalidJid),
        3 => Err(ProfilePictureError::ClientUnavailable),
        4 => Err(ProfilePictureError::RequestCancelled),
        5 => Err(ProfilePictureError::Metadata),
        6 => Err(ProfilePictureError::EmptyUrl),
        7 => Err(ProfilePictureError::Download),
        8 => Err(ProfilePictureError::Oversized),
        9 => Err(ProfilePictureError::InvalidImage),
        _ => Err(ProfilePictureError::InvalidBridgeResult),
    }
}

pub fn get_profile_picture(jid: &JID) -> Result<ProfilePictureAvailability, ProfilePictureError> {
    let jid = CString::new(jid.0.as_ref()).map_err(|_| ProfilePictureError::InvalidJid)?;
    let result = unsafe { C_GetProfilePicture(jid.as_ptr()) };
    let picture_id = if result.picture_id.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(result.picture_id) }
            .to_string_lossy()
            .into_owned()
    };
    let picture_type = if result.picture_type.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(result.picture_type) }
            .to_string_lossy()
            .into_owned()
    };
    let bytes = if result.data.is_null() || result.size == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(result.data, result.size as usize) }.to_vec()
    };
    let converted = profile_picture_from_parts(result.status, &picture_id, &picture_type, bytes);
    unsafe { C_FreeProfilePicture(result) };
    converted
}

#[cfg(test)]
mod profile_picture_tests {
    use super::{ProfilePictureAvailability, ProfilePictureError, profile_picture_from_parts};

    #[test]
    fn maps_available_payload_without_exposing_the_temporary_url() {
        let result = profile_picture_from_parts(0, "picture-42", "preview", vec![1, 2, 3]);

        let ProfilePictureAvailability::Available(picture) = result.unwrap() else {
            panic!("expected available profile picture");
        };
        assert_eq!(picture.id.as_ref(), "picture-42");
        assert_eq!(picture.picture_type.as_ref(), "preview");
        assert_eq!(picture.bytes, vec![1, 2, 3]);
    }

    #[test]
    fn maps_unavailable_separately_from_recoverable_failures() {
        assert_eq!(
            profile_picture_from_parts(1, "", "", Vec::new()),
            Ok(ProfilePictureAvailability::Unavailable)
        );

        let cases = [
            (2, ProfilePictureError::InvalidJid),
            (3, ProfilePictureError::ClientUnavailable),
            (4, ProfilePictureError::RequestCancelled),
            (5, ProfilePictureError::Metadata),
            (6, ProfilePictureError::EmptyUrl),
            (7, ProfilePictureError::Download),
            (8, ProfilePictureError::Oversized),
            (9, ProfilePictureError::InvalidImage),
            (255, ProfilePictureError::InvalidBridgeResult),
        ];
        for (status, expected) in cases {
            assert_eq!(
                profile_picture_from_parts(status, "id", "preview", Vec::new()),
                Err(expected)
            );
        }
    }

    #[test]
    fn rejects_an_available_result_without_owned_bytes() {
        assert_eq!(
            profile_picture_from_parts(0, "id", "preview", Vec::new()),
            Err(ProfilePictureError::InvalidBridgeResult)
        );
    }
}

fn reaction_to_ffi(
    target_jid: &JID,
    destination_jid: &JID,
    sender_jid: &JID,
    message_id: &MessageId,
    reaction: &str,
) -> Result<(CString, CString, CString, CString, CString), MessageActionFailed> {
    Ok((
        CString::new(target_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(destination_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(sender_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(message_id.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(reaction).map_err(|_| MessageActionFailed)?,
    ))
}

pub fn react_to_message(
    chat_jid: &JID,
    sender_jid: &JID,
    message_id: &MessageId,
    reaction: &str,
) -> Result<(), MessageActionFailed> {
    react_to_message_in_chat(chat_jid, chat_jid, sender_jid, message_id, reaction)
}

pub fn react_to_message_in_chat(
    target_jid: &JID,
    destination_jid: &JID,
    sender_jid: &JID,
    message_id: &MessageId,
    reaction: &str,
) -> Result<(), MessageActionFailed> {
    let (target, destination, sender, id, reaction) = reaction_to_ffi(
        target_jid,
        destination_jid,
        sender_jid,
        message_id,
        reaction,
    )?;
    let result = unsafe {
        C_ReactToMessage(
            target.as_ptr(),
            destination.as_ptr(),
            sender.as_ptr(),
            id.as_ptr(),
            reaction.as_ptr(),
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(MessageActionFailed)
    }
}

fn edit_to_ffi(
    chat_jid: &JID,
    message_id: &MessageId,
    replacement: &str,
) -> Result<(CString, CString, CString), MessageActionFailed> {
    if replacement.trim().is_empty() {
        return Err(MessageActionFailed);
    }
    Ok((
        CString::new(chat_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(message_id.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(replacement).map_err(|_| MessageActionFailed)?,
    ))
}

pub fn edit_message(
    chat_jid: &JID,
    message_id: &MessageId,
    replacement: &str,
) -> Result<(), MessageActionFailed> {
    let (chat, id, replacement) = edit_to_ffi(chat_jid, message_id, replacement)?;
    let result = unsafe { C_EditMessage(chat.as_ptr(), id.as_ptr(), replacement.as_ptr()) };
    if result == 0 {
        Ok(())
    } else {
        Err(MessageActionFailed)
    }
}

fn revoke_to_ffi(
    chat_jid: &JID,
    sender_jid: &JID,
    message_id: &MessageId,
) -> Result<(CString, CString, CString), MessageActionFailed> {
    Ok((
        CString::new(chat_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(sender_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(message_id.as_ref()).map_err(|_| MessageActionFailed)?,
    ))
}

pub fn revoke_message(
    chat_jid: &JID,
    sender_jid: &JID,
    message_id: &MessageId,
) -> Result<(), MessageActionFailed> {
    let (chat, sender, id) = revoke_to_ffi(chat_jid, sender_jid, message_id)?;
    let result = unsafe { C_RevokeMessage(chat.as_ptr(), sender.as_ptr(), id.as_ptr()) };
    if result == 0 {
        Ok(())
    } else {
        Err(MessageActionFailed)
    }
}

pub fn download_file(file_id: &FileId, base_path: &Path) -> Result<(), DownloadFailed> {
    let file_id_c = CString::new(file_id.as_ref()).unwrap();
    let base_path_c = CString::new(base_path.to_str().unwrap()).unwrap();
    let code = unsafe { C_DownloadFile(file_id_c.as_ptr(), base_path_c.as_ptr()) };
    if code == 0 {
        Ok(())
    } else {
        Err(DownloadFailed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkAsReadError {
    Disconnected,
    Transient,
    Permanent,
}

fn with_borrowed_mark_read_args<T>(
    msg_id: &MessageId,
    chat_jid: &JID,
    sender_jid: &JID,
    send: impl FnOnce(*const c_char, CJID, CJID) -> T,
) -> Result<T, MarkAsReadError> {
    let msg_id_c = CString::new(msg_id.as_ref()).map_err(|_| MarkAsReadError::Permanent)?;
    let chat_jid_c = CString::new(chat_jid.0.as_ref()).map_err(|_| MarkAsReadError::Permanent)?;
    let sender_jid_c =
        CString::new(sender_jid.0.as_ref()).map_err(|_| MarkAsReadError::Permanent)?;
    Ok(send(
        msg_id_c.as_ptr(),
        chat_jid_c.as_ptr(),
        sender_jid_c.as_ptr(),
    ))
}

pub fn mark_as_read(
    msg_id: &MessageId,
    chat_jid: &JID,
    sender_jid: &JID,
) -> Result<(), MarkAsReadError> {
    let result =
        with_borrowed_mark_read_args(msg_id, chat_jid, sender_jid, |id, chat, sender| unsafe {
            C_MarkAsRead(id, chat, sender)
        })?;
    match result {
        0 => Ok(()),
        1 => Err(MarkAsReadError::Disconnected),
        3 => Err(MarkAsReadError::Permanent),
        _ => Err(MarkAsReadError::Transient),
    }
}

#[cfg(test)]
mod read_receipt_ffi_tests {
    use super::*;
    #[test]
    fn borrowed_ffi_arguments_can_be_reused_without_owned_pointer_leaks() {
        let id: MessageId = "message".into();
        let chat = JID::from("chat@s.whatsapp.net".to_owned());
        let sender = JID::from("sender@s.whatsapp.net".to_owned());
        for _ in 0..1_000 {
            with_borrowed_mark_read_args(&id, &chat, &sender, |id, chat, sender| {
                assert!(!id.is_null() && !chat.is_null() && !sender.is_null());
            })
            .unwrap();
        }
    }
}

pub fn pair_phone(phone: &str) -> String {
    let phone_c = CString::new(phone).unwrap();
    let result = unsafe { C_PairPhone(phone_c.as_ptr()) };
    let result_str = unsafe { CStr::from_ptr(result) }
        .to_string_lossy()
        .into_owned();
    result_str
}

pub fn new_client(db_path: &str) {
    let db_path_c = CString::new(db_path).unwrap();
    unsafe { C_NewClient(db_path_c.as_ptr()) }
}

impl CallbackTranslator<*const CEvent> for Event {
    unsafe fn to_rust(ptr: *const CEvent) -> Self {
        let event = unsafe { &(*ptr) };
        match EventType::from_repr(event.event_type).unwrap() {
            EventType::SyncProgress => {
                let percent = unsafe { *(event.data as *const u8) };
                Event::SyncProgress(percent)
            }
            EventType::AppStateSyncComplete => Event::AppStateSyncComplete,
            EventType::Receipt => {
                let receipt = unsafe { &(*(event.data as *const CReceipt)) };
                let chat: JID = (&receipt.chat).into();
                let message_ids = unsafe {
                    std::slice::from_raw_parts(receipt.message_ids, receipt.count as usize)
                }
                .iter()
                .map(|&id| {
                    unsafe { CStr::from_ptr(id) }
                        .to_string_lossy()
                        .into_owned()
                        .into()
                })
                .collect();

                Event::Receipt {
                    kind: receipt.kind,
                    chat,
                    message_ids,
                }
            }
            EventType::Reaction => unsafe {
                reaction_event_from_ffi(&*(event.data as *const CReactionEvent))
            },
            EventType::Connected => Event::Connected,
            EventType::MessageAction => unsafe {
                message_action_event_from_ffi(&*(event.data as *const CMessageActionEvent))
            },
            EventType::Chat => unsafe { chat_event_from_ffi(&*(event.data as *const CChatEvent)) },
            EventType::LogoutResult => unsafe {
                let result = &(*(event.data as *const CLogoutResultEvent));
                Event::LogoutResult(
                    LogoutStatus::from_repr(result.status).unwrap_or(LogoutStatus::Failed),
                )
            },
        }
    }
}

unsafe fn chat_event_from_ffi(event: &CChatEvent) -> Event {
    Event::Chat {
        jid: (&event.chat).into(),
        last_message_time: event.last_message_time,
    }
}

unsafe fn reaction_event_from_ffi(event: &CReactionEvent) -> Event {
    Event::Reaction {
        chat: (&event.chat).into(),
        target_message_id: unsafe { CStr::from_ptr(event.target_message_id) }
            .to_string_lossy()
            .into_owned()
            .into(),
        participant: (&event.participant).into(),
        text: unsafe { CStr::from_ptr(event.text) }
            .to_string_lossy()
            .into_owned()
            .into(),
        is_from_me: event.is_from_me,
    }
}

unsafe fn message_action_event_from_ffi(event: &CMessageActionEvent) -> Event {
    let kind = match event.kind {
        0 => MessageActionKind::Edit {
            replacement: unsafe { CStr::from_ptr(event.replacement) }
                .to_string_lossy()
                .into_owned()
                .into(),
        },
        1 => MessageActionKind::Delete,
        _ => unreachable!("Go only dispatches supported message action kinds"),
    };
    Event::MessageAction {
        action_id: unsafe { CStr::from_ptr(event.action_id) }
            .to_string_lossy()
            .into_owned()
            .into(),
        target_message_id: unsafe { CStr::from_ptr(event.target_message_id) }
            .to_string_lossy()
            .into_owned()
            .into(),
        chat: (&event.chat).into(),
        sender: (&event.sender).into(),
        kind,
        occurred_at: event.occurred_at,
        arrival_order: event.arrival_order,
    }
}

setup_handler!(
    set_event_handler,
    C_SetEventHandler,
    event: *const CEvent => Event
);

impl CallbackTranslator<CJID> for JID {
    unsafe fn to_rust(from: CJID) -> Self {
        (&from).into()
    }
}

impl CallbackTranslator<i64> for i64 {
    unsafe fn to_rust(value: i64) -> Self {
        value
    }
}

pub fn set_presence_handler<F>(mut callback: F)
where
    F: FnMut(PresenceUpdate) + 'static,
{
    setup_presence_handler(move |from, unavailable, last_seen| {
        if std::env::var("WPTUI_PRESENCE_DEBUG").as_deref() == Ok("1") {
            PRESENCE_CALLBACK_INGRESS.fetch_add(1, Ordering::Relaxed);
        }
        callback(PresenceUpdate {
            from,
            unavailable,
            last_seen,
        });
    });
}

setup_handler!(
    setup_presence_handler,
    C_SetPresenceHandler,
    from: CJID => JID,
    unavailable: bool => bool,
    last_seen: i64 => i64
);

impl CallbackTranslator<*const CMessage> for Message {
    unsafe fn to_rust(ptr: *const CMessage) -> Self {
        let msg = unsafe { &(*ptr) };
        let id = unsafe { CStr::from_ptr(msg.info.id) }
            .to_string_lossy()
            .into_owned()
            .into();
        let chat: JID = (&msg.info.chat).into();
        let sender: JID = (&msg.info.sender).into();
        let push_name = if msg.info.push_name.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(msg.info.push_name) }
                .to_string_lossy()
                .into_owned()
        };
        store_message_push_name(&id, &push_name);

        let c_quote_id = msg.info.quote_id;
        let quote_id = if c_quote_id.is_null() {
            None
        } else {
            Some(
                unsafe { CStr::from_ptr(c_quote_id) }
                    .to_string_lossy()
                    .into_owned()
                    .into(),
            )
        };

        let message = match MessageType::from_repr(msg.message_type).unwrap() {
            MessageType::Text => {
                let text_message = unsafe { &*(msg.message as *const CIncomingTextMessage) };

                let message = unsafe { CStr::from_ptr(text_message.text) }
                    .to_string_lossy()
                    .into_owned();
                let ranges = if text_message.mention_ranges.is_null()
                    || text_message.mention_range_count == 0
                {
                    Vec::new()
                } else {
                    validated_mention_ranges(&message, unsafe {
                        std::slice::from_raw_parts(
                            text_message.mention_ranges,
                            text_message.mention_range_count,
                        )
                    })
                };
                store_message_mention_ranges(&id, &message, ranges);
                MessageContent::Text(message.into())
            }
            MessageType::File => {
                let image_message = unsafe { &*(msg.message as *const CFileMessage) };

                let caption_text = if image_message.caption.is_null() {
                    String::new()
                } else {
                    unsafe { CStr::from_ptr(image_message.caption) }
                        .to_string_lossy()
                        .into_owned()
                };
                let ranges = if image_message.mention_ranges.is_null()
                    || image_message.mention_range_count == 0
                {
                    Vec::new()
                } else {
                    validated_mention_ranges(&caption_text, unsafe {
                        std::slice::from_raw_parts(
                            image_message.mention_ranges,
                            image_message.mention_range_count,
                        )
                    })
                };
                store_message_mention_ranges(&id, &caption_text, ranges);

                let path = unsafe { CStr::from_ptr(image_message.path) }
                    .to_string_lossy()
                    .into_owned()
                    .into();

                let file_id = unsafe { CStr::from_ptr(image_message.file_id) }
                    .to_string_lossy()
                    .into_owned()
                    .into();

                let caption = if image_message.caption.is_null() {
                    None
                } else {
                    Some(caption_text.into())
                };
                MessageContent::File(FileContent {
                    kind: FileKind::from_repr(image_message.kind).unwrap(),
                    path,
                    file_id,
                    caption,
                })
            }
        };

        let info = MessageInfo {
            id,
            chat,
            sender,
            mentions_self: msg.info.mentions_self,
            timestamp: msg.info.timestamp,
            is_from_me: msg.info.is_from_me,
            quote_id,
            read_by: msg.info.read_by,
            forwarding: ForwardingInfo {
                is_forwarded: msg.info.is_forwarded,
                score: msg.info.forwarding_score,
            },
        };
        if !msg.forward_source.is_null() && msg.forward_source_len > 0 {
            store_forward_source(&info, unsafe {
                std::slice::from_raw_parts(msg.forward_source, msg.forward_source_len).to_vec()
            });
        }
        Message { info, message }
    }
}

impl CallbackTranslator<bool> for bool {
    unsafe fn to_rust(ptr: bool) -> bool {
        ptr
    }
}

setup_handler!(
    set_message_handler,
    C_SetMessageHandler,
    msg: *const CMessage => Message,
    is_sync: bool => bool
);

impl CallbackTranslator<*const c_char> for String {
    unsafe fn to_rust(ptr: *const c_char) -> String {
        let c_str = unsafe { CStr::from_ptr(ptr) };
        c_str.to_string_lossy().into_owned()
    }
}

impl CallbackTranslator<u8> for u8 {
    unsafe fn to_rust(ptr: u8) -> u8 {
        ptr
    }
}

setup_handler!(
    set_log_handler,
    C_SetLogHandler,
    msg: *const c_char => String,
    level: u8 => u8
);

setup_handler!(connect, C_Connect, qr: *const c_char => String);

pub fn disconnect() {
    unsafe { C_Disconnect() }
}

pub fn logout() {
    // Performs a deterministic local sign-out on the Go side (disconnect +
    // clear the persisted device). The result arrives via Event::LogoutResult
    // so the app can remove the DB file and quit. No network round-trip, so
    // the UI never blocks.
    unsafe { C_Logout() };
}

unsafe fn take_owned_c_string(
    value: *mut c_char,
    free: unsafe extern "C" fn(*mut c_char),
) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let result = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned();
    unsafe { free(value) };
    Some(result)
}

pub fn drain_raw_presence_diagnostics() -> Option<String> {
    unsafe {
        let mut report = take_owned_c_string(
            C_DrainRawPresenceDiagnostics(),
            C_FreeRawPresenceDiagnostics,
        );
        if std::env::var("WPTUI_PRESENCE_DEBUG").as_deref() == Ok("1") {
            let ingress = PRESENCE_CALLBACK_INGRESS.swap(0, Ordering::Relaxed);
            report.get_or_insert_default().push_str(&format!(
                "Rust callback ingress Presence events: {ingress}\n"
            ));
        }
        report
    }
}

#[cfg(test)]
mod raw_presence_diagnostic_tests {
    use std::{
        ffi::{CString, c_char},
        sync::atomic::{AtomicBool, Ordering},
    };

    use super::take_owned_c_string;

    static FREED: AtomicBool = AtomicBool::new(false);

    unsafe extern "C" fn free_test_string(value: *mut c_char) {
        drop(unsafe { CString::from_raw(value) });
        FREED.store(true, Ordering::SeqCst);
    }

    #[test]
    fn owned_diagnostic_string_is_copied_and_freed_once() {
        FREED.store(false, Ordering::SeqCst);
        let value = CString::new("raw presence events received: 0\n")
            .unwrap()
            .into_raw();

        let report = unsafe { take_owned_c_string(value, free_test_string) };

        assert_eq!(report.as_deref(), Some("raw presence events received: 0\n"));
        assert!(FREED.load(Ordering::SeqCst));
        assert!(unsafe { take_owned_c_string(std::ptr::null_mut(), free_test_string) }.is_none());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SubscribePresenceResult {
    Accepted = 0,
    NoPrivacyToken = 1,
    Rejected = 2,
}

pub fn subscribe_presence(jid: &JID) -> SubscribePresenceResult {
    let jid = CString::new(jid.0.as_ref()).unwrap();
    match unsafe { C_SubscribePresence(jid.as_ptr()) } {
        0 => SubscribePresenceResult::Accepted,
        1 => SubscribePresenceResult::NoPrivacyToken,
        _ => SubscribePresenceResult::Rejected,
    }
}

/// Keeps CStrings and C structs alive for the duration of an FFI call.
/// The inner C structs are boxed so their heap addresses remain stable.
#[allow(dead_code)]
enum ContentHolder {
    Text(CString, Vec<CString>, Vec<CJID>, Box<CTextMessage>),
    File(
        CString,
        CString,
        Option<CString>,
        Vec<CString>,
        Vec<CJID>,
        Box<CFileMessage>,
    ),
}

fn build_content_for_ffi(
    content: &MessageContent,
    mentions: &[Mention],
) -> (u8, *const c_void, ContentHolder) {
    match content {
        MessageContent::Text(text) => {
            let text_c = CString::new(text.as_ref()).unwrap();
            let mention_strings = mentions
                .iter()
                .map(|mention| CString::new(mention.jid.0.as_ref()).unwrap())
                .collect::<Vec<_>>();
            let mention_pointers = mention_strings
                .iter()
                .map(|jid| jid.as_ptr())
                .collect::<Vec<_>>();
            let c_text = Box::new(CTextMessage {
                text: text_c.as_ptr(),
                mentioned_jids: mention_pointers.as_ptr(),
                mentioned_count: mention_pointers.len(),
            });
            let ptr = &*c_text as *const _ as *const c_void;
            (
                MessageType::Text as u8,
                ptr,
                ContentHolder::Text(text_c, mention_strings, mention_pointers, c_text),
            )
        }
        MessageContent::File(file) => {
            let path_c = CString::new(file.path.as_ref()).unwrap();
            let file_id_c = CString::new(file.file_id.as_ref()).unwrap();
            let caption_c = file
                .caption
                .as_ref()
                .map(|c| CString::new(c.as_ref()).unwrap());
            let caption_ptr = caption_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());
            let mention_strings = if file.caption.is_some() {
                mentions
                    .iter()
                    .map(|mention| CString::new(mention.jid.0.as_ref()).unwrap())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let mention_pointers = mention_strings
                .iter()
                .map(|jid| jid.as_ptr())
                .collect::<Vec<_>>();
            let c_file = Box::new(CFileMessage {
                kind: file_kind_discriminant(&file.kind),
                path: path_c.as_ptr(),
                file_id: file_id_c.as_ptr(),
                caption: caption_ptr,
                mentioned_jids: mention_pointers.as_ptr(),
                mentioned_count: mention_pointers.len(),
                mention_ranges: std::ptr::null(),
                mention_range_count: 0,
            });
            let ptr = &*c_file as *const _ as *const c_void;
            (
                MessageType::File as u8,
                ptr,
                ContentHolder::File(
                    path_c,
                    file_id_c,
                    caption_c,
                    mention_strings,
                    mention_pointers,
                    c_file,
                ),
            )
        }
    }
}

fn quote_to_ffi(quoted: Option<&Message>) -> (CString, *const c_char, CJID, CJID) {
    match quoted {
        Some(qm) => {
            let id_c = CString::new(qm.info.id.as_ref()).unwrap();
            let id_ptr = id_c.as_ptr();
            let sender = CJID::from(&qm.info.sender);
            let chat = CJID::from(&qm.info.chat);
            (id_c, id_ptr, sender, chat)
        }
        None => (
            CString::default(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        ),
    }
}

#[cfg(test)]
mod quote_ffi_tests {
    use super::{
        CStr, ContentHolder, FileContent, FileKind, ForwardingInfo, JID, Mention, Message,
        MessageContent, MessageInfo, build_content_for_ffi, quote_to_ffi,
    };

    #[test]
    fn quote_transport_preserves_the_original_status_chat() {
        let quoted = Message {
            info: MessageInfo {
                id: "status-id".into(),
                chat: JID::from("status@broadcast".to_owned()),
                sender: JID::from("alice@s.whatsapp.net".to_owned()),
                mentions_self: false,
                timestamp: 0,
                is_from_me: false,
                quote_id: None,
                read_by: 0,
                forwarding: ForwardingInfo::default(),
            },
            message: MessageContent::Text("status".into()),
        };

        let (_id_owner, id, sender, chat) = quote_to_ffi(Some(&quoted));
        assert_eq!(unsafe { CStr::from_ptr(id) }.to_str(), Ok("status-id"));
        assert_eq!(
            unsafe { CStr::from_ptr(sender) }.to_str(),
            Ok("alice@s.whatsapp.net")
        );
        assert_eq!(
            unsafe { CStr::from_ptr(chat) }.to_str(),
            Ok("status@broadcast")
        );
    }

    #[test]
    fn outbound_mentions_preserve_the_participant_jid_server() {
        let content = MessageContent::Text("@123".into());
        let mention = Mention {
            jid: JID::from("123@lid".to_owned()),
            numeric_user: "123".into(),
        };
        let (_, _, holder) = build_content_for_ffi(&content, &[mention]);
        let ContentHolder::Text(_, strings, pointers, _) = holder else {
            panic!("expected text FFI payload");
        };
        assert_eq!(strings.len(), 1);
        assert_eq!(pointers.len(), 1);
        assert_eq!(
            unsafe { CStr::from_ptr(pointers[0]) }.to_str(),
            Ok("123@lid")
        );
    }

    #[test]
    fn caption_file_ffi_carries_mentions_without_changing_file_fields() {
        let content = MessageContent::File(FileContent {
            kind: FileKind::Image,
            path: "image.png".into(),
            file_id: "".into(),
            caption: Some("@111".into()),
        });
        let mention = Mention {
            jid: JID::from("111@s.whatsapp.net".to_owned()),
            numeric_user: "111".into(),
        };
        let (message_type, _, holder) = build_content_for_ffi(&content, &[mention]);
        assert_eq!(message_type, 1);
        let ContentHolder::File(_, _, Some(_), _, pointers, file) = holder else {
            panic!("expected file FFI payload");
        };
        assert_eq!(pointers.len(), 1);
        assert_eq!(file.mentioned_count, 1);
    }
}

pub fn forward_message(source: &Message, destinations: &[JID]) -> ForwardReport {
    if destinations.is_empty() {
        return ForwardReport::default();
    }
    let Some(forward_source) = forward_source(&source.info) else {
        return ForwardReport::with_reason(
            0,
            destinations.len(),
            ForwardFailure::SourceUnavailable,
        );
    };
    let source_id = match CString::new(source.info.id.as_ref()) {
        Ok(value) => value,
        Err(_) => {
            return ForwardReport::with_reason(
                0,
                destinations.len(),
                ForwardFailure::InvalidSource,
            );
        }
    };
    let destination_values: Result<Vec<_>, _> = destinations
        .iter()
        .map(|jid| CString::new(jid.0.as_ref()))
        .collect();
    let Ok(destination_values) = destination_values else {
        return ForwardReport::with_reason(
            0,
            destinations.len(),
            ForwardFailure::InvalidDestination,
        );
    };
    let destination_pointers: Vec<_> = destination_values.iter().map(|jid| jid.as_ptr()).collect();
    let result = unsafe {
        C_ForwardMessage(
            source_id.as_ptr(),
            CJID::from(&source.info.chat),
            CJID::from(&source.info.sender),
            source.info.is_from_me,
            destination_pointers.as_ptr(),
            destination_pointers.len(),
            forward_source.as_ptr(),
            forward_source.len(),
        )
    };
    ForwardReport {
        succeeded: result.succeeded as usize,
        failed: result.failed as usize,
        failure: ForwardFailure::from_repr(result.failure).unwrap_or(ForwardFailure::SendFailed),
    }
}

pub fn send_message(
    jid: &JID,
    content: &MessageContent,
    quoted_message: Option<&Message>,
    mentions: &[Mention],
) {
    let jid_c = CJID::from(jid);
    let (msg_type, content_ptr, _holder) = build_content_for_ffi(content, mentions);
    let (_quote_id_owner, quote_id, quote_sender, quote_chat) = quote_to_ffi(quoted_message);
    let quote_content = quoted_message.map(|message| build_content_for_ffi(&message.message, &[]));
    let (quote_message_type, quote_message_content) = quote_content
        .as_ref()
        .map_or((0, std::ptr::null()), |(message_type, pointer, _)| {
            (*message_type, *pointer)
        });

    unsafe {
        C_SendMessage(
            jid_c,
            msg_type,
            content_ptr,
            quote_id,
            quote_sender,
            quote_chat,
            quote_message_type,
            quote_message_content,
        )
    }
}

/// Returns all contacts and groups as (JID, display name). Includes LID aliases for contacts.
pub fn get_contacts() -> Vec<(JID, Arc<str>)> {
    let result = unsafe { C_GetContacts() };
    let entries = unsafe { std::slice::from_raw_parts(result.entries, result.size as usize) };

    entries
        .iter()
        .map(|e| {
            let jid: JID = (&e.jid).into();
            let name = unsafe { CStr::from_ptr(e.name) }
                .to_string_lossy()
                .into_owned()
                .into();
            (jid, name)
        })
        .collect()
}

/// Returns real community roots and linked groups reported by WhatsApp.
pub fn get_communities() -> Result<Vec<CommunityInfo>, CommunitiesError> {
    let result = unsafe { C_GetCommunities() };
    if result.status != 0 {
        // C_GetCommunities transfers ownership even when the bridge reports
        // an error; the current error result is empty, but freeing it here
        // keeps that contract correct if it ever carries partial data.
        unsafe { C_FreeCommunities(result) };
        return Err(CommunitiesError::BridgeUnavailable);
    }
    if result.entries.is_null() || result.size == 0 {
        unsafe { C_FreeCommunities(result) };
        return Ok(Vec::new());
    }
    let entries = unsafe { std::slice::from_raw_parts(result.entries, result.size as usize) };
    let communities = entries
        .iter()
        .map(|entry| {
            let parent = unsafe { CStr::from_ptr(entry.parent_jid) }.to_string_lossy();
            CommunityInfo {
                jid: (&entry.jid).into(),
                name: unsafe { CStr::from_ptr(entry.name) }
                    .to_string_lossy()
                    .into_owned()
                    .into(),
                parent_jid: (!parent.is_empty()).then(|| parent.to_string().into()),
                is_parent: entry.is_parent,
            }
        })
        .collect::<Vec<_>>();
    unsafe { C_FreeCommunities(result) };
    Ok(communities)
}

pub fn get_chat_settings(jid: &JID) -> ChatSettings {
    let jid_c = CJID::from(jid);
    let settings = unsafe { C_GetChatSettings(jid_c) };

    ChatSettings {
        found: settings.found,
        muted_until: settings.muted_until,
        pinned: settings.pinned,
        archived: settings.archived,
    }
}

fn group_info_from_parts(
    jid: &JID,
    status: u8,
    is_announce: bool,
    is_admin: bool,
) -> Result<GroupInfo, GroupInfoError> {
    match status {
        0 => Ok(GroupInfo {
            jid: jid.clone(),
            name: "".into(),
            is_announce,
            is_admin,
        }),
        1 => Err(GroupInfoError::NotGroup),
        2 => Err(GroupInfoError::ClientUnavailable),
        3 => Err(GroupInfoError::RequestFailed),
        _ => Err(GroupInfoError::InvalidBridgeResult),
    }
}

pub fn get_group_info(jid: &JID) -> Result<GroupInfo, GroupInfoError> {
    let jid_c = CJID::from(jid);
    let result = unsafe { C_GetGroupInfo(jid_c) };
    group_info_from_parts(jid, result.status, result.is_announce, result.is_admin)
}

pub fn get_group_participants(jid: &JID) -> Vec<GroupParticipant> {
    let jid_c = CJID::from(jid);
    let result = unsafe { C_GetGroupParticipants(jid_c) };
    if result.entries.is_null() || result.size == 0 {
        return Vec::new();
    }
    let entries = unsafe { std::slice::from_raw_parts(result.entries, result.size as usize) };
    let participants = entries
        .iter()
        .filter_map(|entry| {
            let jid: JID = (!entry.jid.is_null()).then(|| (&entry.jid).into())?;
            let phone_number = if entry.phone_number.is_null() {
                jid.clone()
            } else {
                (&entry.phone_number).into()
            };
            let name = if entry.name.is_null() {
                Arc::<str>::from("")
            } else {
                unsafe { CStr::from_ptr(entry.name) }
                    .to_string_lossy()
                    .into_owned()
                    .into()
            };
            Some(GroupParticipant {
                jid,
                phone_number,
                name,
            })
        })
        .collect();
    unsafe { C_FreeGroupParticipants(result) };
    participants
}

#[cfg(test)]
mod group_info_tests {
    use super::{GroupInfoError, JID, group_info_from_parts};

    #[test]
    fn maps_announce_and_admin_flags() {
        let jid = JID("123@g.us".into());
        let info = group_info_from_parts(&jid, 0, true, false).unwrap();

        assert_eq!(info.jid, jid);
        assert!(info.is_announce);
        assert!(!info.is_admin);
    }

    #[test]
    fn maps_bridge_failures_without_claiming_send_permission() {
        for (status, expected) in [
            (1, GroupInfoError::NotGroup),
            (2, GroupInfoError::ClientUnavailable),
            (3, GroupInfoError::RequestFailed),
            (255, GroupInfoError::InvalidBridgeResult),
        ] {
            assert_eq!(
                group_info_from_parts(&JID("chat@g.us".into()), status, false, false),
                Err(expected)
            );
        }
    }
}

/// Resolves a group participant (or any user JID) to the canonical JID of
/// its direct conversation: a LID is mapped to its phone number when known,
/// matching how direct chats are keyed in the conversation list. Used so a
/// "reply privately" opens/sends to the real stored chat, not an empty
/// LID-keyed thread. Returns `None` only if the client is not ready or the
/// JID is unusable.
pub fn resolve_dm_chat(jid: &JID) -> Option<JID> {
    unsafe {
        take_owned_c_string(C_ResolveDmChatId(CJID::from(jid)), C_FreeResolveDmChatId)
            .map(JID::from)
    }
}
