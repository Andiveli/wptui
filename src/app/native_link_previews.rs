use std::{ops::Range, sync::Arc};

use whatsrust as wr;

use super::App;

/// Hydrated independently of the bridge's one-shot receive cache.
#[derive(Clone, Debug)]
pub struct NativeLinkPreviewRecord {
    pub chat: wr::JID,
    pub message_id: wr::MessageId,
    pub source: Arc<str>,
    pub preview: wr::NativeLinkPreview,
}

pub(crate) fn validated_preview(
    message: &wr::Message,
    preview: wr::NativeLinkPreview,
) -> Option<(Range<usize>, wr::NativeLinkPreview)> {
    if message.info.is_from_me || App::is_status_chat(&message.info.chat) {
        return None;
    }
    let wr::MessageContent::Text(text) = &message.message else {
        return None;
    };
    let preview = preview.bounded()?;
    let range = crate::url::extract_openable_url_ranges(text)
        .into_iter()
        .find(|range| &text[range.clone()] == preview.matched_text.as_ref())?;
    Some((range, preview))
}

impl App<'_> {
    /// Returns only app-owned metadata still bound to the current source URL.
    pub fn native_link_preview(
        &self,
        id: &wr::MessageId,
    ) -> Option<(Range<usize>, &wr::NativeLinkPreview)> {
        let record = self.native_link_previews.get(id)?;
        let message = self.messages.get(id)?;
        if record.chat != message.info.chat
            || message.info.is_from_me
            || Self::is_status_chat(&message.info.chat)
            || self
                .message_actions
                .get(id)
                .is_some_and(|actions| !actions.is_empty())
        {
            return None;
        }
        let wr::MessageContent::Text(text) = &message.message else {
            return None;
        };
        if text != &record.source {
            return None;
        }
        let range = crate::url::extract_openable_url_ranges(text)
            .into_iter()
            .find(|range| &text[range.clone()] == record.preview.matched_text.as_ref())?;
        Some((range, &record.preview))
    }

    pub(crate) fn restore_native_link_preview(&mut self, mut record: NativeLinkPreviewRecord) {
        let Some(message) = self.messages.get(&record.message_id) else {
            return;
        };
        if record.chat != message.info.chat
            || !matches!(&message.message, wr::MessageContent::Text(text) if text == &record.source)
            || self
                .message_actions
                .get(&record.message_id)
                .is_some_and(|actions| !actions.is_empty())
        {
            return;
        }
        let Some((_, preview)) = validated_preview(message, record.preview) else {
            return;
        };
        record.preview = preview;
        // App state is bounded independently of callback traffic and DB history.
        const CAPACITY: usize = 256;
        if !self.native_link_previews.contains_key(&record.message_id)
            && self.native_link_previews.len() >= CAPACITY
        {
            if let Some(evicted) = self.native_link_previews.keys().next().cloned() {
                self.native_link_previews.remove(&evicted);
            }
        }
        self.native_link_previews
            .insert(record.message_id.clone(), record);
    }

    pub(crate) fn prune_native_link_preview(&mut self, id: &wr::MessageId) {
        if self.native_link_preview(id).is_none() {
            self.native_link_previews.remove(id);
        }
    }
}
