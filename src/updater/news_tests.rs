use super::*;
use std::collections::VecDeque;

struct Pages {
    replies: VecDeque<Result<Vec<u8>, String>>,
    calls: Vec<u32>,
}

impl Pages {
    fn new(pages: &[&str]) -> Self {
        Self {
            replies: pages
                .iter()
                .map(|page| Ok(page.as_bytes().to_vec()))
                .collect(),
            calls: vec![],
        }
    }

    fn offline() -> Self {
        Self {
            replies: VecDeque::from([Err("offline\u{1b}[31m".into())]),
            calls: vec![],
        }
    }
}

impl NewsPagePort for Pages {
    fn page(&mut self, page: u32) -> Result<Vec<u8>, String> {
        self.calls.push(page);
        self.replies.pop_front().expect("unexpected fetch")
    }
}

fn current() -> Version {
    Version::new(0, 3, 0)
}

const RELEASE: &str = r#"[{"tag_name":"v0.4.0","name":"Fourth","body":"Changes","published_at":"2026-04-01","draft":false,"prerelease":false}]"#;

#[test]
fn installed_notes_are_bundled_and_honest() {
    let note = installed_note();
    assert_eq!(note.version.to_string(), env!("CARGO_PKG_VERSION"));
    assert_eq!(
        note.body,
        "Release notes were not provided for this version."
    );
}

#[test]
fn stable_history_includes_intermediates_sorted_and_deduplicated() {
    let dir = tempfile::tempdir().unwrap();
    let mut pages = Pages::new(&[
        r#"[{"tag_name":"v0.4.0","name":"Fourth","body":"Changes","published_at":"2026-04-01","draft":false,"prerelease":false},{"tag_name":"v0.3.0"},{"tag_name":"v0.2.0"},{"tag_name":"v2.0.0","draft":true},{"tag_name":"v3.0.0","prerelease":true},{"tag_name":"v4.0.0-rc.1"},{"tag_name":"not-a-version"},{"tag_name":"\u001b[31mv9.0.0"}]"#,
        r#"[{"tag_name":"v0.5.0","body":null},{"tag_name":"v0.4.0"}]"#,
        "[]",
    ]);
    let history = load_history(&mut pages, &current(), &dir.path().join("news"));
    assert_eq!(pages.calls, vec![1, 2, 3]);
    assert_eq!(
        history
            .releases
            .iter()
            .map(|r| r.tag.as_str())
            .collect::<Vec<_>>(),
        vec!["v0.5.0", "v0.4.0"]
    );
    assert_eq!(history.releases[1].title, "Fourth");
    assert_eq!(history.releases[1].body, "Changes");
    assert_eq!(
        history.releases[1].published_at.as_deref(),
        Some("2026-04-01")
    );
    assert_eq!(history.releases[0].body, MISSING_NOTES);
    assert!(!history.stale && !history.incomplete);
    assert_eq!(history.error, None);
    assert_eq!(history.cache_error, None);
}

#[test]
fn terminal_sequences_and_format_controls_are_not_displayed() {
    let dir = tempfile::tempdir().unwrap();
    for (input, expected) in [
        (r#"a\u001b[31mb\u001b[0m"#, "ab"),
        (r#"a\u001b]52;c;clipboard\u0007b"#, "ab"),
        (r#"a\u001bPpayload\u001b\\b"#, "ab"),
        (r#"a\u009dtitle\u009cb"#, "ab"),
        (r#"a\u202eb\u0000\r\n\tc"#, "ab\n\tc"),
        (r#"a\u001b]unterminated"#, "a"),
    ] {
        let json = format!(r#"[{{"tag_name":"v0.4.0","body":"{input}"}}]"#);
        let history = load_history(
            &mut Pages::new(&[&json, "[]"]),
            &current(),
            &dir.path().join("news"),
        );
        assert_eq!(history.error, None);
        assert_eq!(history.releases[0].body, expected, "{input:?}");
    }
    let mut pages = Pages::new(&[
        r#"[{"tag_name":"v0.4.0","name":"\u001b[31mTitle","body":"\u001b]52;c;evil\u0007Notes","published_at":"date\u0000"}]"#,
        "[]",
    ]);
    let history = load_history(&mut pages, &current(), &dir.path().join("news"));
    assert_eq!(history.releases[0].title, "Title");
    assert_eq!(history.releases[0].body, "Notes");
    assert_eq!(history.releases[0].published_at.as_deref(), Some("date"));
}

#[test]
fn cache_is_atomic_replaced_and_refiltered_on_offline_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("news");
    load_history(&mut Pages::new(&[RELEASE, "[]"]), &current(), &path);
    let bytes = std::fs::read(&path).unwrap();
    let offline = load_history(&mut Pages::offline(), &current(), &path);
    assert!(offline.stale && !offline.incomplete);
    assert_eq!(offline.error.as_deref(), Some("offline"));
    assert_eq!(offline.releases.len(), 1);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let upgraded = load_history(&mut Pages::offline(), &Version::new(0, 4, 0), &path);
    assert!(upgraded.releases.is_empty() && upgraded.error.is_some());
    let fresh = load_history(&mut Pages::new(&["[]"]), &current(), &path);
    assert!(!fresh.stale && !fresh.incomplete);
    assert!(fresh.releases.is_empty() && fresh.error.is_none());
    assert_ne!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn absent_corrupt_or_oversized_cache_never_means_up_to_date_on_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("news");
    for contents in [
        None,
        Some(b"broken".to_vec()),
        Some(vec![b' '; MAX_CACHE_BYTES + 1]),
    ] {
        if let Some(bytes) = contents {
            std::fs::write(&path, bytes).unwrap();
        }
        let history = load_history(&mut Pages::offline(), &current(), &path);
        assert!(history.stale && history.incomplete && history.error.is_some());
        assert!(history.releases.is_empty());
    }
}

#[test]
fn invalid_or_oversized_pages_preserve_previous_cache() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("news");
    load_history(&mut Pages::new(&[RELEASE, "[]"]), &current(), &path);
    let before = std::fs::read(&path).unwrap();
    for reply in [
        Ok(b"invalid".to_vec()),
        Ok(vec![b' '; MAX_PAGE_BYTES + 1]),
        Err("timeout".into()),
    ] {
        let mut pages = Pages {
            replies: VecDeque::from([reply]),
            calls: vec![],
        };
        let history = load_history(&mut pages, &current(), &path);
        assert!(history.stale && history.error.is_some());
        assert_eq!(history.releases.len(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn page_size_boundary_and_entry_count_limits_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("news");
    let boundary = format!("[]{}", " ".repeat(MAX_PAGE_BYTES - 2));
    assert_eq!(boundary.len(), MAX_PAGE_BYTES);
    let history = load_history(&mut Pages::new(&[&boundary]), &current(), &path);
    assert!(!history.incomplete && history.error.is_none());
    let before = std::fs::read(&path).unwrap();
    let entries = format!(
        "[{}]",
        vec![r#"{"tag_name":"v0.4.0"}"#; PAGE_SIZE + 1].join(",")
    );
    let history = load_history(&mut Pages::new(&[&entries]), &current(), &path);
    assert_eq!(
        history.error.as_deref(),
        Some("release history page exceeds entry limit")
    );
    assert!(history.stale);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn untrusted_cache_is_shape_checked_and_display_sanitized() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("news");
    for invalid in ["[]", "[[],[]]", "null", r#"[[{"body":"no tag"}]]"#] {
        std::fs::write(&path, invalid).unwrap();
        let history = load_history(&mut Pages::offline(), &current(), &path);
        assert!(history.incomplete && history.stale && history.error.is_some());
        assert!(history.releases.is_empty());
    }
    std::fs::write(&path, r#"[[{"tag_name":"v0.4.0","body":"\u001b[31mCached","assets":[{"browser_download_url":"untrusted"}]}],[]]"#).unwrap();
    let history = load_history(&mut Pages::offline(), &current(), &path);
    assert!(history.stale && !history.incomplete && history.error.is_some());
    assert_eq!(history.releases[0].body, "Cached");
}

#[test]
fn bounded_history_explicitly_reports_incomplete_even_after_cache_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("news");
    let mut pages = Pages::new(&vec![RELEASE; MAX_PAGES as usize]);
    let history = load_history(&mut pages, &current(), &path);
    assert_eq!(pages.calls.len(), MAX_PAGES as usize);
    assert!(history.incomplete && !history.stale && history.error.is_none());
    assert!(load_history(&mut Pages::offline(), &current(), &path).incomplete);
}

#[test]
fn failed_later_page_does_not_replace_cache_with_partial_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("news");
    load_history(&mut Pages::new(&[RELEASE, "[]"]), &current(), &path);
    let before = std::fs::read(&path).unwrap();
    let mut pages = Pages::new(&[r#"[{"tag_name":"v0.5.0"}]"#]);
    pages.replies.push_back(Err("timeout".into()));
    let history = load_history(&mut pages, &current(), &path);
    assert!(history.stale);
    assert_eq!(history.releases[0].tag, "v0.4.0");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn cache_write_failure_does_not_hide_successful_fetch() {
    let dir = tempfile::tempdir().unwrap();
    let history = load_history(&mut Pages::new(&[RELEASE, "[]"]), &current(), dir.path());
    assert_eq!(history.releases.len(), 1);
    assert!(!history.stale && !history.incomplete && history.error.is_none());
    assert!(history.cache_error.is_some());
}
