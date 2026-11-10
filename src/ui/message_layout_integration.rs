use whatsrust as wr;

use crate::app::{App, FileMeta, Metadata};

use super::message_helpers::forwarding_label;
use super::message_layout::{self, LayoutInput, file_kind};
use super::{AuthorGroupContext, MessageTextMode, status_label};

pub fn message_height(
    message: &wr::Message,
    width: usize,
    is_selected: bool,
    author_group: AuthorGroupContext,
    app: &mut App,
) -> usize {
    let text_mode = if App::is_status_chat(&message.info.chat) {
        MessageTextMode::Status
    } else {
        MessageTextMode::Chat
    };
    message_height_with_mode(message, width, is_selected, author_group, app, text_mode)
}

pub(crate) fn message_height_with_mode(
    message: &wr::Message,
    width: usize,
    is_selected: bool,
    author_group: AuthorGroupContext,
    app: &mut App,
    text_mode: MessageTextMode,
) -> usize {
    let native = crate::ui::native_link_preview::summary(app, message, text_mode)
        .map(crate::ui::native_link_preview::OwnedSummary::from);
    let input = LayoutInput {
        width,
        incoming_urls: text_mode.incoming_urls(message),
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
            wr::MessageContent::Text(text) => text_content(
                text,
                forwarding_label(message),
                status_label(app, &message.info.id),
                native.as_ref(),
            ),
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
    text_mode: MessageTextMode,
) -> usize {
    if !app.messages.contains_key(id) {
        app.invalidate_message_sequences_containing(id);
        return 0;
    }
    let native = {
        let message = app.messages.get(id).expect("cached message must exist");
        crate::ui::native_link_preview::summary(app, message, text_mode)
            .map(crate::ui::native_link_preview::OwnedSummary::from)
    };
    let input = {
        let message = app.messages.get(id).expect("cached message must exist");
        LayoutInput {
            width,
            incoming_urls: text_mode.incoming_urls(message),
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
                wr::MessageContent::Text(text) => text_content(
                    text,
                    forwarding_label(message),
                    status_label(app, id),
                    native.as_ref(),
                ),
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

fn text_content<'a>(
    body: &'a str,
    forwarding: Option<super::message_helpers::ForwardingLabel>,
    status: Option<super::message_helpers::StatusLabel>,
    native: Option<&'a crate::ui::native_link_preview::OwnedSummary>,
) -> message_layout::HeightContent<'a> {
    match native {
        Some(native) => message_layout::HeightContent::NativeText {
            body,
            forwarding,
            status,
            native: native.summary(),
        },
        None => message_layout::HeightContent::Text {
            body,
            forwarding,
            status,
        },
    }
}

/// Refresh the bounded cache before trusting an anchor that may skip newer rows.
/// Metadata arrival/removal can change height without changing the raw text.
pub(crate) fn refresh_native_heights(app: &mut App, mode: MessageTextMode) {
    for id in app.message_height_cache.cached_ids() {
        let native = app
            .messages
            .get(&id)
            .and_then(|message| crate::ui::native_link_preview::summary(app, message, mode));
        if !app.message_height_cache.native_matches(&id, native) {
            app.message_height_cache.invalidate(&id);
        }
    }
}

pub(crate) fn retain_message_heights(app: &mut App, items: &[wr::MessageId]) {
    app.message_height_cache.retain_messages(items);
}

pub(crate) fn height_generation(app: &App) -> u64 {
    app.message_height_cache.generation()
}
