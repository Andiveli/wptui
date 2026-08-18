use std::sync::Arc;

use chrono::{DateTime, Local};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use whatsrust as wr;

use super::bidi::visual_graphemes_with_range;

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

#[derive(Clone, Copy)]
enum TextOrder {
    Visual,
    Logical,
}

pub(crate) fn inline_content_lines(
    body: &str,
    status: Option<StatusLabel>,
    width: usize,
) -> Vec<Line<'static>> {
    inline_content_lines_with_order(body, status, width, TextOrder::Visual)
}

pub(crate) fn inline_content_lines_logical(
    body: &str,
    status: Option<StatusLabel>,
    width: usize,
) -> Vec<Line<'static>> {
    inline_content_lines_with_order(body, status, width, TextOrder::Logical)
}

fn inline_content_lines_with_order(
    body: &str,
    status: Option<StatusLabel>,
    width: usize,
    text_order: TextOrder,
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
    let mut status_ranges = vec![None; wrapped.len()];
    if let Some(status) = status_text {
        let mut remaining = status.graphemes(true).collect::<Vec<_>>();
        for (index, line) in wrapped.iter().enumerate().rev() {
            let graphemes = line.graphemes(true).collect::<Vec<_>>();
            if remaining.ends_with(&graphemes) {
                status_ranges[index] = Some((0, graphemes.len()));
                remaining.truncate(remaining.len().saturating_sub(graphemes.len()));
            } else if let Some(start) =
                (0..graphemes.len()).find(|start| graphemes[*start..].starts_with(&remaining))
            {
                status_ranges[index] = Some((start, start + remaining.len()));
                break;
            }
        }
    }
    wrapped
        .iter()
        .zip(status_ranges)
        .map(|(line, status_range)| {
            if matches!(text_order, TextOrder::Visual) {
                inline_line(&visual_graphemes_with_range(
                    line,
                    status_range.map(|(start, end)| start..end),
                ))
            } else {
                let graphemes = line
                    .graphemes(true)
                    .enumerate()
                    .map(|(index, grapheme)| {
                        (
                            grapheme.to_owned(),
                            status_range.is_some_and(|(start, end)| (start..end).contains(&index)),
                        )
                    })
                    .collect::<Vec<_>>();
                inline_line(&graphemes)
            }
        })
        .collect()
}

fn inline_line(graphemes: &[(String, bool)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (grapheme, is_status) in graphemes {
        let style = if *is_status {
            Style::default().dark_gray()
        } else {
            Style::default()
        };
        match spans.last_mut() {
            Some(span) if span.style == style => span.content.to_mut().push_str(grapheme),
            _ => spans.push(Span::styled(grapheme.clone(), style)),
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

pub fn get_quoted_text(msg: &wr::Message) -> Arc<str> {
    match &msg.message {
        wr::MessageContent::Text(text) => text.clone(),
        wr::MessageContent::File(data) => {
            format!("{}: {}", data.path, data.caption.as_deref().unwrap_or("")).into()
        }
    }
}
