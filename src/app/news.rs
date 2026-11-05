//! App-owned display state; release history never grants executable/install authority.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::App;
use crate::app::actions::{AppAction, FocusPane, Section};
use crate::app::events::{AppEvent, AppInput};
use crate::input_key::{Key, KeyCode};
use crate::updater::news::{GithubNewsClient, load_history};
pub use crate::updater::news::{NewsHistory, ReleaseNote, installed_note};

type NewsSource = dyn Fn(&Path) -> NewsHistory + Send + Sync;

pub struct NewsState {
    pub installed: ReleaseNote,
    pub repository_starred: Option<bool>,
    pub(crate) star: super::news_star::RepositoryStar,
    pub history: Option<NewsHistory>,
    /// Zero selects bundled installed notes; newer releases follow backend order.
    pub selected: usize,
    pub scroll: usize,
    pub loading: bool,
    pub cache_path: PathBuf,
    pub(crate) source: Arc<NewsSource>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl NewsState {
    pub fn new(cache_path: PathBuf) -> Self {
        let star_path = cache_path.with_file_name("news-repository-star");
        Self::with_star_path(cache_path, star_path)
    }

    pub(crate) fn with_star_path(cache_path: PathBuf, star_path: PathBuf) -> Self {
        Self {
            installed: installed_note(),
            repository_starred: None,
            star: super::news_star::RepositoryStar::new(star_path),
            history: None,
            selected: 0,
            scroll: 0,
            loading: false,
            cache_path,
            worker: None,
            source: Arc::new(|path| {
                load_history(
                    &mut GithubNewsClient::new(),
                    &installed_note().version,
                    path,
                )
            }),
        }
    }

    pub(crate) fn join(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    /// The selected stable note is the explicit target, not latest/main.
    pub fn selected_note(&self) -> &ReleaseNote {
        self.history
            .as_ref()
            .and_then(|history| {
                self.selected
                    .checked_sub(1)
                    .and_then(|i| history.releases.get(i))
            })
            .unwrap_or(&self.installed)
    }

    pub fn note_count(&self) -> usize {
        1 + self
            .history
            .as_ref()
            .map_or(0, |history| history.releases.len())
    }

    pub fn select(&mut self, selected: usize) {
        self.selected = selected.min(self.note_count().saturating_sub(1));
        self.scroll = 0;
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.select(self.selected.saturating_add_signed(delta));
    }

    pub fn scroll_by(&mut self, delta: isize) {
        self.scroll = self.scroll.saturating_add_signed(delta);
    }

    pub(crate) fn apply(&mut self, history: NewsHistory) {
        // Preserve selection by version across refreshes, never silently retarget it.
        let version = self.selected_note().version.clone();
        let mut history = history;
        if history.stale && history.releases.is_empty() {
            if let Some(previous) = &self.history {
                history.releases = previous.releases.clone();
            }
        }
        let selected = history
            .releases
            .iter()
            .position(|note| note.version == version)
            .map_or(0, |i| i + 1);
        self.history = Some(history);
        self.loading = false;
        self.select(selected);
    }
}

impl Drop for NewsState {
    fn drop(&mut self) {
        self.join();
    }
}

impl App<'_> {
    pub(crate) fn refresh_news(&mut self) {
        if self.news.loading {
            return;
        }
        self.news.join();
        self.news.loading = true;
        let source = Arc::clone(&self.news.source);
        let path = self.news.cache_path.clone();
        let tx = self.tx.clone();
        // Keep one handle so shutdown/restart cannot race atomic cache publication.
        let spawn = std::thread::Builder::new()
            .name("release-news".into())
            .spawn(move || {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| source(&path)))
                        .unwrap_or_else(|_| NewsHistory {
                            releases: vec![],
                            stale: true,
                            incomplete: true,
                            error: Some("Release history worker stopped unexpectedly".into()),
                            cache_error: None,
                        });
                let _ = tx.send(AppInput::App(AppEvent::NewsLoaded(result)));
            });
        match spawn {
            Ok(worker) => self.news.worker = Some(worker),
            Err(error) => self.news.apply(NewsHistory {
                releases: vec![],
                stale: true,
                incomplete: true,
                error: Some(format!("Could not start release history worker: {error}")),
                cache_error: None,
            }),
        }
    }

    /// Before chat search handling, but after modal handling.
    pub(crate) fn handle_news_key(&mut self, key: &Key) -> bool {
        if self.selected_section != Section::News || self.rail_on_logout {
            return false;
        }
        if self.handle_news_update_key(key) {
            return true;
        }
        if key == &Key::c('s') {
            self.start_repository_star(super::news_star::Operation::Star);
            return true;
        }
        if key == &Key::c('d') {
            self.dismiss_repository_star();
            return true;
        }
        if key == &Key::c('r') {
            self.refresh_news();
            return true;
        }
        if key == &Key::k(KeyCode::Tab) {
            self.dispatch_action(AppAction::FocusNext);
            return true;
        }
        if self.focus_pane == FocusPane::SectionRail {
            return false;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.news.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.news.move_selection(-1),

            KeyCode::Enter => self.focus_pane = FocusPane::Conversation,
            KeyCode::Esc => self.focus_pane = FocusPane::ChatList,
            // Let existing pane/leader/quit shortcuts retain their conventions.
            KeyCode::Char('h' | 'l' | ' ' | '?') | KeyCode::Left | KeyCode::Right => return false,
            _ if key != &Key::c('i')
                && key != &Key::c('/')
                && key != &Key::c('n')
                && key != &Key::c('N') =>
            {
                return false;
            }
            _ => {}
        }
        true
    }

    pub(crate) fn dispatch_news_navigation(&mut self, action: &AppAction) -> bool {
        if self.selected_section != Section::News || self.rail_on_logout {
            return false;
        }
        match action {
            AppAction::SelectNext
            | AppAction::SelectPrevious
            | AppAction::JumpTop
            | AppAction::JumpBottom
            | AppAction::HalfPageDown
            | AppAction::HalfPageUp
                if self.focus_pane == FocusPane::SectionRail =>
            {
                return false;
            }
            AppAction::SelectNext => self.news.move_selection(1),
            AppAction::SelectPrevious => self.news.move_selection(-1),
            AppAction::JumpTop => self.news.select(0),
            AppAction::JumpBottom => self.news.select(self.news.note_count() - 1),
            AppAction::HalfPageDown => self.news.scroll_by(10),
            AppAction::HalfPageUp => self.news.scroll_by(-10),
            AppAction::FocusNext
            | AppAction::FocusPrevious
            | AppAction::FocusPane(_)
            | AppAction::ToggleChatList
            | AppAction::ToggleSectionRail
            | AppAction::Quit
            | AppAction::Logout
            | AppAction::ToggleShortcutPopup
            | AppAction::ToggleComposerDirection
            | AppAction::PlannedLeaderAction(_) => return false,
            // News is read-only: never act on the retained open chat behind it.
            _ => {}
        }
        true
    }
}

#[cfg(test)]
#[path = "news_tests.rs"]
mod tests;
