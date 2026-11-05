//! Display-only formatting. All offsets refer to the unchanged UTF-8 source.
use std::ops::Range;

use ratatui::style::{Modifier, Style};

pub(crate) struct Markdown {
    hidden: Vec<bool>,
    styles: Vec<Style>,
}

pub(crate) struct Projection {
    pub(crate) text: String,
    pub(crate) styles: Vec<(Range<usize>, Style)>,
    mapping: Vec<(Range<usize>, Range<usize>)>,
}

impl Projection {
    pub(crate) fn ranges(&self, source: &[Range<usize>]) -> Vec<Range<usize>> {
        self.mapping
            .iter()
            .filter(|(original, _)| {
                source
                    .iter()
                    .any(|range| original.start < range.end && original.end > range.start)
            })
            .map(|(_, display)| display.clone())
            .collect()
    }
}

impl Markdown {
    pub(crate) fn new(source: &str, active: Option<usize>) -> Self {
        let mut result = Self {
            hidden: vec![false; source.len()],
            styles: vec![Style::default(); source.len()],
        };
        result.parse(source, 0..source.len(), active, 0);
        let mut offset = 0;
        for line in source.split('\n') {
            let end = offset + line.len();
            // Code owns its contents, including what otherwise looks like a heading.
            if !result
                .styles
                .get(offset)
                .is_some_and(|s| s.add_modifier.contains(Modifier::REVERSED))
            {
                let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
                if (1..=6).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ') {
                    result.format(
                        offset..end,
                        offset + hashes + 1..end,
                        Style::default().add_modifier(Modifier::BOLD),
                        active,
                    );
                } else if line.starts_with("> ") {
                    result.format(
                        offset..end,
                        offset + 2..end,
                        Style::default().add_modifier(Modifier::ITALIC | Modifier::DIM),
                        active,
                    );
                } else if line.starts_with("- ") || line.starts_with("* ") {
                    result.paint(
                        offset..offset + 1,
                        Style::default().add_modifier(Modifier::DIM),
                    );
                }
            }
            offset = end + 1;
        }
        result
    }

    pub(crate) fn visible(&self, byte: usize) -> bool {
        !self.hidden.get(byte).copied().unwrap_or(false)
    }

    pub(crate) fn style(&self, byte: usize) -> Style {
        self.styles.get(byte).copied().unwrap_or_default()
    }

    pub(crate) fn project(&self, source: &str) -> Projection {
        let mut result = Projection {
            text: String::new(),
            styles: Vec::new(),
            mapping: Vec::new(),
        };
        for (byte, ch) in source.char_indices() {
            if !self.visible(byte) {
                continue;
            }
            let start = result.text.len();
            result.text.push(ch);
            let display = start..result.text.len();
            let style = self.style(byte);
            match result.styles.last_mut() {
                Some((range, previous)) if *previous == style && range.end == start => {
                    range.end = display.end;
                }
                _ => result.styles.push((display.clone(), style)),
            }
            result.mapping.push((byte..byte + ch.len_utf8(), display));
        }
        result
    }

    fn paint(&mut self, range: Range<usize>, style: Style) {
        for cell in &mut self.styles[range] {
            *cell = cell.patch(style);
        }
    }

    fn format(
        &mut self,
        fragment: Range<usize>,
        content: Range<usize>,
        style: Style,
        active: Option<usize>,
    ) {
        self.paint(content.clone(), style);
        if active.is_some_and(|cursor| fragment.start <= cursor && cursor <= fragment.end) {
            return;
        }
        self.hidden[fragment.start..content.start].fill(true);
        self.hidden[content.end..fragment.end].fill(true);
    }

    fn parse(&mut self, source: &str, range: Range<usize>, active: Option<usize>, depth: usize) {
        if depth >= 16 {
            return;
        }
        let mut at = range.start;
        let mut line_end = range.start;
        let mut unmatched_until = [0; 8];
        while at < range.end {
            if at >= line_end {
                line_end = source[at..range.end]
                    .find('\n')
                    .map_or(range.end, |n| at + n);
            }
            let tail = &source[at..range.end];
            let ch = tail.chars().next().unwrap();
            if ch == '\\' {
                let next = at + 1;
                if let Some(escaped) = source[next..range.end].chars().next()
                    && "*_~`#>[\\".contains(escaped)
                {
                    self.format(
                        at..next + escaped.len_utf8(),
                        next..next + escaped.len_utf8(),
                        Style::default(),
                        active,
                    );
                    at = next + escaped.len_utf8();
                    continue;
                }
            }
            // Keep link destinations readable, with their original openable URL.
            if ch == '['
                && (at == 0 || !source[..at].ends_with('!'))
                && let Some(label_end) = tail.find("](")
                && let Some(url_end) = tail[label_end + 2..].find(')')
            {
                let label_end = at + label_end;
                let end = label_end + 2 + url_end + 1;
                let destination = &source[label_end + 2..end - 1];
                if destination.starts_with("https://") || destination.starts_with("http://") {
                    self.paint(
                        at + 1..end - 1,
                        Style::default().add_modifier(Modifier::UNDERLINED),
                    );
                    if !active.is_some_and(|cursor| at <= cursor && cursor <= end) {
                        self.hidden[at] = true;
                        self.hidden[label_end] = true;
                    }
                    self.parse(source, at + 1..label_end, active, depth + 1);
                    at = end;
                    continue;
                }
            }
            let marker = ["```", "**", "__", "~~", "`", "*", "_", "~"]
                .into_iter()
                .enumerate()
                .find(|(_, marker)| tail.starts_with(marker));
            let Some((marker_index, marker)) = marker else {
                at += ch.len_utf8();
                continue;
            };
            let start = at + marker.len();
            let code = marker.starts_with('`');
            let boundary = at == range.start
                || !source[..at]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric);
            if !code && (!boundary || source[start..range.end].starts_with(char::is_whitespace)) {
                at = start;
                continue;
            }
            let end_limit = if marker == "```" { range.end } else { line_end };
            let closing = source[start..end_limit]
                .match_indices(marker)
                .find_map(|(offset, _)| {
                    let end = start + offset;
                    let escaped = source[..end]
                        .bytes()
                        .rev()
                        .take_while(|b| *b == b'\\')
                        .count()
                        % 2
                        == 1;
                    let after = end + marker.len();
                    let boundary = after == range.end
                        || !source[after..]
                            .chars()
                            .next()
                            .is_some_and(char::is_alphanumeric);
                    (end > start
                        && !escaped
                        && (code || (boundary && !source[..end].ends_with(char::is_whitespace))))
                    .then_some(end)
                });
            let Some(end) = closing else {
                unmatched_until[marker_index] = end_limit;
                at = start;
                continue;
            };
            let modifier = match marker {
                "```" | "`" => Modifier::REVERSED,
                "_" => Modifier::ITALIC,
                "~" | "~~" => Modifier::CROSSED_OUT,
                _ => Modifier::BOLD,
            };
            let mut content_start = start;
            let mut content_end = end;
            if marker == "```"
                && let Some(newline) = source[start..end].find('\n')
            {
                let language = &source[start..start + newline];
                if language
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_+-".contains(c))
                {
                    content_start = start + newline + 1;
                    if source[..end].ends_with('\n') {
                        content_end = end.saturating_sub(1).max(content_start);
                    }
                }
            }
            if code {
                self.paint(
                    at..end + marker.len(),
                    Style::default().add_modifier(modifier),
                );
            }
            self.format(
                at..end + marker.len(),
                content_start..content_end,
                Style::default().add_modifier(modifier),
                active,
            );
            if !code {
                self.parse(source, start..end, active, depth + 1);
            }
            at = end + marker.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whatsapp_and_extended_markdown_project_without_rewriting_source() {
        for (source, expected, modifier) in [
            ("*bold*", "bold", Modifier::BOLD),
            ("_italic_", "italic", Modifier::ITALIC),
            ("~old~", "old", Modifier::CROSSED_OUT),
            ("**bold**", "bold", Modifier::BOLD),
            ("__bold__", "bold", Modifier::BOLD),
            ("~~old~~", "old", Modifier::CROSSED_OUT),
            ("`*literal*`", "*literal*", Modifier::REVERSED),
            ("```code```", "code", Modifier::REVERSED),
            ("# Title", "Title", Modifier::BOLD),
            ("> quoted", "quoted", Modifier::ITALIC),
        ] {
            let saved = source.to_owned();
            let projected = Markdown::new(source, None).project(source);
            assert_eq!(projected.text, expected, "{source}");
            assert!(
                projected
                    .styles
                    .iter()
                    .all(|(_, style)| style.add_modifier.contains(modifier)),
                "{source}"
            );
            assert_eq!(source, saved);
        }
    }

    #[test]
    fn incomplete_markers_words_lists_and_unsupported_syntax_stay_literal() {
        for source in [
            "hello",
            "*unfinished",
            "a_b_c",
            "a*b*c",
            "* spaced *",
            "- item",
            "1. item",
            "| a | b |",
            "![image](file.png)",
        ] {
            assert_eq!(Markdown::new(source, None).project(source).text, source);
        }
        assert_eq!(
            Markdown::new(r"\*literal\*", None)
                .project(r"\*literal\*")
                .text,
            "*literal*"
        );
    }

    #[test]
    fn nesting_unicode_links_and_source_ranges_are_preserved() {
        let source = "**_阿丽 👩‍💻_** [site](https://example.com)";
        let projected = Markdown::new(source, None).project(source);
        assert_eq!(projected.text, "阿丽 👩‍💻 site(https://example.com)");
        assert!(
            projected.styles[0]
                .1
                .add_modifier
                .contains(Modifier::BOLD | Modifier::ITALIC)
        );
        let start = source.find("阿丽").unwrap();
        let ranges = projected.ranges(&[start..start + "阿丽".len()]);
        assert_eq!(ranges.first().unwrap().start, 0);
        assert_eq!(ranges.last().unwrap().end, "阿丽".len());
    }

    #[test]
    fn fenced_code_hides_language_and_does_not_parse_its_contents() {
        let source = "```rust\n# *literal*\n```";
        assert_eq!(
            Markdown::new(source, None).project(source).text,
            "# *literal*"
        );
        assert_eq!(Markdown::new(source, Some(12)).project(source).text, source);
    }

    #[test]
    fn only_active_fragment_reveals_its_markers() {
        let source = "*one* **two**";
        assert_eq!(
            Markdown::new(source, Some(2)).project(source).text,
            "*one* two"
        );
        assert_eq!(
            Markdown::new(source, Some(9)).project(source).text,
            "one **two**"
        );
        assert_eq!(Markdown::new(source, None).project(source).text, "one two");
    }
}
