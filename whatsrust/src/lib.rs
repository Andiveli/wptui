use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::Path,
    sync::Arc,
};

#[macro_use]
mod callbacks;
mod abi;
mod actions;
mod caches;
mod events;
mod incoming;
mod lifecycle;
mod media;
mod models;
mod presence;
mod read_sync;
mod registrations;
use abi::*;
pub use abi::{LogoutStatus, ReceiptKind};
pub use actions::{edit_message, react_to_message, react_to_message_in_chat, revoke_message};
pub use callbacks::CallbackTranslator;
pub use events::set_event_handler;
pub use lifecycle::{connect, disconnect, logout, new_client, pair_phone};
pub use media::{download_file, get_community_profile_picture, get_profile_picture};
pub(crate) use models::file_kind_discriminant;
pub use models::{
    ChatSettings, CommunitiesError, CommunityInfo, Contact, DownloadFailed, Event, FileContent,
    FileId, FileKind, ForwardingInfo, GroupInfo, GroupInfoError, GroupParticipant, JID,
    LogoutError, Mention, Message, MessageActionFailed, MessageActionKind, MessageContent,
    MessageId, MessageInfo, PresenceUpdate, ProfilePicture, ProfilePictureAvailability,
    ProfilePictureError,
};
pub use presence::{SubscribePresenceResult, drain_raw_presence_diagnostics, subscribe_presence};
pub use read_sync::{MarkAsReadError, mark_as_read, sync_chat_read};
pub use registrations::{
    set_log_handler, set_message_handler, set_optimistic_text_sent_handler, set_presence_handler,
};
use strum::FromRepr;

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

    use super::actions::{edit_to_ffi, reaction_to_ffi, revoke_to_ffi};
    use super::events::{message_action_event_from_ffi, reaction_event_from_ffi};
    use super::{CMessageActionEvent, CReactionEvent, Event, JID, MessageActionKind};

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

pub use caches::{forward_source, message_mention_ranges, message_push_name};
pub use caches::{
    remove_forward_source, store_forward_source, store_message_mention_ranges,
    store_message_push_name,
};

pub const VIEW_ONCE_UNAVAILABLE_DESCRIPTION: &str =
    "View-once media is unavailable here. View it on your phone.";

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

/// Keeps CStrings and C structs alive for the duration of an FFI call.
/// The inner C structs are boxed so their heap addresses remain stable.
#[allow(dead_code)]
enum ContentHolder {
    ViewOnceUnavailable,
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
        MessageContent::ViewOnceUnavailable => (
            MessageType::ViewOnceUnavailable as u8,
            std::ptr::null(),
            ContentHolder::ViewOnceUnavailable,
        ),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextSendResult {
    Sent,
    Failed,
}

pub fn send_text_message(
    jid: &JID,
    content: &MessageContent,
    quoted_message: Option<&Message>,
    mentions: &[Mention],
    local_send_id: u64,
) -> TextSendResult {
    let MessageContent::Text(_) = content else {
        return TextSendResult::Failed;
    };
    let jid_c = CJID::from(jid);
    let (_message_type, content_ptr, _holder) = build_content_for_ffi(content, mentions);
    let (_quote_id_owner, quote_id, quote_sender, quote_chat) = quote_to_ffi(quoted_message);
    let quote_content = quoted_message.map(|message| build_content_for_ffi(&message.message, &[]));
    let (quote_message_type, quote_message_content) = quote_content
        .as_ref()
        .map_or((0, std::ptr::null()), |(message_type, pointer, _)| {
            (*message_type, *pointer)
        });
    let status = unsafe {
        C_SendTextMessage(
            jid_c,
            content_ptr,
            quote_id,
            quote_sender,
            quote_chat,
            quote_message_type,
            quote_message_content,
            local_send_id,
        )
    };
    if status == 0 {
        TextSendResult::Sent
    } else {
        TextSendResult::Failed
    }
}

/// Returns all contacts and groups as (JID, display name). Includes LID aliases for contacts.
pub fn get_contacts() -> Vec<(JID, Arc<str>)> {
    let result = unsafe { C_GetContacts() };
    let entries = unsafe { std::slice::from_raw_parts(result.entries, result.size as usize) };

    let contacts = entries
        .iter()
        .map(|e| {
            let jid: JID = (&e.jid).into();
            let name = unsafe { CStr::from_ptr(e.name) }
                .to_string_lossy()
                .into_owned()
                .into();
            (jid, name)
        })
        .collect();
    unsafe { C_FreeContacts(result) };
    contacts
}

const COMMUNITY_ANNOUNCEMENT_UNKNOWN: u8 = 0;
const COMMUNITY_ANNOUNCEMENT_NO: u8 = 1;
const COMMUNITY_ANNOUNCEMENT_YES: u8 = 2;

fn community_announcement_from_code(code: u8) -> Option<bool> {
    match code {
        COMMUNITY_ANNOUNCEMENT_UNKNOWN => None,
        COMMUNITY_ANNOUNCEMENT_NO => Some(false),
        COMMUNITY_ANNOUNCEMENT_YES => Some(true),
        _ => None,
    }
}

fn community_participant_count_from_abi(value: i64) -> Option<u32> {
    (value >= 0).then(|| u32::try_from(value).ok()).flatten()
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
                is_joined: entry.is_joined,
                is_default_subgroup: entry.is_default_subgroup,
                is_announce: community_announcement_from_code(entry.announcement),
                participant_count: community_participant_count_from_abi(entry.participant_count),
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
    use super::{
        GroupInfoError, JID, community_announcement_from_code,
        community_participant_count_from_abi, group_info_from_parts,
    };

    #[test]
    fn maps_announce_and_admin_flags() {
        let jid = JID("123@g.us".into());
        let info = group_info_from_parts(&jid, 0, true, false).unwrap();

        assert_eq!(info.jid, jid);
        assert!(info.is_announce);
        assert!(!info.is_admin);
    }

    #[test]
    fn maps_community_announcement_tristate() {
        assert_eq!(community_announcement_from_code(0), None);
        assert_eq!(community_announcement_from_code(1), Some(false));
        assert_eq!(community_announcement_from_code(2), Some(true));
        assert_eq!(community_announcement_from_code(255), None);
    }

    #[test]
    fn maps_community_participant_count_without_truncation() {
        assert_eq!(community_participant_count_from_abi(-1), None);
        assert_eq!(community_participant_count_from_abi(0), Some(0));
        assert_eq!(
            community_participant_count_from_abi(i64::from(u32::MAX)),
            Some(u32::MAX)
        );
        assert_eq!(
            community_participant_count_from_abi(i64::from(u32::MAX) + 1),
            None
        );
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
        presence::take_owned_c_string(C_ResolveDmChatId(CJID::from(jid)), C_FreeResolveDmChatId)
            .map(JID::from)
    }
}
