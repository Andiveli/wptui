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
};
pub use message_layout::{
    IMAGE_HEIGHT, IMAGE_WIDTH, MESSAGE_HEIGHT_CACHE_CAPACITY, MessageHeightCache, VIDEO_HEIGHT,
    VIDEO_WIDTH,
};
pub use message_list_state::MessageListState;
pub use message_media::preview_height;
use message_media::render_file;

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
    fn text_height_contract_handles_unicode_and_narrow_widths() {
        assert_eq!(super::message_layout::text_height("café 👩‍💻", 20), 1);
        assert_eq!(super::message_layout::text_height("café 👩‍💻", 4), 2);
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

    let sender_name = app.message_sender_name(message);
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
            .map(|quote| reply_summary(quote, &app.message_sender_name(quote)))
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
            let mention_ranges = wr::message_mention_ranges(&message.info.id, text);
            let lines =
                inline_content_lines(text, &mention_ranges, status, content_area.width as usize);
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
    let result = render_message_items(frame, app, list_area, items, unread_count);
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
    render_message_items(frame, app, area, items, 0);
}

/// Shared core of chat and status message rendering. The caller supplies
/// the messages newest-first (as rendered top-to-bottom).
fn render_message_items(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    items: Vec<wr::Message>,
    unread_count: usize,
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
    );

    None
}
