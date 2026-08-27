use std::ffi::{CString, c_char, c_void};

use crate::{
    ForwardFailure, ForwardReport,
    abi::{CFileMessage, CJID, CTextMessage, MessageType},
    file_kind_discriminant,
    models::{JID, Mention, Message, MessageContent},
};

/// Keeps CStrings and C structs alive for the duration of an FFI call.
/// The inner C structs are boxed so their heap addresses remain stable.
#[allow(dead_code)]
pub(crate) enum ContentHolder {
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

pub(crate) fn build_content_for_ffi(
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

pub(crate) fn quote_to_ffi(quoted: Option<&Message>) -> (CString, *const c_char, CJID, CJID) {
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

pub fn forward_message(source: &Message, destinations: &[JID]) -> ForwardReport {
    if destinations.is_empty() {
        return ForwardReport::default();
    }
    let Some(forward_source) = crate::forward_source(&source.info) else {
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
        crate::abi::C_ForwardMessage(
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
        crate::abi::C_SendMessage(
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
        crate::abi::C_SendTextMessage(
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
#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::*;
    use crate::models::{FileContent, FileKind, ForwardingInfo, JID, MessageInfo};
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
