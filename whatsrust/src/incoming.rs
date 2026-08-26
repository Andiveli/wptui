use std::ffi::CStr;

use crate::abi::{
    C_SetMessageHandler, C_SetOptimisticTextSentHandler, CFileMessage, CIncomingTextMessage,
    CMessage, MessageType,
};
use crate::caches::{
    store_forward_source, store_message_mention_ranges, store_message_push_name,
    validated_mention_ranges,
};
use crate::callbacks::CallbackTranslator;
use crate::{FileContent, FileKind, ForwardingInfo, JID, Message, MessageContent, MessageInfo};
use strum::FromRepr;

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
            MessageType::ViewOnceUnavailable => MessageContent::ViewOnceUnavailable,
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
