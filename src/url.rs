use std::io;
use std::ops::Range;
use std::process::{Command, Stdio};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UrlLaunchPlan {
    pub program: String,
    pub arguments: Vec<String>,
}

pub fn url_launch_plan(url: &str) -> UrlLaunchPlan {
    #[cfg(target_os = "macos")]
    {
        UrlLaunchPlan {
            program: "open".into(),
            arguments: vec![url.into()],
        }
    }

    #[cfg(target_os = "windows")]
    {
        UrlLaunchPlan {
            program: "rundll32".into(),
            arguments: vec!["url.dll,FileProtocolHandler".into(), url.into()],
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        UrlLaunchPlan {
            program: "xdg-open".into(),
            arguments: vec![url.into()],
        }
    }
}

pub fn execute_url_launch(plan: &UrlLaunchPlan) -> io::Result<()> {
    Command::new(&plan.program)
        .args(&plan.arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

pub fn extract_openable_urls(text: &str) -> Vec<String> {
    extract_openable_url_ranges(text)
        .into_iter()
        .map(|range| text[range].to_owned())
        .collect()
}

// Capture accepted occurrences before discarding token boundaries or trimming
// punctuation, so display consumers never relocate a URL to a rejected token.
pub(crate) fn extract_openable_url_ranges(text: &str) -> Vec<Range<usize>> {
    let mut urls = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let remaining = &text[index..];
        let Some(offset) = find_url_start(remaining) else {
            break;
        };
        let start = index + offset;
        if start > 0 && !is_url_boundary(bytes[start - 1]) {
            index = start + 1;
            continue;
        }
        let end = text[start..]
            .find(char::is_whitespace)
            .map_or(text.len(), |length| start + length);
        let candidate = trim_url_punctuation(&text[start..end]);
        if is_openable_url(candidate) {
            urls.push(start..start + candidate.len());
        }
        index = end.max(start + 1);
    }
    urls
}

fn find_url_start(text: &str) -> Option<usize> {
    text.char_indices().find_map(|(index, _)| {
        let tail = &text[index..];
        (tail.starts_with("http://") || tail.starts_with("https://")).then_some(index)
    })
}

fn is_url_boundary(byte: u8) -> bool {
    !byte.is_ascii_alphanumeric() && !matches!(byte, b'_' | b':' | b'/' | b'@')
}

fn trim_url_punctuation(mut value: &str) -> &str {
    while let Some(last) = value.chars().last() {
        let trim = matches!(last, '.' | ',' | '!' | '?' | ';' | ':' | '\'' | '"')
            || matches!(last, ')' | ']' | '}')
                && closing_count(value, last) > opening_count(value, matching_open(last));
        if !trim {
            break;
        }
        value = &value[..value.len() - last.len_utf8()];
    }
    value
}

fn matching_open(closing: char) -> char {
    match closing {
        ')' => '(',
        ']' => '[',
        '}' => '{',
        _ => unreachable!("only closing punctuation is passed here"),
    }
}

fn closing_count(value: &str, closing: char) -> usize {
    value
        .chars()
        .filter(|character| *character == closing)
        .count()
}

fn opening_count(value: &str, opening: char) -> usize {
    value
        .chars()
        .filter(|character| *character == opening)
        .count()
}

fn is_openable_url(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once("://") else {
        return false;
    };
    matches!(scheme, "http" | "https")
        && !rest.is_empty()
        && !rest.starts_with('/')
        && rest
            .bytes()
            .all(|byte| !byte.is_ascii_control() && !byte.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::{extract_openable_url_ranges, extract_openable_urls, url_launch_plan};

    #[test]
    fn extractor_strings_and_ranges_preserve_token_rules() {
        for (rule, source, expected) in [
            (
                "rejected prefixed duplicates",
                "xhttp://a _http://a :http://a /http://a @http://a http://a",
                vec![(
                    "xhttp://a _http://a :http://a /http://a @http://a ".len(),
                    "http://a",
                )],
            ),
            (
                "balanced punctuation and trailing sentence punctuation",
                "(https://example.test/a_(b)).",
                vec![(1, "https://example.test/a_(b)")],
            ),
            (
                "multibyte byte offset",
                "阿丽 👩‍💻 http://a, tail",
                vec![("阿丽 👩‍💻 ".len(), "http://a")],
            ),
            (
                "valid duplicates remain ordered",
                "http://a http://a",
                vec![(0, "http://a"), ("http://a ".len(), "http://a")],
            ),
            (
                "validation and case sensitivity",
                "https:// http:///bad HTTP://a http://a",
                vec![("https:// http:///bad HTTP://a ".len(), "http://a")],
            ),
        ] {
            let expected_ranges = expected
                .iter()
                .map(|(start, url)| *start..*start + url.len())
                .collect::<Vec<_>>();
            let expected_urls = expected.iter().map(|(_, url)| *url).collect::<Vec<_>>();
            let ranges = extract_openable_url_ranges(source);
            assert_eq!(ranges, expected_ranges, "{rule}");
            assert_eq!(extract_openable_urls(source), expected_urls, "{rule}");
            assert_eq!(
                ranges
                    .into_iter()
                    .map(|range| &source[range])
                    .collect::<Vec<_>>(),
                expected_urls,
                "{rule}"
            );
        }
    }

    #[test]
    fn preserves_balanced_parentheses_and_trims_sentence_punctuation() {
        assert_eq!(
            extract_openable_urls("(https://example.test/a_(b))."),
            vec!["https://example.test/a_(b)"],
        );
    }

    #[test]
    fn launch_plan_passes_the_url_as_one_argument() {
        let plan = url_launch_plan("https://example.test/?a=1&b=2");
        assert!(
            plan.arguments
                .iter()
                .any(|argument| argument == "https://example.test/?a=1&b=2")
        );
    }
}
