use std::{
    collections::HashMap,
    ops::Range,
    sync::{Arc, LazyLock, Mutex},
};

use crate::abi::CMentionRange;
use crate::{JID, MessageId, MessageInfo};

static FORWARD_SOURCES: LazyLock<Mutex<HashMap<(Arc<str>, Arc<str>, Arc<str>), Vec<u8>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static MESSAGE_PUSH_NAMES: LazyLock<Mutex<HashMap<Arc<str>, Arc<str>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static MESSAGE_MENTION_RANGES: LazyLock<Mutex<HashMap<MessageId, (Arc<str>, Vec<Range<usize>>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn validated_mention_ranges(text: &str, ranges: &[CMentionRange]) -> Vec<Range<usize>> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
