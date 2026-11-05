//! Optional News-only GitHub action. No credentials or subprocess output enter events.
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::App;
use super::events::{AppEvent, AppInput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Check,
    Star,
}

impl Operation {
    pub(crate) fn args(self) -> Vec<&'static str> {
        let mut args = vec!["api", "--hostname", "github.com"];
        if self == Self::Star {
            args.extend(["--method", "PUT"]);
        }
        args.extend(["--include", "user/starred/Andiveli/wptui"]);
        args
    }
}

pub(crate) type Runner = dyn Fn(Operation, &AtomicBool) -> Option<bool> + Send + Sync;

pub(crate) struct RepositoryStar {
    pub(crate) pending: Option<Operation>,
    pub(crate) success: bool,
    pub(crate) dismissed: bool,
    pub(crate) generation: u64,
    checked: bool,
    pub(crate) path: PathBuf,
    pub(crate) runner: Arc<Runner>,
    cancel: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl RepositoryStar {
    pub(crate) fn new(path: PathBuf) -> Self {
        // A missing file is initialized before offering an action. Any persistence
        // failure suppresses the invitation, including unknown/corrupt contents.
        let readable = fs::File::open(&path).and_then(|file| file.take(1).read(&mut [0]));
        let enabled = match readable {
            Ok(length) => length == 0 && OpenOptions::new().write(true).open(&path).is_ok(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .and_then(|file| file.sync_all())
                .is_ok(),
            Err(_) => false,
        };
        Self {
            pending: None,
            success: false,
            dismissed: !enabled,
            generation: 0,
            checked: false,
            path,
            runner: Arc::new(|operation, cancel| run_gh(Path::new("gh"), operation, cancel)),
            cancel: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for RepositoryStar {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn classify(success: bool, headers: &str) -> Option<bool> {
    // Only the HTTP status line counts, never a body/error mentioning 404.
    let status = headers
        .lines()
        .next()?
        .split_whitespace()
        .collect::<Vec<_>>();
    if !status.first()?.starts_with("HTTP/") {
        return None;
    }
    match (success, *status.get(1)?) {
        (true, "204") => Some(true),
        (false, "404") => Some(false),
        _ => None,
    }
}

pub(crate) fn run_gh(executable: &Path, operation: Operation, cancel: &AtomicBool) -> Option<bool> {
    run_gh_with_timeout(executable, operation, cancel, Duration::from_secs(8))
}

fn run_gh_with_timeout(
    executable: &Path,
    operation: Operation,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Option<bool> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    // Redirect, rather than pipe, so a noisy CLI cannot deadlock on a full pipe.
    // Read only the bounded header prefix; never log response bodies or stderr.
    let output = tempfile::tempfile().ok()?;
    let mut child = Command::new(executable)
        .args(operation.args())
        .stdin(Stdio::null())
        .stdout(Stdio::from(output.try_clone().ok()?))
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if !cancel.load(Ordering::Relaxed) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                // Never call wait(): even shutdown has a bounded reap budget.
                let _ = child.kill();
                let reap_deadline = Instant::now() + Duration::from_millis(250);
                while Instant::now() < reap_deadline {
                    if !matches!(child.try_wait(), Ok(None)) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                return None;
            }
        }
    };
    use std::io::{Seek, SeekFrom};
    let mut output = output;
    output.seek(SeekFrom::Start(0)).ok()?;
    let mut headers = String::new();
    output.take(4096).read_to_string(&mut headers).ok()?;
    let result = classify(status.success(), &headers);
    if operation == Operation::Star {
        result.filter(|starred| *starred)
    } else {
        result
    }
}

impl App<'_> {
    pub(crate) fn activate_news_star(&mut self) {
        if self.selected_section == super::actions::Section::News
            && !self.rail_on_logout
            && !self.should_quit
            && !self.logout_in_progress
            && !self.news.star.checked
        {
            self.news.star.checked = true;
            self.start_repository_star(Operation::Check);
        }
    }

    pub(crate) fn start_repository_star(&mut self, operation: Operation) {
        let star = &mut self.news.star;
        if star.dismissed
            || star.pending.is_some()
            || (operation == Operation::Star && self.news.repository_starred != Some(false))
        {
            return;
        }
        star.shutdown();
        star.cancel = Arc::new(AtomicBool::new(false));
        star.generation += 1;
        let generation = star.generation;
        star.pending = Some(operation);
        if operation == Operation::Check {
            self.news.repository_starred = None;
        }
        let runner = Arc::clone(&star.runner);
        let cancel = Arc::clone(&star.cancel);
        let tx = self.tx.clone();
        match std::thread::Builder::new()
            .name("news-repository-star".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runner(operation, &cancel)
                }))
                .unwrap_or(None);
                let _ = tx.send(AppInput::App(AppEvent::NewsStarFinished {
                    generation,
                    operation,
                    result,
                }));
            }) {
            Ok(worker) => star.worker = Some(worker),
            Err(_) => {
                star.pending = None;
                self.news.repository_starred = None;
            }
        }
    }

    pub(crate) fn dismiss_repository_star(&mut self) {
        if self.news.star.dismissed || self.news.star.pending == Some(Operation::Star) {
            return;
        }
        let star = &mut self.news.star;
        star.dismissed = true;
        star.generation += 1;
        star.cancel.store(true, Ordering::Relaxed);
        star.pending = None;
        self.news.repository_starred = None;
        // Fail closed in memory as well as on the next launch if a partial write
        // leaves an unrecognized marker. Never offer a retry after a write error.
        let _ = OpenOptions::new()
            .write(true)
            .open(&star.path)
            .and_then(|mut file| {
                file.write_all(b"dismissed\n")?;
                file.sync_all()
            });
    }

    pub(crate) fn finish_repository_star(
        &mut self,
        generation: u64,
        operation: Operation,
        result: Option<bool>,
    ) {
        let star = &mut self.news.star;
        if star.dismissed || star.generation != generation || star.pending != Some(operation) {
            return;
        }
        // The event is sent just before thread exit. Leave joining to the next
        // operation/shutdown, avoiding a blocking event-handler join.
        star.pending = None;
        star.success = operation == Operation::Star && result == Some(true);
        self.news.repository_starred = result;
    }
}

#[cfg(test)]
#[path = "news_star_tests.rs"]
mod tests;
