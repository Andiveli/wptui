use std::{ops::Range, sync::Arc};

use chrono::{DateTime, Local};
use ratatui::{
    layout::Alignment,
    style::Style,
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use whatsrust as wr;

use crate::ui::bidi::{Direction, visual_graphemes_in_paragraph};

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

fn line_range(paragraph: &str, width: usize) -> Vec<Range<usize>> {
    if paragraph.is_empty() {
        return vec![0..0];
    }

    #[derive(Clone, Copy)]
    struct Word {
        start: usize,
        end: usize,
        width: usize,
        separator_width: usize,
    }

    let width = width.max(1);
    let mut words = Vec::new();
    let mut word_start = None;
    let mut word_end = 0;
    let mut word_width: usize = 0;
    let mut word_separator_width: usize = 0;
    let mut separator_width: usize = 0;
    let mut in_word = false;

    for (start, grapheme) in paragraph.grapheme_indices(true) {
        let grapheme_width = Line::from(grapheme).width();
        let is_whitespace = grapheme.chars().all(|character| character.is_whitespace());

        if is_whitespace {
            if in_word {
                separator_width = separator_width.saturating_add(grapheme_width);
                in_word = false;
            }
            continue;
        }

        if !in_word {
            if let Some(word_start) = word_start {
                words.push(Word {
                    start: word_start,
                    end: word_end,
                    width: word_width,
                    separator_width: word_separator_width,
                });
            }
            word_start = Some(start);
            word_separator_width = separator_width;
            separator_width = 0;
            word_width = 0;
            in_word = true;
        }

        word_width = word_width.saturating_add(grapheme_width);
        word_end = start + grapheme.len();
    }

    if let Some(word_start) = word_start {
        words.push(Word {
            start: word_start,
            end: word_end,
            width: word_width,
            separator_width: word_separator_width,
        });
    }

    let mut ranges = Vec::new();
    let mut line_start = 0;
    let mut line_end = 0;
    let mut line_width: usize = 0;
    let mut has_line = false;

    for word in words {
        if word.width <= width {
            let fits = has_line
                && line_width
                    .saturating_add(word.separator_width)
                    .saturating_add(word.width)
                    <= width;
            if !fits {
                if has_line {
                    ranges.push(line_start..line_end);
                }
                line_start = word.start;
                line_end = word.end;
                line_width = word.width;
                has_line = true;
            } else {
                line_end = word.end;
                line_width = line_width
                    .saturating_add(word.separator_width)
                    .saturating_add(word.width);
            }
            continue;
        }

        if has_line {
            ranges.push(line_start..line_end);
            has_line = false;
        }

        let mut chunk_start = word.start;
        let mut chunk_end = word.start;
        let mut chunk_width: usize = 0;
        for (relative_start, grapheme) in paragraph[word.start..word.end].grapheme_indices(true) {
            let start = word.start + relative_start;
            let end = start + grapheme.len();
            let grapheme_width = Line::from(grapheme).width();

            if chunk_width > 0 && chunk_width.saturating_add(grapheme_width) > width {
                ranges.push(chunk_start..chunk_end);
                chunk_start = start;
                chunk_width = 0;
            }

            chunk_end = end;
            chunk_width = chunk_width.saturating_add(grapheme_width);
        }
        if chunk_start < chunk_end {
            ranges.push(chunk_start..chunk_end);
        }
    }

    if has_line {
        ranges.push(line_start..line_end);
    }

    ranges
}

fn local_range(
    range: &Range<usize>,
    paragraph_start: usize,
    paragraph_end: usize,
) -> Option<Range<usize>> {
    let start = range.start.max(paragraph_start).min(paragraph_end);
    let end = range.end.max(paragraph_start).min(paragraph_end);
    (start < end).then_some(start - paragraph_start..end - paragraph_start)
}

pub(crate) fn inline_content_lines(
    body: &str,
    mention_ranges: &[Range<usize>],
    status: Option<StatusLabel>,
    width: usize,
) -> Vec<Line<'static>> {
    let (content, status_range) = if matches!(status, Some(StatusLabel::Deleted)) {
        (
            crate::app::DELETED_MESSAGE_TEXT.to_owned(),
            Some(0..crate::app::DELETED_MESSAGE_TEXT.len()),
        )
    } else {
        let status_text = status.map(StatusLabel::text);
        let content = match (body, status_text) {
            ("", Some(status)) => status.to_owned(),
            (_, Some(status)) => format!("{body} {status}"),
            (_, None) => body.to_owned(),
        };
        let status_range = status_text.map(|status| {
            let start = if body.is_empty() { 0 } else { body.len() + 1 };
            start..start + status.len()
        });
        (content, status_range)
    };

    if content.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut paragraph_start = 0;
    for paragraph in content.split('\n') {
        let paragraph_end = paragraph_start + paragraph.len();
        let direction = Direction::from_text(paragraph);
        for range in line_range(paragraph, width) {
            let absolute_range = paragraph_start + range.start..paragraph_start + range.end;
            let status_for_line = status_range
                .as_ref()
                .and_then(|range| local_range(range, paragraph_start, paragraph_end));
            let mentions_for_line = mention_ranges
                .iter()
                .filter_map(|range| local_range(range, paragraph_start, paragraph_end))
                .collect::<Vec<_>>();
            let visual = visual_graphemes_in_paragraph(
                paragraph,
                range,
                status_for_line,
                &mentions_for_line,
            );
            let graphemes = if visual.is_empty() {
                paragraph
                    [absolute_range.start - paragraph_start..absolute_range.end - paragraph_start]
                    .graphemes(true)
                    .map(|grapheme| (grapheme.to_owned(), false, false))
                    .collect()
            } else {
                visual
            };
            lines.push(inline_line(&graphemes, direction));
        }
        paragraph_start = paragraph_end + 1;
    }
    lines
}

fn inline_line(graphemes: &[(String, bool, bool)], direction: Direction) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (grapheme, is_status, is_mention) in graphemes {
        let style = if *is_status {
            Style::default().dark_gray()
        } else if *is_mention {
            Style::default().fg(ratatui::style::Color::Blue).bold()
        } else {
            Style::default()
        };
        append_styled_grapheme(&mut spans, grapheme, style);
    }
    Line::from(spans).alignment(match direction {
        Direction::Ltr => Alignment::Left,
        Direction::Rtl => Alignment::Right,
    })
}

pub(crate) fn directionally_ordered_spans(text: &str, style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (index, paragraph) in text.split('\n').enumerate() {
        if index > 0 {
            append_styled_grapheme(&mut spans, "\n", style);
        }
        let visual = visual_graphemes_in_paragraph(paragraph, 0..paragraph.len(), None, &[]);
        for (grapheme, _, _) in visual {
            append_styled_grapheme(&mut spans, &grapheme, style);
        }
    }
    spans
}

fn append_styled_grapheme(spans: &mut Vec<Span<'static>>, grapheme: &str, style: Style) {
    match spans.last_mut() {
        Some(span) if span.style == style => span.content.to_mut().push_str(grapheme),
        _ => spans.push(Span::styled(grapheme.to_owned(), style)),
    }
}

pub(crate) fn forwarding_indicator_lines(label: Option<ForwardingLabel>, width: usize) -> usize {
    label.map_or(0, |label| {
        inline_content_lines(label.text(), &[], None, width).len()
    })
}

/// Limit text to the available terminal width without splitting a grapheme
/// cluster. A single ellipsis is used when even the first cluster cannot fit;
/// this keeps narrow quote chrome to one rendered row.
pub(crate) fn fit_text_to_width(text: &str, width: usize) -> String {
    let width = width.max(1);
    if Line::from(text).width() <= width {
        return text.to_owned();
    }

    let ellipsis = "…";
    let mut result = String::new();
    for grapheme in text.graphemes(true) {
        let next_width = Line::from(format!("{result}{grapheme}{ellipsis}")).width();
        if next_width > width {
            break;
        }
        result.push_str(grapheme);
    }
    if result.is_empty() {
        return ellipsis.to_owned();
    }
    result.push_str(ellipsis);
    result
}

pub fn reply_summary(message: &wr::Message, author: &str) -> String {
    format!("> {author}: {}", reply_excerpt(&get_quoted_text(message)))
}

/// Limit a quote summary to the available terminal width without splitting a
/// grapheme cluster. This keeps quote chrome to one rendered row, matching the
/// fixed quote row in message layout.
pub(crate) fn reply_summary_for_width(message: &wr::Message, author: &str, width: usize) -> String {
    fit_text_to_width(&reply_summary(message, author), width)
}

fn reply_excerpt(text: &str) -> String {
    let normalized = text.replace(['\n', '\r'], " ");
    let mut graphemes = normalized.graphemes(true);
    let excerpt = graphemes
        .by_ref()
        .take(REPLY_EXCERPT_MAX_CHARS)
        .collect::<String>();

    if graphemes.next().is_some() {
        format!("{excerpt}…")
    } else {
        excerpt
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        layout::Alignment,
        style::{Color, Modifier},
        text::Span,
    };

    use unicode_segmentation::UnicodeSegmentation;

    use super::inline_content_lines;

    #[test]
    fn word_wrapping_prefers_whitespace_boundaries() {
        assert_eq!(
            super::line_range("a message that wraps", 8),
            vec![0..1, 2..9, 10..14, 15..20]
        );
        let lines = inline_content_lines("a message that wraps", &[], None, 8);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(rendered, ["a", "message", "that", "wraps"]);
    }

    #[test]
    fn word_wrapping_splits_only_words_wider_than_the_available_width() {
        let lines = inline_content_lines("short superlongword", &[], None, 6);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(rendered, ["short", "superl", "ongwor", "d"]);
    }

    #[test]
    fn word_wrapping_trims_leading_trailing_and_repeated_whitespace() {
        let lines = inline_content_lines("  a   b  ", &[], None, 1);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(rendered, ["a", "b"]);
    }

    #[test]
    fn canonical_lines_align_each_paragraph_by_its_base_direction() {
        let lines = inline_content_lines("אבג\nabc", &[], None, 8);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].alignment, Some(Alignment::Right));
        assert_eq!(lines[1].alignment, Some(Alignment::Left));
    }

    #[test]
    fn canonical_planner_keeps_paragraph_context_across_soft_wraps() {
        let lines = inline_content_lines("123 אבג דהו", &[], None, 5);

        assert_eq!(lines.len(), 3);
        assert!(
            lines
                .iter()
                .all(|line| line.alignment == Some(Alignment::Right))
        );
    }

    #[test]
    fn canonical_pipeline_preserves_combining_and_emoji_zwj_graphemes() {
        let lines = inline_content_lines("אְב 👩‍💻", &[], None, 20);
        let rendered = lines[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(rendered.contains("אְ"));
        assert!(rendered.contains("👩‍💻"));
        assert_eq!(lines[0].alignment, Some(Alignment::Right));
    }

    #[test]
    fn width_one_preserves_combining_and_zwj_graphemes_without_panicking() {
        let lines = inline_content_lines("אְ 👩‍💻", &[], None, 1);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(rendered, ["אְ", "👩‍💻"]);
    }

    #[test]
    fn rtl_mentions_keep_their_style_after_visual_reordering() {
        let body = "@שלום";
        let lines = inline_content_lines(body, &[0..body.len()], None, 20);
        assert!(lines[0].spans.iter().all(|span| {
            span.style.fg == Some(Color::Blue) && span.style.add_modifier.contains(Modifier::BOLD)
        }));
    }

    #[test]
    fn quote_excerpt_truncates_by_grapheme_clusters() {
        let text = "👩‍💻".repeat(super::REPLY_EXCERPT_MAX_CHARS + 1);
        let excerpt = super::reply_excerpt(&text);

        assert!(excerpt.ends_with('…'));
        assert_eq!(
            excerpt.graphemes(true).count(),
            super::REPLY_EXCERPT_MAX_CHARS + 1
        );
    }

    #[test]
    fn narrow_quote_fallback_fits_one_cell_without_wrapping() {
        assert_eq!(super::fit_text_to_width("not found", 1), "…");
    }

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
        wr::MessageContent::ViewOnceUnavailable => wr::VIEW_ONCE_UNAVAILABLE_DESCRIPTION.into(),
        wr::MessageContent::Text(text) => text.clone(),
        wr::MessageContent::File(data) => {
            format!("{}: {}", data.path, data.caption.as_deref().unwrap_or("")).into()
        }
    }
}
