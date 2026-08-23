use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use whatsrust as wr;

use super::message_helpers::{AuthorGroupContext, directionally_ordered_spans};

const AUTHOR_PALETTE: &[Color] = &[
    Color::Rgb(0xE7, 0x9F, 0x3C),
    Color::Rgb(0x6F, 0xC9, 0xCE),
    Color::Rgb(0xC9, 0x8F, 0xE7),
    Color::Rgb(0x8F, 0xC9, 0x4F),
    Color::Rgb(0xE7, 0x6F, 0x8F),
    Color::Rgb(0x6F, 0x8F, 0xE7),
    Color::Rgb(0xE7, 0x4F, 0x4F),
    Color::Rgb(0x4F, 0xC9, 0x8F),
    Color::Rgb(0xC9, 0x8F, 0x4F),
    Color::Rgb(0x8F, 0x4F, 0xC9),
    Color::Rgb(0x4F, 0x8F, 0xC9),
    Color::Rgb(0xC9, 0x4F, 0x8F),
];

pub(super) fn author_color(sender: &wr::JID) -> Color {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in sender.0.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    AUTHOR_PALETTE[(hash as usize) % AUTHOR_PALETTE.len()]
}

pub(super) fn message_block<'a>(
    mut header: Vec<Span<'a>>,
    timestamp: Span<'a>,
    is_selected: bool,
    author_group: AuthorGroupContext,
) -> Block<'a> {
    if is_selected {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(symbols::border::ROUNDED)
            .border_style(Style::default().fg(Color::Green));
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

pub(super) fn message_content_area(area: Rect, is_selected: bool) -> Rect {
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

fn reaction_counts(reactions: Option<&HashMap<wr::JID, Arc<str>>>) -> BTreeMap<&Arc<str>, usize> {
    let mut counts = BTreeMap::new();
    for reaction in reactions
        .into_iter()
        .flat_map(|reactions| reactions.values())
    {
        *counts.entry(reaction).or_insert(0) += 1;
    }
    counts
}

pub fn reaction_chips(reactions: Option<&HashMap<wr::JID, Arc<str>>>) -> Vec<String> {
    reaction_counts(reactions)
        .into_iter()
        .map(|(emoji, count)| format!("[{emoji} {count}]"))
        .collect()
}

pub(crate) fn reaction_line(
    reactions: Option<&HashMap<wr::JID, Arc<str>>>,
    alignment: Alignment,
) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, (emoji, count)) in reaction_counts(reactions).into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::raw("["));
        spans.extend(directionally_ordered_spans(emoji, Style::default()));
        spans.push(Span::raw(format!(" {count}]")));
    }
    Line::from(spans).alignment(alignment)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaStatus {
    Pending,
    Downloaded,
    Downloading,
    DownloadFailed,
    LoadFailed,
    Loading,
}

fn media_status_line(path: &str, status: MediaStatus) -> Line<'static> {
    let mut spans = vec![Span::raw("🔗 ")];
    match status {
        MediaStatus::Pending => {
            spans.extend(directionally_ordered_spans(path, Style::default()));
            spans.push(Span::raw(" +"));
        }
        MediaStatus::Downloaded => {
            spans.extend(directionally_ordered_spans(path, Style::default()));
            spans.push(Span::raw(" ✓"));
        }
        MediaStatus::Downloading => {
            spans.extend(directionally_ordered_spans(path, Style::default()));
            spans.push(Span::raw(" downloading"));
        }
        MediaStatus::DownloadFailed => {
            spans.push(Span::raw("Failed to download "));
            spans.extend(directionally_ordered_spans(path, Style::default()));
        }
        MediaStatus::LoadFailed => {
            spans.push(Span::raw("Failed to load "));
            spans.extend(directionally_ordered_spans(path, Style::default()));
        }
        MediaStatus::Loading => {
            spans.extend(directionally_ordered_spans(path, Style::default()));
            spans.push(Span::raw(" loading"));
        }
    }
    Line::from(spans)
}

pub(crate) fn media_paragraph(
    path: &str,
    status: MediaStatus,
    is_audio: bool,
    audio_seed: &str,
    audio_duration: Option<u64>,
    alignment: Alignment,
) -> Paragraph<'static> {
    let mut lines = vec![media_status_line(path, status)];
    if is_audio {
        lines.push(audio_widget_line(audio_seed, audio_duration));
    }
    Paragraph::new(lines).alignment(alignment)
}

#[cfg(test)]
mod tests {
    use super::{MediaStatus, media_status_line, reaction_line};
    use ratatui::layout::Alignment;
    use std::collections::HashMap;
    use std::sync::Arc;
    use whatsrust as wr;

    #[test]
    fn media_status_keeps_icons_and_tokens_structured_around_visual_path() {
        let line = media_status_line("abc אבג", MediaStatus::Downloaded);
        let contents = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(contents, ["🔗 ", "abc גבא", " ✓"]);
    }

    #[test]
    fn reaction_line_keeps_emoji_and_counts_atomic() {
        let mut reactions = HashMap::new();
        reactions.insert(
            wr::JID::from("alice@example.test".to_owned()),
            Arc::from("👍"),
        );
        reactions.insert(
            wr::JID::from("bob@example.test".to_owned()),
            Arc::from("👍"),
        );
        reactions.insert(
            wr::JID::from("carol@example.test".to_owned()),
            Arc::from("👩‍💻"),
        );

        let line = reaction_line(Some(&reactions), Alignment::Right);
        let contents = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(contents, ["[", "👍", " 2]", " ", "[", "👩‍💻", " 1]"]);
        assert_eq!(line.alignment, Some(Alignment::Right));
    }
}

pub(super) fn unread_divider_line(width: usize) -> Line<'static> {
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
