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

pub(crate) fn retain_message_heights(app: &mut App, items: &[wr::Message]) {
    let item_ids = items
        .iter()
        .map(|message| message.info.id.clone())
        .collect::<Vec<_>>();
    app.message_height_cache.retain_messages(&item_ids);
}

pub(crate) fn height_generation(app: &App) -> u64 {
    app.message_height_cache.generation()
}
