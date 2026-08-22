use std::ops::Range;

use unicode_bidi::{BidiClass, BidiInfo, Level};
use unicode_segmentation::UnicodeSegmentation;

/// Base direction selected from the first strong character in a paragraph.
/// Numbers, punctuation, and formatting controls do not establish direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Direction {
    Ltr,
    Rtl,
}

impl Direction {
    pub(crate) fn from_text(text: &str) -> Self {
        text.chars()
            .find_map(|character| match unicode_bidi::bidi_class(character) {
                BidiClass::L => Some(Self::Ltr),
                BidiClass::R | BidiClass::AL => Some(Self::Rtl),
                _ => None,
            })
            .unwrap_or(Self::Ltr)
    }

    fn level(self) -> Level {
        match self {
            Self::Ltr => Level::ltr(),
            Self::Rtl => Level::rtl(),
        }
    }

    pub(crate) fn alignment(self) -> ratatui::layout::Alignment {
        match self {
            Self::Ltr => ratatui::layout::Alignment::Left,
            Self::Rtl => ratatui::layout::Alignment::Right,
        }
    }
}

/// Convert one logical line to visual grapheme order using the bidi levels of
/// its complete logical paragraph. The line range is a byte range in
/// `paragraph`, and marks are byte ranges in that same paragraph.
///
/// Arabic joining forms, ligatures, glyph selection, and UAX #9 rule L4
/// mirroring are deliberately not performed here. Those require the terminal
/// renderer and its font shaping support; this app controls only logical
/// paragraph analysis and cell/run order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VisualGrapheme {
    pub(crate) text: String,
    pub(crate) source_range: Range<usize>,
    pub(crate) direction: Direction,
}

pub(crate) fn visual_graphemes_with_ranges(
    paragraph: &str,
    line_range: Range<usize>,
) -> Vec<VisualGrapheme> {
    visual_graphemes_with_base_direction(paragraph, line_range, Direction::from_text(paragraph))
}

pub(crate) fn visual_graphemes_with_base_direction(
    paragraph: &str,
    line_range: Range<usize>,
    base_direction: Direction,
) -> Vec<VisualGrapheme> {
    if line_range.is_empty() {
        return Vec::new();
    }

    let bidi_info = BidiInfo::new(paragraph, Some(base_direction.level()));
    let paragraph_info = &bidi_info.paragraphs[0];
    let (levels, runs) = bidi_info.visual_runs(paragraph_info, line_range.clone());
    let graphemes = paragraph[line_range.clone()]
        .grapheme_indices(true)
        .map(|(offset, grapheme)| VisualGrapheme {
            text: grapheme.to_owned(),
            source_range: line_range.start + offset..line_range.start + offset + grapheme.len(),
            direction: Direction::Ltr,
        })
        .collect::<Vec<_>>();
    let mut visual = Vec::new();

    for run in runs {
        let direction = if levels[run.start].is_rtl() {
            Direction::Rtl
        } else {
            Direction::Ltr
        };
        let mut run_graphemes = graphemes
            .iter()
            .filter(|grapheme| {
                grapheme.source_range.start >= run.start && grapheme.source_range.start < run.end
            })
            .cloned()
            .map(|mut grapheme| {
                grapheme.direction = direction;
                grapheme
            })
            .collect::<Vec<_>>();
        if direction == Direction::Rtl {
            run_graphemes.reverse();
        }
        visual.extend(run_graphemes);
    }

    visual
}

/// Compatibility wrapper used by committed-message rendering.
pub(crate) fn visual_graphemes_in_paragraph(
    paragraph: &str,
    line_range: Range<usize>,
    marked_range: Option<Range<usize>>,
    marked_ranges: &[Range<usize>],
) -> Vec<(String, bool, bool)> {
    visual_graphemes_with_ranges(paragraph, line_range)
        .into_iter()
        .map(|grapheme| {
            let marked = marked_range.as_ref().is_some_and(|range| {
                grapheme.source_range.start < range.end && grapheme.source_range.end > range.start
            });
            let additionally_marked = marked_ranges.iter().any(|range| {
                grapheme.source_range.start < range.end && grapheme.source_range.end > range.start
            });
            (grapheme.text, marked, additionally_marked)
        })
        .collect()
}

/// Convert logical message text to visual order for tests and small callers.
#[cfg(test)]
pub(crate) fn visual_text(text: &str) -> String {
    let mut visual = String::new();
    for paragraph in text.split('\n') {
        visual.push_str(
            &visual_graphemes_in_paragraph(paragraph, 0..paragraph.len(), None, &[])
                .into_iter()
                .map(|(grapheme, _, _)| grapheme)
                .collect::<String>(),
        );
        visual.push('\n');
    }
    visual.pop();
    visual
}

#[cfg(test)]
mod tests {
    use super::{Direction, visual_text};

    #[test]
    fn mixed_rtl_latin_and_numbers_have_stable_visual_order() {
        assert_eq!(visual_text("abc אבג 123"), "abc 123 גבא");
        assert_eq!(visual_text("abc אבג 123"), visual_text("abc אבג 123"));
    }

    #[test]
    fn pure_arabic_and_hebrew_keep_grapheme_order_deterministically() {
        assert_eq!(visual_text("مرحبا"), "ابحرم");
        assert_eq!(visual_text("שלום"), "םולש");
    }

    #[test]
    fn mixed_arabic_latin_numbers_and_punctuation_use_paragraph_context() {
        assert_eq!(visual_text("مرحبا abc 123!"), "!abc 123 ابحرم");
    }

    #[test]
    fn direction_uses_first_strong_character_and_ltr_fallback() {
        assert_eq!(Direction::from_text("123 שלום"), Direction::Rtl);
        assert_eq!(Direction::from_text("123 hello"), Direction::Ltr);
        assert_eq!(Direction::from_text("123 !?"), Direction::Ltr);
    }

    #[test]
    fn each_paragraph_uses_its_own_first_strong_direction() {
        assert_eq!(visual_text("123 שלום\n123 hello"), "םולש 123\n123 hello");
    }

    #[test]
    fn rtl_grapheme_clusters_remain_intact() {
        assert_eq!(visual_text("אְב"), "באְ");
    }

    #[test]
    fn visual_graphemes_expose_stable_logical_source_ranges() {
        let source = "a אב";
        let visual = super::visual_graphemes_with_ranges(source, 0..source.len());
        let rebuilt = visual
            .iter()
            .map(|item| item.text.as_str())
            .collect::<String>();
        assert_eq!(rebuilt, "a בא");
        assert!(
            visual
                .iter()
                .all(|item| source.is_char_boundary(item.source_range.start))
        );
        assert!(
            visual
                .iter()
                .all(|item| source.is_char_boundary(item.source_range.end))
        );
    }

    #[test]
    fn empty_and_control_only_text_have_deterministic_fallbacks() {
        assert_eq!(visual_text(""), "");
        let controls = "\u{200e}\u{200f}";
        assert_eq!(visual_text(controls), controls);
    }

    #[test]
    fn visual_text_does_not_mutate_logical_source() {
        let logical = String::from("abc אבג 123");
        let logical_before = logical.clone();
        let _visual = visual_text(&logical);
        assert_eq!(logical.as_bytes(), logical_before.as_bytes());
    }
}
