use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use chrono::{DateTime, Datelike, Local};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use whatsrust as wr;

use crate::app::{App, FileMeta, Metadata};

#[path = "message_helpers.rs"]
mod message_helpers;
#[path = "message_layout.rs"]
mod message_layout;
#[path = "message_list_state.rs"]
mod message_list_state;
#[path = "message_media.rs"]
mod message_media;
#[path = "message_viewport.rs"]
mod message_viewport;

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
use message_layout::{LayoutInput, file_kind};
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

/// Palette inspired by WhatsApp's group chat author colors.
/// Picked for solid contrast on both black and dark-gray backgrounds.
const AUTHOR_PALETTE: &[Color] = &[
    Color::Rgb(0xE7, 0x9F, 0x3C), // amber
    Color::Rgb(0x6F, 0xC9, 0xCE), // teal
    Color::Rgb(0xC9, 0x8F, 0xE7), // lavender
    Color::Rgb(0x8F, 0xC9, 0x4F), // lime
    Color::Rgb(0xE7, 0x6F, 0x8F), // pink
    Color::Rgb(0x6F, 0x8F, 0xE7), // indigo
    Color::Rgb(0xE7, 0x4F, 0x4F), // red
    Color::Rgb(0x4F, 0xC9, 0x8F), // mint
    Color::Rgb(0xC9, 0x8F, 0x4F), // bronze
    Color::Rgb(0x8F, 0x4F, 0xC9), // purple
    Color::Rgb(0x4F, 0x8F, 0xC9), // sky
    Color::Rgb(0xC9, 0x4F, 0x8F), // magenta
];

/// Deterministic color for a sender JID. Same sender -> same color everywhere.
/// Uses FxHash-free, std-only hash so we don't pull in a new dep.
fn author_color(sender: &wr::JID) -> Color {
    // FNV-1a over the JID bytes — stable, no allocation, good distribution.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in sender.0.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    AUTHOR_PALETTE[(hash as usize) % AUTHOR_PALETTE.len()]
}

fn message_block<'a>(
    mut header: Vec<Span<'a>>,
    timestamp: Span<'a>,
    is_selected: bool,
    author_group: AuthorGroupContext,
) -> Block<'a> {
    if is_selected {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(symbols::border::ROUNDED)
            .border_style(Style::default().fg(ratatui::style::Color::Green));
        if author_group.starts_group() {
            header.insert(0, "─ ".into());
            block.title(header)
        } else {
            block.title_bottom(Line::from(vec!["─ ".into(), timestamp, " ".into()]))
        }
    } else if author_group.starts_group() {
        Block::default().title(header)
    } else {
        Block::default()
    }
}

fn message_content_area(area: Rect, is_selected: bool) -> Rect {
    if is_selected {
        Rect::new(
            area.x.saturating_add(1),
            area.y,
            area.width.saturating_sub(1),
            area.height,
        )
    } else {
        area
    }
}

pub fn reaction_chips(reactions: Option<&HashMap<wr::JID, Arc<str>>>) -> Vec<String> {
    let mut counts = BTreeMap::new();
    for reaction in reactions
        .into_iter()
        .flat_map(|reactions| reactions.values())
    {
        *counts.entry(reaction).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(emoji, count)| format!("[{emoji} {count}]"))
        .collect()
}

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

const AUDIO_WIDGET_BARS: usize = 16;

fn audio_widget_line(seed: &str, duration: Option<u64>) -> Line<'static> {
    let mut spans = vec![Span::styled(
        "|> ",
        Style::default().fg(Color::Green).bold(),
    )];
    if let Some(seconds) = duration {
        spans.push(Span::styled(
            format!("{} ", format_duration(seconds)),
            Style::default().fg(Color::DarkGray),
        ));
    }
    spans.push(Span::raw(waveform_bars(seed, AUDIO_WIDGET_BARS)));
    Line::from(spans)
}

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

fn waveform_bars(seed: &str, len: usize) -> String {
    let mut state: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.as_bytes() {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(0x100_0000_01b3);
    }
    let mut next = move || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut waveform = String::with_capacity(len);
    let mut level = 3 + (next() as usize % 3);
    for _ in 0..len {
        let step = (next() % 3) as i32 - 1;
        level = (level as i32 + step).clamp(0, 7) as usize;
        waveform.push(BARS[level]);
    }
    waveform
}

pub(crate) fn media_paragraph(
    status: String,
    is_audio: bool,
    audio_seed: &str,
    audio_duration: Option<u64>,
    alignment: ratatui::layout::Alignment,
) -> Paragraph<'static> {
    let mut lines = vec![Line::from(status)];
    if is_audio {
        lines.push(audio_widget_line(audio_seed, audio_duration));
    }
    Paragraph::new(lines).alignment(alignment)
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
            let lines = inline_content_lines(text, status, content_area.width as usize);
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

    let item_ids = items
        .iter()
        .map(|message| message.info.id.clone())
        .collect::<Vec<_>>();
    app.message_height_cache.retain_messages(&item_ids);

    if items.is_empty() {
        app.message_list_state.select(None);
        return Some(());
    }

    if app.message_list_state.selected.is_none()
        && app.message_list_state.selected_message.is_some()
    {
        let selected_message = app.message_list_state.selected_message.clone().unwrap();
        if let Some(idx) = items
            .iter()
            .position(|item| item.info.id == selected_message)
        {
            app.message_list_state.select(Some(idx));
        } else {
            app.message_list_state.select(None);
        }
    }

    if let Some(idx) = app.message_list_state.selected
        && idx >= items.len()
    {
        app.message_list_state.selected = items.len().checked_sub(1);
    }

    let width = list_area.width as isize;
    let padding = 4;
    let divider_after = unread_count.checked_sub(1);

    let previous_offset = app.message_list_state.offset;
    let mut previous_anchor = app
        .message_list_state
        .viewport_anchor
        .clone()
        .filter(|anchor| {
            anchor.width == width as usize
                && anchor.offset == app.message_list_state.offset
                && anchor.generation == app.message_height_cache.generation()
                && app
                    .message_list_state
                    .selected
                    .is_none_or(|selected| selected >= anchor.index)
                && items
                    .get(anchor.index)
                    .is_some_and(|item| item.info.id == anchor.message_id)
        });
    if let Some(anchor) = previous_anchor.as_mut() {
        let new_bottom = list_area.bottom();
        let bottom_delta = new_bottom as isize - anchor.bottom as isize;
        anchor.y = anchor.y.saturating_add(bottom_delta);
        anchor.bottom = new_bottom;
    }

    app.message_list_state.selected_message = app
        .message_list_state
        .selected
        .map(|selected| items[selected].info.id.clone());

    if app.message_list_state.selected.is_some() && app.message_list_state.update_selected {
        let selected = app.message_list_state.selected.unwrap();
        app.message_list_state.update_selected = false;

        // Use the previous viewport as an anchor while selection moves locally. A
        // cold jump still falls back to the exact prefix calculation below.
        let acc_height = previous_anchor
            .as_ref()
            .filter(|anchor| anchor.index <= selected)
            .map(|anchor| {
                let mut cursor = anchor.y;
                for index in anchor.index..selected {
                    cursor -= message_height(
                        &items[index],
                        width as usize,
                        app.message_list_state.selected == Some(index),
                        author_groups[index],
                        app,
                    ) as isize;
                    cursor -= spacing_after_message(
                        index,
                        &author_groups,
                        app.message_list_state.selected,
                    ) as isize;
                    if divider_after == Some(index) {
                        cursor -= 1;
                    }
                }
                (list_area.bottom() as isize + app.message_list_state.offset as isize - cursor)
                    as usize
            })
            .unwrap_or_else(|| {
                items
                    .iter()
                    .take(selected)
                    .enumerate()
                    .map(|(index, item)| {
                        usize::from(divider_after == Some(index))
                            + message_height(
                                item,
                                width as usize,
                                app.message_list_state.selected == Some(index),
                                author_groups[index],
                                app,
                            )
                            + spacing_after_message(
                                index,
                                &author_groups,
                                app.message_list_state.selected,
                            )
                    })
                    .sum::<usize>()
            });

        let selected_height = message_height(
            &items[selected],
            width as usize,
            true,
            author_groups[selected],
            app,
        );

        let low = acc_height < app.message_list_state.offset + padding;
        let high = acc_height + selected_height
            > app
                .message_list_state
                .offset
                .saturating_add((list_area.height as usize).saturating_sub(padding));

        // if low && high {
        //     info!("idk");
        // } else if low {
        if low {
            app.message_list_state.offset = acc_height.saturating_sub(padding);
        } else if high {
            app.message_list_state.offset =
                (acc_height + selected_height + padding).saturating_sub(list_area.height as usize);
        }
        if app.message_list_state.offset != previous_offset {
            if let Some(anchor) = previous_anchor.as_mut() {
                let delta = if app.message_list_state.offset >= previous_offset {
                    app.message_list_state.offset - previous_offset
                } else {
                    previous_offset - app.message_list_state.offset
                };
                let delta = delta.min(isize::MAX as usize) as isize;
                anchor.y = if app.message_list_state.offset >= previous_offset {
                    anchor.y.saturating_add(delta)
                } else {
                    anchor.y.saturating_sub(delta)
                };
                anchor.offset = app.message_list_state.offset;
            }
        }
    }

    let (start_index, y) = previous_anchor
        .map(|anchor| (anchor.index, anchor.y))
        .unwrap_or((
            0,
            list_area.bottom() as isize + app.message_list_state.offset as isize,
        ));
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

fn unread_divider_line(width: usize) -> Line<'static> {
    const TAG: &str = " New ";
    let style = Style::default().fg(Color::Rgb(237, 66, 69));
    if width <= 7 {
        return Line::styled("─".repeat(width), style);
    }
    Line::from(vec![
        Span::styled("─".repeat(width.saturating_sub(TAG.len())), style),
        Span::styled(TAG, style.bold()),
    ])
}
