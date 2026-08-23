use whatsrust as wr;

use crate::app::{App, FileMeta, Metadata};

use super::message_helpers::forwarding_label;
use super::message_layout::{self, LayoutInput, file_kind};
use super::{AuthorGroupContext, status_label};

pub fn message_height(
    message: &wr::Message,
    width: usize,
    is_selected: bool,
    author_group: AuthorGroupContext,
    app: &mut App,
) -> usize {
    let input = LayoutInput {
        width,
        is_selected,
        has_quote: message.info.quote_id.is_some(),
        has_reactions: app.reactions.contains_key(&message.info.id),
        author_group,
        content: match &message.message {
            wr::MessageContent::ViewOnceUnavailable => {
                message_layout::HeightContent::Informational {
                    body: wr::VIEW_ONCE_UNAVAILABLE_DESCRIPTION,
                }
            }
            wr::MessageContent::Text(text) => message_layout::HeightContent::Text {
                body: text,
                forwarding: forwarding_label(message),
                status: status_label(app, &message.info.id),
            },
            wr::MessageContent::File(file) => message_layout::HeightContent::File {
                kind: file_kind(file.kind.clone()),
                caption: file.caption.as_deref(),
                forwarding: forwarding_label(message),
                preview_loaded: matches!(
                    app.metadata.get(&message.info.id),
                    Some(Metadata::File(FileMeta::Loaded))
                ),
                status: status_label(app, &message.info.id),
            },
        },
    };
    message_layout::height(&mut app.message_height_cache, &message.info.id, &input)
}

pub(crate) fn message_height_for_id(
    id: &wr::MessageId,
    width: usize,
    is_selected: bool,
    author_group: AuthorGroupContext,
    app: &mut App,
) -> usize {
    if !app.messages.contains_key(id) {
        app.invalidate_message_sequences_containing(id);
        return 0;
    }
    let input = {
        let message = app.messages.get(id).expect("cached message must exist");
        LayoutInput {
            width,
            is_selected,
            has_quote: message.info.quote_id.is_some(),
            has_reactions: app.reactions.contains_key(id),
            author_group,
            content: match &message.message {
                wr::MessageContent::ViewOnceUnavailable => {
                    message_layout::HeightContent::Informational {
                        body: wr::VIEW_ONCE_UNAVAILABLE_DESCRIPTION,
                    }
                }
                wr::MessageContent::Text(text) => message_layout::HeightContent::Text {
                    body: text,
                    forwarding: forwarding_label(message),
                    status: status_label(app, id),
                },
                wr::MessageContent::File(file) => message_layout::HeightContent::File {
                    kind: file_kind(file.kind.clone()),
                    caption: file.caption.as_deref(),
                    forwarding: forwarding_label(message),
                    preview_loaded: matches!(
                        app.metadata.get(id),
                        Some(Metadata::File(FileMeta::Loaded))
                    ),
                    status: status_label(app, id),
                },
            },
        }
    };
    message_layout::height(&mut app.message_height_cache, id, &input)
}

pub(crate) fn retain_message_heights(app: &mut App, items: &[wr::MessageId]) {
    app.message_height_cache.retain_messages(items);
}

pub(crate) fn height_generation(app: &App) -> u64 {
    app.message_height_cache.generation()
}
