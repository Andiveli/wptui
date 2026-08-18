use chrono::{DateTime, Datelike, Local};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};
use whatsrust as wr;

use crate::app::App;

#[path = "bidi.rs"]
mod bidi;
#[path = "message_formatting.rs"]
mod message_formatting;
#[path = "message_helpers.rs"]
mod message_helpers;
#[path = "message_layout.rs"]
mod message_layout;
#[path = "message_layout_integration.rs"]
mod message_layout_integration;
#[path = "message_list_reconciliation.rs"]
mod message_list_reconciliation;
#[path = "message_list_state.rs"]
mod message_list_state;
#[path = "message_media.rs"]
mod message_media;
#[path = "message_viewport.rs"]
mod message_viewport;

pub use message_formatting::reaction_chips;
use message_formatting::{author_color, media_paragraph, message_block, message_content_area};
pub use message_helpers::{
    AUTHOR_GROUP_MAX_GAP, AuthorGroupContext, get_quoted_text, reply_summary, starts_author_group,
};
use message_helpers::{
    StatusLabel, forwarding_indicator_lines, forwarding_label, inline_content_lines,
    inline_content_lines_logical,
};
pub use message_layout::{
    IMAGE_HEIGHT, IMAGE_WIDTH, MESSAGE_HEIGHT_CACHE_CAPACITY, MessageHeightCache, VIDEO_HEIGHT,
    VIDEO_WIDTH,
};
pub use message_list_state::MessageListState;
pub use message_media::preview_height;
use message_media::render_file;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MessageTextMode {
    Chat,
    Status,
}

fn status_label(app: &App, message_id: &wr::MessageId) -> Option<StatusLabel> {
    let status = app.message_status(message_id);
    status
        .deleted
        .then_some(StatusLabel::Deleted)
        .or_else(|| status.edited.then_some(StatusLabel::Edited))
}

#[cfg(test)]
mod layout_contract_tests {
    #[test]
    fn rendered_cells_receive_visual_text_once() {
        use ratatui::{
            buffer::Buffer,
            layout::Rect,
            widgets::{Paragraph, Widget},
        };

        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 1));
        Paragraph::new(super::inline_content_lines("abc אבג 123", None, 20))
            .render(Rect::new(0, 0, 20, 1), &mut buffer);
        let rendered = (0..12).map(|x| buffer[(x, 0)].symbol()).collect::<String>();

        assert_eq!(rendered, "abc 123 גבא ");
    }

    #[test]
    fn status_content_keeps_logical_order() {
        let lines = super::inline_content_lines_logical(
            "abc אבג 123",
            Some(super::StatusLabel::Edited),
            20,
        );
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(rendered, ["abc אבג 123 (edited)"]);
        assert!(lines[0].spans.iter().any(|span| {
            span.content.as_ref() == "(edited)"
                && span.style == ratatui::style::Style::default().dark_gray()
        }));
    }

    #[test]
    fn rtl_status_label_stays_styled_after_reordering() {
        let lines = super::inline_content_lines("אבג", Some(super::StatusLabel::Edited), 20);
        let line = &lines[0];

        assert_eq!(
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
            ")edited( גבא"
        );
        assert!(line.spans.iter().any(|span| {
            span.content.as_ref() == ")edited("
                && span.style == ratatui::style::Style::default().dark_gray()
        }));
    }

    #[test]
    fn caption_cells_follow_visual_order_after_logical_wrapping() {
        use ratatui::{
            buffer::Buffer,
            layout::Rect,
            widgets::{Paragraph, Widget},
        };

        let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 3));
        Paragraph::new(super::inline_content_lines("abc אבג 123", None, 4))
            .render(Rect::new(0, 0, 4, 3), &mut buffer);
        let rendered = (0..3)
            .map(|y| (0..4).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>();

        assert_eq!(rendered, ["abc ", "גבא ", "123 "]);
    }

    #[test]
    fn text_height_contract_handles_unicode_and_narrow_widths() {
        assert_eq!(super::message_layout::text_height("café 👩‍💻", 20), 1);
        assert_eq!(super::message_layout::text_height("café 👩‍💻", 4), 2);
    }

    #[test]
    fn logical_wrap_precedes_per_line_visual_reordering() {
        let lines = super::inline_content_lines("abc אבג 123", None, 4);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(rendered, ["abc", "גבא", "123"]);
    }

    #[test]
    fn visual_wrapping_and_cached_height_agree_at_narrow_width() {
        use std::sync::Arc;

        let body = "abc אבג 123";
        let width = 4;
        let lines = super::inline_content_lines(body, None, width);
        let input = super::message_layout::LayoutInput {
            width,
            is_selected: false,
            has_quote: false,
            has_reactions: false,
            author_group: super::AuthorGroupContext::STARTS_GROUP,
            content: super::message_layout::HeightContent::Text {
                body,
                forwarding: None,
                status: None,
            },
        };
        let mut cache = super::MessageHeightCache::default();
        let cached = super::message_layout::height(&mut cache, &Arc::from("rtl"), &input);
        let caption_input = super::message_layout::LayoutInput {
            content: super::message_layout::HeightContent::File {
                kind: super::message_layout::HeightFileKind::Document,
                caption: Some(body),
                preview_loaded: false,
                forwarding: None,
                status: None,
            },
            ..input.clone()
        };
        let caption_cached =
            super::message_layout::height(&mut cache, &Arc::from("rtl-caption"), &caption_input);

        assert_eq!(cached, lines.len() + 1);
        assert_eq!(caption_cached, lines.len() + 2);
        assert_eq!(super::message_layout::text_height(body, width), lines.len());
    }
}

pub use message_layout_integration::message_height;

fn spacing_after_message(
    index: usize,
    author_groups: &[AuthorGroupContext],
    selected: Option<usize>,
) -> usize {
    let has_older_neighbor = index + 1 < author_groups.len();
    let continuation = !author_groups[index].starts_group();
    let selected_at_boundary = selected == Some(index) || selected == Some(index + 1);

    usize::from(has_older_neighbor && (!continuation || selected_at_boundary))
}

/// When `render_image` is false (partial path and image fully off-screen), show a placeholder
/// instead of StatefulImage so we don't mark the protocol as "transmitted" until we actually
/// send at least one row to the frame.
fn render_message(
    buf: &mut Buffer,
    message: &wr::Message,
    is_selected: bool,
    author_group: AuthorGroupContext,
    app: &mut App,
    area: Rect,
    render_image: bool,
    text_mode: MessageTextMode,
) {
    let alignment = ratatui::layout::Alignment::Left;
    // let alignment = if message.info.is_from_me {
    //     ratatui::layout::Alignment::Right
    // } else {
    //     ratatui::layout::Alignment::Left
    // };

    let timestamp_text = {
        let local_time: DateTime<Local> = DateTime::from_timestamp(message.info.timestamp, 0)
            .unwrap()
            .into();
        if local_time.date_naive() == Local::now().date_naive() {
            local_time.format("%H:%M").to_string()
        } else if local_time.date_naive() == (Local::now() - chrono::Duration::days(1)).date_naive()
        {
            local_time.format("Yesterday %H:%M").to_string()
        } else if local_time > (Local::now() - chrono::Duration::days(7)) {
            local_time.format("%a %H:%M").to_string()
        } else if local_time.year() == Local::now().year() {
            local_time.format("%d %b %H:%M").to_string()
        } else {
            local_time.format("%Y %d %b %H:%M").to_string()
        }
    };
    let timestamp = timestamp_text.clone().italic();

    let sender_name = app.contact_name(&message.info.sender);
    let sender_color = author_color(&message.info.sender);

    let mut header = vec![
        Span::styled(
            sender_name.to_string(),
            Style::default().fg(sender_color).bold(),
        ),
        " (".into(),
        timestamp.clone(),
        ")".into(),
    ];
    if message.info.read_by >= 1 {
        header.push(" ✓".into());
    }
    header.push(" ".into());
    let msg_block = message_block(header, timestamp, is_selected, author_group);

    // let sender_widget = Line::from_iter(header).alignment(alignment).bold();

    let quoted_text = message.info.quote_id.as_ref().and_then(|quote_id| {
        app.messages
            .get(quote_id)
            .map(|quote| reply_summary(quote, &app.contact_name(&quote.info.sender)))
    });

    let quote_widget = message.info.quote_id.as_ref().map(|_quote_id| {
        let quoted_text = quoted_text.unwrap_or_else(|| "not found".into());

        Line::from(quoted_text).alignment(alignment).dark_gray()
    });
    let status = status_label(app, &message.info.id);
    let forwarding = forwarding_label(message);

    let msg_area = message_content_area(msg_block.inner(area), is_selected);
    // let msg_area = area;
    // let msg_area = if message.info.is_from_me {
    //     let [_, b] = Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)])
    //         .areas(area);
    //     b
    // } else {
    //     let [a, _] = Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)])
    //         .areas(area);
    //     a
    // };

    // let [sender_area, quoted_area, content_area] = Layout::vertical([
    //     Constraint::Length(1),
    //     Quote height is allocated by the active layout below.
    //     Constraint::Min(1),
    // ])
    let [quoted_area, forwarding_area, content_area, reactions_area] = Layout::vertical([
        Constraint::Length(if quote_widget.is_some() { 1 } else { 0 }),
        Constraint::Length(forwarding_indicator_lines(forwarding, msg_area.width as usize) as u16),
        Constraint::Min(0),
        Constraint::Length(u16::from(app.reactions.contains_key(&message.info.id))),
    ])
    .areas(msg_area);

    msg_block.render(area, buf);
    // sender_widget.render(sender_area, buf);
    if let Some(quoted_widget) = quote_widget {
        quoted_widget.render(quoted_area, buf);
    }
    if let Some(forwarding) = forwarding {
        Paragraph::new(forwarding.text())
            .dark_gray()
            .wrap(Wrap { trim: true })
            .render(forwarding_area, buf);
    }
    match &message.message {
        wr::MessageContent::Text(text) => {
            let lines = match text_mode {
                MessageTextMode::Chat => {
                    inline_content_lines(text, status, content_area.width as usize)
                }
                MessageTextMode::Status => {
                    inline_content_lines_logical(text, status, content_area.width as usize)
                }
            };
            Paragraph::new(lines)
                .alignment(alignment)
                .render(content_area, buf);
        }
        wr::MessageContent::File(data) => {
            render_file(
                buf,
                &message.info.id,
                data,
                status,
                app,
                content_area,
                render_image,
                alignment,
                text_mode,
            );
        }
    };
    if !reactions_area.is_empty() {
        Paragraph::new(reaction_chips(app.reactions.get(&message.info.id)).join(" "))
            .alignment(alignment)
            .render(reactions_area, buf);
    }
}

pub fn render_messages(frame: &mut Frame, app: &mut App, area: Rect) -> Option<()> {
    crate::crash_diagnostics::breadcrumb("first-message-render", "start");
    let chat_jid = app.open_chat()?;
    if area.is_empty() {
        return Some(());
    }

    let unread_count = app.unread_boundary(&chat_jid).map_or(0, |(count, _)| count);
    let banner = app.unread_boundary(&chat_jid).map(|(count, since)| {
        format!(
            " {count} unread messages since {}",
            DateTime::<chrono::Utc>::from_timestamp(since, 0)
                .map(DateTime::<Local>::from)
                .unwrap()
                .format("%Y-%m-%d %H:%M")
        )
    });
    let pending = app.pending_new_messages(&chat_jid);
    let notice_height = u16::from(banner.is_some());
    let pending_height = u16::from(pending > 0);
    let notice_area = Rect::new(area.x, area.y, area.width, notice_height);
    let list_area = Rect::new(
        area.x,
        area.y.saturating_add(notice_height),
        area.width,
        area.height
            .saturating_sub(notice_height)
            .saturating_sub(pending_height),
    );
    let pending_area = Rect::new(
        area.x,
        area.bottom().saturating_sub(pending_height),
        area.width,
        pending_height,
    );

    let items: Vec<_> = app
        .chat_messages
        .get(&chat_jid)?
        .iter()
        .rev()
        .filter_map(|msg_id| app.messages.get(msg_id).cloned())
        .collect();
    let result = render_message_items(
        frame,
        app,
        list_area,
        items,
        unread_count,
        MessageTextMode::Chat,
    );
    crate::crash_diagnostics::breadcrumb("first-message-render", "complete");
    if let Some(text) = banner {
        Paragraph::new(text)
            .style(
                Style::default()
                    .fg(Color::Reset)
                    .bg(Color::Rgb(37, 211, 102)),
            )
            .render(notice_area, frame.buffer_mut());
    }
    if pending > 0 && !pending_area.is_empty() {
        let label = format!("↓ {pending} new messages ");
        let width = label.chars().count().min(pending_area.width as usize) as u16;
        let x = pending_area.x + pending_area.width.saturating_sub(width) / 2;
        Paragraph::new(Line::styled(label, Style::default().fg(Color::Cyan).bold()))
            .render(Rect::new(x, pending_area.y, width, 1), frame.buffer_mut());
    }
    result
}

/// Read-only statuses of the opened status contact, rendered with the
/// same machinery as chats (media previews, timestamps, sender header).
pub fn render_status_messages(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(contact) = app.open_status_contact() else {
        return;
    };
    let items = app
        .status_messages(&contact)
        .iter()
        .rev()
        .filter_map(|id| app.messages.get(id).cloned())
        .collect::<Vec<_>>();
    render_message_items(frame, app, area, items, 0, MessageTextMode::Status);
}

/// Shared core of chat and status message rendering. The caller supplies
/// the messages newest-first (as rendered top-to-bottom).
fn render_message_items(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    items: Vec<wr::Message>,
    unread_count: usize,
    text_mode: MessageTextMode,
) -> Option<()> {
    let list_area = area;
    if list_area.is_empty() {
        return Some(());
    }

    let author_groups = items
        .iter()
        .enumerate()
        .map(|(index, message)| {
            AuthorGroupContext::new(starts_author_group(items.get(index + 1), message))
        })
        .collect::<Vec<_>>();

    message_layout_integration::retain_message_heights(app, &items);

    if items.is_empty() {
        app.message_list_state.select(None);
        return Some(());
    }

    let width = list_area.width as isize;
    let (start_index, y) = message_list_reconciliation::reconcile(
        app,
        list_area,
        &items,
        &author_groups,
        unread_count,
    );
    app.message_list_state.viewport_anchor = message_viewport::render(
        frame,
        app,
        list_area,
        &items,
        &author_groups,
        unread_count,
        width,
        start_index,
        y,
        text_mode,
    );

    None
}
