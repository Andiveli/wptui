use std::{
    cmp::{max, min},
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use chrono::{DateTime, Datelike, Local};
use log::trace;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget, Wrap},
};
use ratatui_image::StatefulImage;
use whatsrust::{self as wr, FileKind};

use crate::app::events::{AppEvent, AppInput};
use crate::app::{App, FileMeta, Metadata};

#[path = "message_helpers.rs"]
mod message_helpers;
#[path = "message_layout.rs"]
mod message_layout;

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

/// Number of bars in the static audio waveform.
const AUDIO_WIDGET_BARS: usize = 16;

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

fn file_content_height(_id: &wr::MessageId, file: &wr::FileContent, _app: &mut App) -> usize {
    match file.kind {
        // Media previews occupy their final block from the first render. The
        // preview lifecycle changes only the contents of this block, never its
        // geometry.
        FileKind::Image | FileKind::Sticker | FileKind::Video => preview_height(&file.kind),
        // Audio renders a static play bar under the file line, so it takes
        // two rows. Must stay in sync with `message_height`.
        FileKind::Audio => 2,
        FileKind::Document => 1,
    }
}

/// Height (in terminal rows) of an inline media preview once loaded.
/// Video is rendered taller than images/stickers so it doesn't look tiny.
pub fn preview_height(kind: &FileKind) -> usize {
    match kind {
        FileKind::Video => VIDEO_HEIGHT,
        _ => IMAGE_HEIGHT,
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

/// Visual-only waveform for audio messages. Deliberately static: a functional
/// progress bar would need per-frame re-rendering and playback position
/// tracking, which is out of scope.
///
/// The pattern is deterministic per message (seeded from the file path), so
/// each audio message keeps its own waveform across re-renders. `duration` is
/// the probed length in seconds, shown between the play marker and the wave.
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

/// Formats whole seconds as `m:ss` (or `h:mm:ss` for long recordings).
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

/// Generates `len` waveform bars (▁..█) via a deterministic random walk seeded
/// from `seed`, so the same audio always renders the same pattern.
fn waveform_bars(seed: &str, len: usize) -> String {
    // FNV-1a over the seed bytes (same approach as `author_color`).
    let mut state: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.as_bytes() {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(0x100_0000_01b3);
    }
    // xorshift64* PRNG for a stable pseudo-random walk.
    let mut next = move || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut waveform = String::with_capacity(len);
    let mut level = 3 + (next() as usize % 3); // start mid-range
    for _ in 0..len {
        let step = (next() % 3) as i32 - 1;
        level = (level as i32 + step).clamp(0, 7) as usize;
        waveform.push(BARS[level]);
    }
    waveform
}

/// Builds the media line(s) for a file message. Audio messages append a
/// static waveform (with probed duration when available) under the file line.
fn media_paragraph(
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
            let content_height = file_content_height(&message.info.id, data, app);
            let is_audio = matches!(data.kind, FileKind::Audio);
            let audio_duration = if is_audio {
                app.audio_durations.get(data.path.as_ref()).copied()
            } else {
                None
            };

            let [media_area, caption_area] = Layout::vertical([
                Constraint::Length(content_height as u16),
                Constraint::Min(0),
            ])
            .areas(content_area);

            match app.metadata.get(&message.info.id) {
                None => {
                    media_paragraph(
                        format!("🔗 {} +", data.path),
                        is_audio,
                        data.path.as_ref(),
                        audio_duration,
                        alignment,
                    )
                    .render(media_area, buf);
                    app.tx
                        .send(AppInput::App(AppEvent::DownloadFile(
                            message.info.id.clone(),
                            data.file_id.clone(),
                        )))
                        .unwrap();
                }
                Some(Metadata::File(meta)) => match meta {
                    FileMeta::Downloaded => {
                        media_paragraph(
                            format!("🔗 {} ✓", data.path),
                            is_audio,
                            data.path.as_ref(),
                            audio_duration,
                            alignment,
                        )
                        .render(media_area, buf);

                        if let FileKind::Image | FileKind::Sticker | FileKind::Video = data.kind {
                            let already_loading = matches!(
                                app.metadata.get(&message.info.id),
                                Some(Metadata::File(FileMeta::Loading))
                            );
                            if !already_loading {
                                app.tx
                                    .send(AppInput::App(AppEvent::LoadFilePreview(
                                        message.info.id.clone(),
                                    )))
                                    .unwrap();
                            }
                        }
                    }
                    FileMeta::Downloading => {
                        media_paragraph(
                            format!("🔗 {} downloading", data.path),
                            is_audio,
                            data.path.as_ref(),
                            audio_duration,
                            alignment,
                        )
                        .render(media_area, buf);
                    }
                    FileMeta::DownloadFailed => {
                        media_paragraph(
                            format!("🔗 Failed to download {}", data.path),
                            is_audio,
                            data.path.as_ref(),
                            audio_duration,
                            alignment,
                        )
                        .render(media_area, buf);
                    }
                    FileMeta::LoadFailed => {
                        media_paragraph(
                            format!("🔗 Failed to load {}", data.path),
                            is_audio,
                            data.path.as_ref(),
                            audio_duration,
                            alignment,
                        )
                        .render(media_area, buf);
                    }
                    FileMeta::Loading => {
                        trace!("Rendering loading for {}", &message.info.id);
                        media_paragraph(
                            format!("🔗 {} loading", data.path),
                            is_audio,
                            data.path.as_ref(),
                            audio_duration,
                            alignment,
                        )
                        .render(media_area, buf);
                    }
                    FileMeta::Loaded => match data.kind {
                        FileKind::Image | FileKind::Sticker | FileKind::Video => {
                            let placeholder = match data.kind {
                                FileKind::Video => "🎬",
                                _ => "🖼",
                            };
                            if !render_image || app.image_cache.get_mut(&data.path).is_none() {
                                Paragraph::new(placeholder)
                                    .alignment(alignment)
                                    .render(media_area, buf);
                            } else {
                                app.touch_image_cache(&data.path);
                                if let Some(image) = app.image_cache.get_mut(&data.path) {
                                    StatefulImage::default().render(media_area, buf, image);
                                } else {
                                    Paragraph::new(placeholder)
                                        .alignment(alignment)
                                        .render(media_area, buf);
                                }
                            }
                        }
                        FileKind::Audio | FileKind::Document => {
                            media_paragraph(
                                format!("🔗 {} ✓", data.path),
                                is_audio,
                                data.path.as_ref(),
                                audio_duration,
                                alignment,
                            )
                            .render(media_area, buf);
                        }
                    },
                },
            };

            if data.caption.is_some() || status.is_some() {
                let lines = inline_content_lines(
                    data.caption.as_deref().unwrap_or_default(),
                    status,
                    content_area.width as usize,
                );
                Paragraph::new(lines)
                    .alignment(alignment)
                    .render(caption_area, buf);
            }
        }
    };
    if !reactions_area.is_empty() {
        Paragraph::new(reaction_chips(app.reactions.get(&message.info.id)).join(" "))
            .alignment(alignment)
            .render(reactions_area, buf);
    }
}

pub fn render_messages(frame: &mut Frame, app: &mut App, area: Rect) -> Option<()> {
    let chat_jid = app.open_chat()?;

    let list_area = area;
    if list_area.is_empty() {
        return Some(());
    }

    let items: Vec<_> = app
        .chat_messages
        .get(&chat_jid)?
        .iter()
        .rev()
        .filter_map(|msg_id| app.messages.get(msg_id).cloned())
        .collect();
    render_message_items(frame, app, area, items)
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
    render_message_items(frame, app, area, items);
}

/// Shared core of chat and status message rendering. The caller supplies
/// the messages newest-first (as rendered top-to-bottom).
fn render_message_items(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    items: Vec<wr::Message>,
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

    app.message_list_state.selected_message = app
        .message_list_state
        .selected
        .map(|selected| items[selected].info.id.clone());

    if app.message_list_state.selected.is_some() && app.message_list_state.update_selected {
        let selected = app.message_list_state.selected.unwrap();
        app.message_list_state.update_selected = false;

        // Height to the bottom of selected msg
        let acc_height = items
            .iter()
            .take(selected)
            .enumerate()
            .map(|(index, item)| {
                message_height(
                    item,
                    width as usize,
                    app.message_list_state.selected == Some(index),
                    author_groups[index],
                    app,
                ) + spacing_after_message(index, &author_groups, app.message_list_state.selected)
            })
            .sum::<usize>();

        let selected_height = message_height(
            &items[selected],
            width as usize,
            true,
            author_groups[selected],
            app,
        );

        let low = acc_height < app.message_list_state.offset + padding;
        let high = acc_height + selected_height
            > app.message_list_state.offset + list_area.height as usize - padding;

        // if low && high {
        //     info!("idk");
        // } else if low {
        if low {
            app.message_list_state.offset = acc_height.saturating_sub(padding);
        } else if high {
            app.message_list_state.offset =
                (acc_height + selected_height + padding).saturating_sub(list_area.height as usize);
        }
    }

    let mut y = list_area.bottom() as isize + app.message_list_state.offset as isize;
    for (i, item) in items.iter().enumerate() {
        let is_selected = app.message_list_state.selected == Some(i);
        let author_group = author_groups[i];
        let height = message_height(item, width as usize, is_selected, author_group, app) as isize;

        let bottom = y;
        let top = y - height;

        if bottom < list_area.top() as isize {
            break;
        }

        if top <= list_area.bottom() as isize {
            let too_low = top < list_area.top() as isize;
            let too_high = bottom > list_area.bottom() as isize;

            if too_low || too_high {
                let item_area = Rect::new(0, 0, width as u16, height as u16);
                let mut buf = Buffer::empty(item_area);

                let available_top = max(top, list_area.top() as isize) as u16;
                let available_bottom = min(bottom, list_area.bottom() as isize) as u16;
                let visible_buf_top = (available_top as isize - top) as u16;
                let visible_buf_height = available_bottom - available_top;

                // -- BEGIN AI IMPRESSIVE HACK --
                // Only render the image (and thus touch protocol state) when at least one image
                // row is in the visible slice. Otherwise we'd set "transmitted" but never send
                // any cell to the frame, and the image would never show when scrolled into view.
                let render_image = match &item.message {
                    wr::MessageContent::File(data)
                        if matches!(
                            app.metadata.get(&item.info.id),
                            Some(Metadata::File(FileMeta::Loaded))
                        ) && matches!(
                            data.kind,
                            FileKind::Image | FileKind::Sticker | FileKind::Video
                        ) =>
                    {
                        let image_top = u16::from(is_selected || author_group.starts_group())
                            + u16::from(item.info.quote_id.is_some());
                        let image_bottom = image_top + preview_height(&data.kind) as u16;
                        let visible_buf_bottom = visible_buf_top + visible_buf_height;
                        visible_buf_top < image_bottom && visible_buf_bottom > image_top
                    }
                    _ => true,
                };
                // -- END AI IMPRESSIVE HACK --

                render_message(
                    &mut buf,
                    item,
                    is_selected,
                    author_group,
                    app,
                    item_area,
                    render_image,
                );

                let buf_area = Rect::new(
                    list_area.left(),
                    available_top,
                    width as u16,
                    visible_buf_height,
                );

                if !buf_area.is_empty() {
                    let mut mapped_area = buf_area;
                    mapped_area.y = visible_buf_top;
                    mapped_area.x = 0;

                    // -- BEGIN AI IMPRESSIVE HACK --
                    // When the visible slice doesn't include the image's first row, Kitty never
                    // receives the image transmit (it's in that first row's cell). Inject it into
                    // the first visible row's left cell so the image displays.
                    let (inject_transmit, media_first_row, media_first_col) = match &item.message {
                        wr::MessageContent::File(data)
                            if matches!(
                                app.metadata.get(&item.info.id),
                                Some(Metadata::File(FileMeta::Loaded))
                            ) && matches!(
                                data.kind,
                                FileKind::Image | FileKind::Sticker | FileKind::Video
                            ) =>
                        {
                            let first_row = u16::from(is_selected || author_group.starts_group())
                                + u16::from(item.info.quote_id.is_some());
                            let inject = mapped_area.y > first_row
                                && mapped_area.y < first_row + preview_height(&data.kind) as u16;
                            (inject, first_row, if is_selected { 2 } else { 0 })
                        }
                        _ => (false, 0, 0),
                    };

                    for (row_idx, (screen_row, msg_row)) in
                        buf_area.rows().zip(mapped_area.rows()).enumerate()
                    {
                        for (screen_col, msg_col) in screen_row.columns().zip(msg_row.columns()) {
                            let mut cell = buf[msg_col].clone();
                            if inject_transmit
                                && row_idx == 0
                                && screen_col.x == list_area.left() + media_first_col
                            {
                                let first_sym = buf[(media_first_col, media_first_row)].symbol();
                                if let Some(pos) = first_sym.find("\x1b[s") {
                                    let merged = format!("{}{}", &first_sym[..pos], cell.symbol());
                                    cell.set_symbol(&merged);
                                }
                            }
                            frame.buffer_mut()[screen_col] = cell;
                        }
                    }
                    // -- END AI IMPRESSIVE HACK --
                }
            } else {
                let item_area = Rect {
                    x: list_area.left(),
                    y: top as u16,
                    width: width as u16,
                    height: height as u16,
                };

                render_message(
                    frame.buffer_mut(),
                    item,
                    is_selected,
                    author_group,
                    app,
                    item_area,
                    true,
                );
            }
        }

        y -= height
            + spacing_after_message(i, &author_groups, app.message_list_state.selected) as isize;
    }

    None
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash)]
pub struct MessageListState {
    pub selected: Option<usize>,
    pub offset: usize,
    selected_message: Option<wr::MessageId>,
    pub update_selected: bool,
}

impl MessageListState {
    pub fn get_selected_message(&self) -> Option<wr::MessageId> {
        self.selected_message.clone()
    }
    pub fn set_selected_message(&mut self, msg_id: wr::MessageId) {
        self.selected_message = Some(msg_id);
        self.selected = None;
        self.update_selected = false;
    }
}

impl MessageListState {
    pub fn reset(&mut self) {
        self.selected = None;
        self.offset = 0;
        self.selected_message = None;
        self.update_selected = false;
    }

    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index;
        if index.is_none() {
            self.offset = 0;
        } else {
            self.update_selected = true;
        }
    }
    pub fn select_next(&mut self) {
        let next = self.selected.map_or(0, |i| i.saturating_add(1));
        self.select(Some(next));
    }

    pub fn select_previous(&mut self) {
        let previous = self.selected.map_or(usize::MAX, |i| i.saturating_sub(1));
        self.select(Some(previous));
    }

    pub fn select_first(&mut self) {
        self.select(Some(0));
    }

    pub fn select_last(&mut self) {
        self.select(Some(usize::MAX));
    }

    pub fn scroll_down_by(&mut self, amount: u16) {
        let selected = self.selected.unwrap_or_default();
        self.select(Some(selected.saturating_add(amount as usize)));
    }

    pub fn scroll_up_by(&mut self, amount: u16) {
        let selected = self.selected.unwrap_or_default();
        self.select(Some(selected.saturating_sub(amount as usize)));
    }

    pub fn select_next_bounded(&mut self, item_count: usize) {
        if self.selected.is_none() {
            self.select_bounded(item_count, 0);
        } else {
            self.move_by(item_count, 1);
        }
    }

    pub fn select_previous_bounded(&mut self, item_count: usize) {
        self.move_by(item_count, -1);
    }

    pub fn jump_top_bounded(&mut self, item_count: usize) {
        self.select_bounded(item_count, 0);
    }

    pub fn jump_bottom_bounded(&mut self, item_count: usize) {
        self.select_bounded(item_count, item_count.saturating_sub(1));
    }

    pub fn half_page_down_bounded(&mut self, item_count: usize, page_size: usize) {
        self.move_by(item_count, page_size as isize);
    }

    pub fn half_page_up_bounded(&mut self, item_count: usize, page_size: usize) {
        self.move_by(item_count, -(page_size as isize));
    }

    fn move_by(&mut self, item_count: usize, delta: isize) {
        let current = self.selected.unwrap_or_default();
        self.select_bounded(item_count, current.saturating_add_signed(delta));
    }

    fn select_bounded(&mut self, item_count: usize, index: usize) {
        if item_count == 0 {
            self.selected = None;
            self.offset = 0;
            self.selected_message = None;
            self.update_selected = false;
        } else {
            self.select(Some(index.min(item_count - 1)));
        }
    }
}
