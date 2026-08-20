use std::{ops::Range, sync::Arc};

use chrono::{DateTime, Local};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use whatsrust as wr;

pub const AUTHOR_GROUP_MAX_GAP: i64 = 5 * 60;
const REPLY_EXCERPT_MAX_CHARS: usize = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorGroupContext {
    starts_group: bool,
}

impl AuthorGroupContext {
    pub const STARTS_GROUP: Self = Self { starts_group: true };
    pub const CONTINUATION: Self = Self {
        starts_group: false,
    };

    pub const fn starts_group(self) -> bool {
        self.starts_group
    }

    pub(crate) const fn new(starts_group: bool) -> Self {
        Self { starts_group }
    }
}

pub fn starts_author_group(previous: Option<&wr::Message>, current: &wr::Message) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    let same_author = if previous.info.is_from_me || current.info.is_from_me {
        previous.info.is_from_me && current.info.is_from_me
    } else {
        previous.info.sender == current.info.sender
    };
    let Some(previous_time) = DateTime::<chrono::Utc>::from_timestamp(previous.info.timestamp, 0)
        .map(DateTime::<Local>::from)
    else {
        return true;
    };
    let Some(current_time) = DateTime::<chrono::Utc>::from_timestamp(current.info.timestamp, 0)
        .map(DateTime::<Local>::from)
    else {
        return true;
    };
    let elapsed = current.info.timestamp - previous.info.timestamp;

    !same_author
        || previous_time.date_naive() != current_time.date_naive()
        || !(0..AUTHOR_GROUP_MAX_GAP).contains(&elapsed)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ForwardingLabel {
    Forwarded,
    ForwardedManyTimes,
}

impl ForwardingLabel {
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Forwarded => "Forwarded",
            Self::ForwardedManyTimes => "Forwarded many times",
        }
    }
}

pub(crate) fn forwarding_label(message: &wr::Message) -> Option<ForwardingLabel> {
    message.info.forwarding.is_forwarded.then(|| {
        if message.info.forwarding.score >= 5 {
            ForwardingLabel::ForwardedManyTimes
        } else {
            ForwardingLabel::Forwarded
        }
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StatusLabel {
    Edited,
    Deleted,
}

impl StatusLabel {
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Edited => "(edited)",
            Self::Deleted => "(deleted)",
        }
    }
}

pub(crate) fn inline_content_lines(
    body: &str,
    mention_ranges: &[Range<usize>],
    status: Option<StatusLabel>,
    width: usize,
) -> Vec<Line<'static>> {
    if matches!(status, Some(StatusLabel::Deleted)) {
        return textwrap::wrap(crate::app::DELETED_MESSAGE_TEXT, width.max(1))
            .into_iter()
            .map(|line| Line::styled(line.into_owned(), Style::default().dark_gray()))
            .collect();
    }
    let status_text = status.map(StatusLabel::text);
    let content = match (body, status_text) {
        ("", Some(status)) => status.to_owned(),
        (_, Some(status)) => format!("{body} {status}"),
        (_, None) => body.to_owned(),
    };
    if content.is_empty() {
        return Vec::new();
    }
    let wrapped = textwrap::wrap(&content, width.max(1))
        .into_iter()
        .map(|line| line.into_owned())
        .collect::<Vec<_>>();
    let mut status_starts = vec![None; wrapped.len()];
    if let Some(status) = status_text {
        let mut remaining = status.graphemes(true).collect::<Vec<_>>();
        for (index, line) in wrapped.iter().enumerate().rev() {
            let graphemes = line.graphemes(true).collect::<Vec<_>>();
            if remaining.ends_with(&graphemes) {
                status_starts[index] = Some(0);
                remaining.truncate(remaining.len().saturating_sub(graphemes.len()));
            } else if let Some(start) =
                (0..graphemes.len()).find(|start| remaining.starts_with(&graphemes[*start..]))
            {
                status_starts[index] = Some(start);
                break;
            }
        }
    }
    let line_starts = wrapped
        .iter()
        .scan(0, |cursor, line| {
            let start = content[*cursor..]
                .find(line)
                .map_or(*cursor, |offset| *cursor + offset);
            *cursor = start + line.len();
            Some(start)
        })
        .collect::<Vec<_>>();
    wrapped
        .iter()
        .zip(status_starts)
        .zip(line_starts)
        .map(|((line, status_start), line_start)| {
            let mut byte_offset = 0;
            let graphemes = line
                .graphemes(true)
                .enumerate()
                .map(|(index, grapheme)| {
                    let start = line_start + byte_offset;
                    byte_offset += grapheme.len();
                    (
                        grapheme,
                        status_start.is_some_and(|status_start| index >= status_start),
                        mention_ranges
                            .iter()
                            .any(|range| start < range.end && start + grapheme.len() > range.start),
                    )
                })
                .collect::<Vec<_>>();
            inline_line(&graphemes)
        })
        .collect()
}

fn inline_line(graphemes: &[(&str, bool, bool)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (grapheme, is_status, is_mention) in graphemes {
        let style = if *is_status {
            Style::default().dark_gray()
        } else if *is_mention {
            Style::default().fg(ratatui::style::Color::Blue).bold()
        } else {
            Style::default()
        };
        match spans.last_mut() {
            Some(span) if span.style == style => span.content.to_mut().push_str(grapheme),
            _ => spans.push(Span::styled((*grapheme).to_owned(), style)),
        }
    }
    Line::from(spans)
}

pub(crate) fn forwarding_indicator_lines(label: Option<ForwardingLabel>, width: usize) -> usize {
    label.map_or(0, |label| textwrap::wrap(label.text(), width.max(1)).len())
}

pub fn reply_summary(message: &wr::Message, author: &str) -> String {
    format!("> {author}: {}", reply_excerpt(&get_quoted_text(message)))
}

fn reply_excerpt(text: &str) -> String {
    let normalized = text.replace(['\n', '\r'], " ");
    let mut characters = normalized.chars();
    let excerpt = characters
        .by_ref()
        .take(REPLY_EXCERPT_MAX_CHARS)
        .collect::<String>();

    if characters.next().is_some() {
        format!("{excerpt}…")
    } else {
        excerpt
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        style::{Color, Modifier},
        text::Span,
    };

    use super::inline_content_lines;

    #[test]
    fn semantic_mentions_stay_blue_and_bold_when_wrapped_without_styling_other_tokens() {
        let lines = inline_content_lines("@阿丽 and @999", &[0..7], None, 8);
        assert_eq!(lines.len(), 2);
        let spans = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .collect::<Vec<&Span>>();
        assert!(spans.iter().any(|span| span.content == "@阿丽"
            && span.style.fg == Some(Color::Blue)
            && span.style.add_modifier.contains(Modifier::BOLD)));
        assert!(
            spans
                .iter()
                .any(|span| span.content.contains("@999") && span.style.fg != Some(Color::Blue))
        );
    }
}

pub fn get_quoted_text(msg: &wr::Message) -> Arc<str> {
    match &msg.message {
        wr::MessageContent::Text(text) => text.clone(),
        wr::MessageContent::File(data) => {
            format!("{}: {}", data.path, data.caption.as_deref().unwrap_or("")).into()
        }
    }
}
