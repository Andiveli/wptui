use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use whatsrust::{self as wr, FileKind};

use super::message_helpers::{
    AuthorGroupContext, ForwardingLabel, StatusLabel, forwarding_indicator_lines,
    inline_content_lines,
};

pub const IMAGE_HEIGHT: usize = 12;
pub const IMAGE_WIDTH: usize = IMAGE_HEIGHT * 3;
pub const VIDEO_HEIGHT: usize = IMAGE_HEIGHT;
pub const VIDEO_WIDTH: usize = 72;
pub const MESSAGE_HEIGHT_CACHE_CAPACITY: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
enum OwnedHeightContent {
    Text {
        body: Arc<str>,
        forwarding: Option<ForwardingLabel>,
        status: Option<StatusLabel>,
    },
    File {
        kind: HeightFileKind,
        caption: Option<Arc<str>>,
        preview_loaded: bool,
        forwarding: Option<ForwardingLabel>,
        status: Option<StatusLabel>,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HeightContent<'a> {
    Text {
        body: &'a str,
        forwarding: Option<ForwardingLabel>,
        status: Option<StatusLabel>,
    },
    File {
        kind: HeightFileKind,
        caption: Option<&'a str>,
        preview_loaded: bool,
        forwarding: Option<ForwardingLabel>,
        status: Option<StatusLabel>,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HeightFileKind {
    Image,
    Video,
    Audio,
    Document,
    Sticker,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LayoutInput<'a> {
    pub(crate) width: usize,
    pub(crate) is_selected: bool,
    pub(crate) has_quote: bool,
    pub(crate) has_reactions: bool,
    pub(crate) author_group: AuthorGroupContext,
    pub(crate) content: HeightContent<'a>,
}
#[derive(Clone, Debug)]
struct MessageHeightEntry {
    input: OwnedLayoutInput,
    height: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct OwnedLayoutInput {
    width: usize,
    is_selected: bool,
    has_quote: bool,
    has_reactions: bool,
    author_group: AuthorGroupContext,
    content: OwnedHeightContent,
}
impl<'a> From<&LayoutInput<'a>> for OwnedLayoutInput {
    fn from(input: &LayoutInput<'a>) -> Self {
        Self {
            width: input.width,
            is_selected: input.is_selected,
            has_quote: input.has_quote,
            has_reactions: input.has_reactions,
            author_group: input.author_group,
            content: match input.content {
                HeightContent::Text {
                    body,
                    forwarding,
                    status,
                } => OwnedHeightContent::Text {
                    body: body.into(),
                    forwarding,
                    status,
                },
                HeightContent::File {
                    kind,
                    caption,
                    preview_loaded: _,
                    forwarding,
                    status,
                } => OwnedHeightContent::File {
                    kind,
                    caption: caption.map(Into::into),
                    preview_loaded: false,
                    forwarding,
                    status,
                },
            },
        }
    }
}
impl OwnedLayoutInput {
    fn matches(&self, input: &LayoutInput<'_>) -> bool {
        self.width == input.width
            && self.is_selected == input.is_selected
            && self.has_quote == input.has_quote
            && self.has_reactions == input.has_reactions
            && self.author_group == input.author_group
            && match (&self.content, input.content) {
                (
                    OwnedHeightContent::Text {
                        body,
                        forwarding,
                        status,
                    },
                    HeightContent::Text {
                        body: other,
                        forwarding: other_forwarding,
                        status: other_status,
                    },
                ) => {
                    body.as_ref() == other
                        && *forwarding == other_forwarding
                        && *status == other_status
                }
                (
                    OwnedHeightContent::File {
                        kind,
                        caption,
                        preview_loaded: _,
                        forwarding,
                        status,
                    },
                    HeightContent::File {
                        kind: other_kind,
                        caption: other_caption,
                        preview_loaded: _,
                        forwarding: other_forwarding,
                        status: other_status,
                    },
                ) => {
                    *kind == other_kind
                        && caption.as_deref() == other_caption
                        && *forwarding == other_forwarding
                        && *status == other_status
                }
                _ => false,
            }
    }
}
#[derive(Debug, Default)]
pub struct MessageHeightCache {
    entries: HashMap<wr::MessageId, MessageHeightEntry>,
    order: VecDeque<wr::MessageId>,
    generation: u64,
    measurements: u64,
}

impl MessageHeightCache {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn measurement_count(&self) -> u64 {
        self.measurements
    }

    pub fn invalidate(&mut self, id: &wr::MessageId) {
        self.entries.remove(id);
        self.order.retain(|cached| cached != id);
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn mark_layout_changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn retain_messages(&mut self, ids: &[wr::MessageId]) {
        let retained: HashSet<_> = ids.iter().cloned().collect();
        self.entries.retain(|id, _| retained.contains(id));
        self.order.retain(|id| retained.contains(id));
    }

    fn get(&mut self, id: &wr::MessageId, input: &LayoutInput<'_>) -> Option<usize> {
        let height = self
            .entries
            .get(id)
            .filter(|entry| entry.input.matches(input))
            .map(|entry| entry.height)?;
        self.order.retain(|cached| cached != id);
        self.order.push_back(id.clone());
        Some(height)
    }

    fn insert(&mut self, id: wr::MessageId, input: &LayoutInput<'_>, height: usize) {
        if !self.entries.contains_key(&id)
            && self.entries.len() >= MESSAGE_HEIGHT_CACHE_CAPACITY
            && let Some(oldest) = self.order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        self.order.retain(|cached| cached != &id);
        self.order.push_back(id.clone());
        self.entries.insert(
            id,
            MessageHeightEntry {
                input: input.into(),
                height,
            },
        );
    }
}

#[cfg(test)]
pub(crate) fn text_height(body: &str, width: usize) -> usize {
    inline_content_lines(body, &[], None, width).len()
}

pub(crate) fn height(
    cache: &mut MessageHeightCache,
    id: &wr::MessageId,
    input: &LayoutInput<'_>,
) -> usize {
    if let Some(height) = cache.get(id, &input) {
        return height;
    }

    cache.measurements = cache.measurements.wrapping_add(1);

    let content_width = if input.is_selected {
        input.width.saturating_sub(3)
    } else {
        input.width
    }
    .max(1);
    let content_height = match &input.content {
        HeightContent::Text {
            body,
            status,
            forwarding,
        } => {
            inline_content_lines(body, &[], *status, content_width).len()
                + forwarding_indicator_lines(*forwarding, content_width)
        }
        HeightContent::File {
            kind,
            caption,
            preview_loaded: _,
            forwarding,
            status,
        } => {
            let caption_height = caption.as_ref().map_or_else(
                || inline_content_lines("", &[], *status, content_width).len(),
                |caption| inline_content_lines(caption, &[], *status, content_width).len(),
            );
            let file_height = match kind {
                // Reserve media geometry independently of the asynchronous
                // preview lifecycle so the message list cannot reflow.
                HeightFileKind::Video => VIDEO_HEIGHT,
                HeightFileKind::Image | HeightFileKind::Sticker => IMAGE_HEIGHT,
                HeightFileKind::Audio => 2,
                HeightFileKind::Document => 1,
            };
            file_height + caption_height + forwarding_indicator_lines(*forwarding, content_width)
        }
    };

    let height = usize::from(input.author_group.starts_group() || input.is_selected)
        + usize::from(input.has_quote)
        + content_height
        + usize::from(input.has_reactions)
        + usize::from(input.is_selected);
    cache.insert(id.clone(), input, height);
    height
}

pub(crate) fn file_kind(kind: FileKind) -> HeightFileKind {
    match kind {
        FileKind::Image => HeightFileKind::Image,
        FileKind::Video => HeightFileKind::Video,
        FileKind::Audio => HeightFileKind::Audio,
        FileKind::Document => HeightFileKind::Document,
        FileKind::Sticker => HeightFileKind::Sticker,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(body: &str) -> LayoutInput<'_> {
        LayoutInput {
            width: 20,
            is_selected: false,
            has_quote: false,
            has_reactions: false,
            author_group: AuthorGroupContext::STARTS_GROUP,
            content: HeightContent::Text {
                body,
                forwarding: None,
                status: None,
            },
        }
    }
    #[test]
    fn cache_hit_refreshes_recency_before_eviction() {
        let mut cache = MessageHeightCache::default();
        for index in 0..MESSAGE_HEIGHT_CACHE_CAPACITY {
            let id: Arc<str> = format!("message-{index}").into();
            height(&mut cache, &id, &text("cached"));
        }
        height(&mut cache, &Arc::from("message-0"), &text("cached"));
        height(&mut cache, &Arc::from("new"), &text("cached"));

        assert!(cache.contains("message-0"));
        assert!(!cache.contains("message-1"));
    }
    #[test]
    fn same_id_changed_input_replaces_snapshot_and_recalculates() {
        let mut cache = MessageHeightCache::default();
        let id: Arc<str> = "same-id".into();

        assert_eq!(height(&mut cache, &id, &text("short")), 2);
        assert_eq!(
            height(&mut cache, &id, &text("a body that definitely wraps")),
            3
        );
    }
    #[test]
    fn every_file_kind_and_preview_state_has_a_stable_height() {
        let kinds = [
            HeightFileKind::Image,
            HeightFileKind::Video,
            HeightFileKind::Audio,
            HeightFileKind::Document,
            HeightFileKind::Sticker,
        ];
        let mut cache = MessageHeightCache::default();
        for (index, kind) in kinds.into_iter().enumerate() {
            let id: Arc<str> = format!("file-{index}").into();
            let input = LayoutInput {
                content: HeightContent::File {
                    kind,
                    caption: Some("caption"),
                    preview_loaded: false,
                    forwarding: None,
                    status: None,
                },
                ..text("")
            };
            assert_eq!(
                height(&mut cache, &id, &input),
                match kind {
                    HeightFileKind::Audio => 4,
                    HeightFileKind::Image | HeightFileKind::Video | HeightFileKind::Sticker => 14,
                    HeightFileKind::Document => 3,
                }
            );
            let loaded = LayoutInput {
                content: HeightContent::File {
                    kind,
                    caption: Some("caption"),
                    preview_loaded: true,
                    forwarding: None,
                    status: None,
                },
                ..text("")
            };
            assert_eq!(
                height(&mut cache, &id, &loaded),
                match kind {
                    HeightFileKind::Audio => 4,
                    HeightFileKind::Image | HeightFileKind::Video | HeightFileKind::Sticker => 14,
                    HeightFileKind::Document => 3,
                }
            );
        }
    }
}
