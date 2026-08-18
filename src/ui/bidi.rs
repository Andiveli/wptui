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
}

/// Convert logical message text to the visual order expected by positioned
/// terminal cells. Paragraphs are handled independently, while the source
/// string remains untouched by this presentation-only transformation.
///
/// This applies Unicode bidirectional run ordering, but not Arabic shaping or
/// glyph mirroring. Those are font/terminal rendering concerns and are not
/// provided by UAX #9 or by this app-level cell-order transformation.
#[cfg(test)]
pub(crate) fn visual_text(text: &str) -> String {
    visual_graphemes_with_range(text, None)
        .into_iter()
        .map(|(grapheme, _)| grapheme)
        .collect()
}

pub(crate) fn visual_graphemes_with_range(
    text: &str,
    marked_range: Option<Range<usize>>,
) -> Vec<(String, bool)> {
    if text.is_empty() {
        return Vec::new();
    }

    let paragraphs = BidiInfo::new(text, Some(Level::ltr())).paragraphs;
    let graphemes = text
        .grapheme_indices(true)
        .enumerate()
        .map(|(index, (start, grapheme))| (start, index, grapheme.to_owned()))
        .collect::<Vec<_>>();
    let mut visual = Vec::new();

    for paragraph in &paragraphs {
        let separator_start = text[..paragraph.range.end]
            .char_indices()
            .next_back()
            .and_then(|(start, character)| {
                matches!(unicode_bidi::bidi_class(character), BidiClass::B).then_some(start)
            })
            .unwrap_or(paragraph.range.end);
        if paragraph.range.start < separator_start {
            let paragraph_text = &text[paragraph.range.start..separator_start];
            let paragraph_info = BidiInfo::new(
                paragraph_text,
                Some(Direction::from_text(paragraph_text).level()),
            );
            let paragraph_info_data = &paragraph_info.paragraphs[0];
            let line = paragraph_info_data.range.clone();
            let (levels, runs) = paragraph_info.visual_runs(paragraph_info_data, line);
            for run in runs {
                let rtl = levels[run.start].is_rtl();
                let run_start = paragraph.range.start + run.start;
                let run_end = paragraph.range.start + run.end;
                let mut run_graphemes = graphemes
                    .iter()
                    .filter(|(start, _, _)| *start >= run_start && *start < run_end)
                    .map(|(_, index, grapheme)| {
                        let marked = marked_range
                            .as_ref()
                            .is_some_and(|range| range.contains(index));
                        (grapheme.clone(), marked)
                    })
                    .collect::<Vec<_>>();
                if rtl {
                    run_graphemes.reverse();
                }
                visual.extend(run_graphemes);
            }
        }

        for (_, index, grapheme) in graphemes
            .iter()
            .filter(|(start, _, _)| *start >= separator_start && *start < paragraph.range.end)
        {
            let marked = marked_range
                .as_ref()
                .is_some_and(|range| range.contains(index));
            visual.push((grapheme.clone(), marked));
        }
    }

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
