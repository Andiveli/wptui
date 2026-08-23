use std::backtrace::Backtrace;
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::Utc;

const MAX_BREADCRUMBS: usize = 32;
const MAX_BREADCRUMB_LENGTH: usize = 160;
const MAX_REPORT_LENGTH: usize = 64 * 1024;

struct State {
    path: PathBuf,
    breadcrumbs: Mutex<VecDeque<String>>,
}

static STATE: OnceLock<Arc<State>> = OnceLock::new();

/// Installs the persistent panic reporter. The report is truncated at the
/// start of every run and only written when a panic occurs.
pub fn install(path: PathBuf) {
    let state = Arc::new(State {
        path,
        breadcrumbs: Mutex::new(VecDeque::with_capacity(MAX_BREADCRUMBS)),
    });
    let hook_state = Arc::clone(&state);
    let _ = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&hook_state.path);
    let previous = panic::take_hook();
    let _ = STATE.set(state);
    panic::set_hook(Box::new(move |info| {
        let _ =
            std::panic::catch_unwind(AssertUnwindSafe(|| write_panic_report(&hook_state, info)));
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| previous(info)));
    }));
}

/// Records a bounded, sanitized phase marker for the next crash report.
pub fn breadcrumb(phase: &str, state: &str) {
    let Some(reporter) = STATE.get() else { return };
    let entry = sanitize_text(&format!("{}: {}", phase, state), MAX_BREADCRUMB_LENGTH);
    let mut breadcrumbs = reporter
        .breadcrumbs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if breadcrumbs.len() == MAX_BREADCRUMBS {
        breadcrumbs.pop_front();
    }
    breadcrumbs.push_back(entry);
}

fn write_panic_report(state: &State, info: &PanicHookInfo<'_>) {
    let breadcrumbs = state
        .breadcrumbs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    let payload = info
        .payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload");
    let location = info
        .location()
        .map(|location| {
            format!(
                "{}:{}:{}",
                safe_location(location.file()),
                location.line(),
                location.column()
            )
        })
        .unwrap_or_else(|| "unknown".to_owned());
    let backtrace = sanitize_multiline(
        &format!("{}", Backtrace::force_capture()),
        MAX_REPORT_LENGTH,
    );
    let report = format_report(
        &Utc::now().to_rfc3339(),
        payload,
        &location,
        &breadcrumbs,
        &backtrace,
    );

    let result = write_report(&state.path, &report);
    let _ = result;
}

fn write_report(path: &Path, report: &str) -> std::io::Result<()> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .and_then(|mut file| {
            file.write_all(report.as_bytes())?;
            file.sync_all()
        })
}

fn format_report(
    timestamp: &str,
    payload: &str,
    location: &str,
    breadcrumbs: &[String],
    backtrace: &str,
) -> String {
    let mut report = String::new();
    report.push_str("wptui crash report\n");
    report.push_str(&format!("timestamp: {}\n", sanitize_text(timestamp, 128)));
    report.push_str(&format!("panic: {}\n", sanitize_text(payload, 2_048)));
    report.push_str(&format!("location: {}\n", sanitize_text(location, 256)));
    report.push_str("breadcrumbs:\n");
    for breadcrumb in breadcrumbs.iter().take(MAX_BREADCRUMBS) {
        report.push_str("- ");
        report.push_str(&sanitize_text(breadcrumb, MAX_BREADCRUMB_LENGTH));
        report.push('\n');
    }
    report.push_str("backtrace (forced):\n");
    report.push_str(backtrace);
    report.truncate(MAX_REPORT_LENGTH);
    report
}

fn safe_location(file: &str) -> String {
    file.rsplit_once("/src/")
        .map(|(_, suffix)| format!("src/{suffix}"))
        .or_else(|| {
            Path::new(file)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

fn sanitize_text(value: &str, max_length: usize) -> String {
    let sanitized = value
        .split_whitespace()
        .map(|token| {
            if token.starts_with('/') || token.contains("@") || token.contains("://") {
                "<redacted>"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    sanitized.chars().take(max_length).collect()
}

fn sanitize_multiline(value: &str, max_length: usize) -> String {
    value
        .lines()
        .map(|line| sanitize_text(line, max_length))
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(max_length)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn report_is_written_with_panic_fields_and_bounded_breadcrumbs() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("crash.log");
        let state = State {
            path: path.clone(),
            breadcrumbs: Mutex::new((0..40).map(|n| format!("phase-{n}")).collect()),
        };
        let report = format_report(
            "2026-01-01T00:00:00Z",
            "boom /home/private@example.test",
            "src/main.rs:1:2",
            &state
                .breadcrumbs
                .lock()
                .unwrap()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            "forced backtrace",
        );
        write_report(&path, &report).unwrap();
        let contents = fs::read_to_string(path).unwrap();
        assert!(contents.contains("timestamp: 2026-01-01T00:00:00Z"));
        assert!(contents.contains("backtrace (forced):"));
        assert!(!contents.contains("/home/private"));
        assert!(
            contents
                .lines()
                .filter(|line| line.starts_with("- "))
                .count()
                <= MAX_BREADCRUMBS
        );
    }

    #[test]
    fn breadcrumb_sanitization_redacts_sensitive_tokens_and_bounds_length() {
        let value = sanitize_text(
            "phase /home/me secret@example.test https://private.test",
            20,
        );
        assert!(!value.contains("/home"));
        assert!(!value.contains('@'));
        assert!(value.chars().count() <= 20);
    }
}
