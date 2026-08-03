use std::io;
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
            urls.push(candidate.to_owned());
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
    use super::{extract_openable_urls, url_launch_plan};

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
