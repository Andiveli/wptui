use std::{path::PathBuf, sync::Arc, thread::JoinHandle};

use crate::updater::news_update::{UpdateTarget, run_selected_update};
use crate::{
    app::{App, actions::StatusCompositionState},
    input_key::{Key, KeyCode},
};

pub type InstallSelected = dyn Fn(&UpdateTarget) -> Result<PathBuf, String> + Send + Sync;

pub struct NewsUpdateState {
    pub confirm: Option<UpdateTarget>,
    pub working: Option<UpdateTarget>,
    pub installed_path: Option<PathBuf>,
    pub restart_prompt: bool,
    pub affirmative: bool,
    pub status: String,
    pub install: Arc<InstallSelected>,
    worker: Option<JoinHandle<()>>,
    restart_requested: bool,
}
impl Default for NewsUpdateState {
    fn default() -> Self {
        Self {
            confirm: None,
            working: None,
            installed_path: None,
            restart_prompt: false,
            affirmative: false,
            status: String::new(),
            install: Arc::new(run_selected_update),
            worker: None,
            restart_requested: false,
        }
    }
}
impl NewsUpdateState {
    pub fn join(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
    pub fn controls(&self) -> String {
        if let Some(target) = &self.confirm {
            format!(
                "Install {}? {} · Up/Down: choose · Enter: confirm · Esc: cancel",
                target.version,
                if self.affirmative {
                    "[Install] / Cancel"
                } else {
                    "Install / [Cancel]"
                }
            )
        } else if self.restart_prompt {
            format!(
                "Update installed. {} · Up/Down: choose · Enter: confirm · Esc: Later",
                if self.affirmative {
                    "[Restart now] / Later"
                } else {
                    "Restart now / [Later]"
                }
            )
        } else {
            format!(
                "{} · u: install displayed release · R: restart installed update",
                self.status
            )
        }
    }
}
impl Drop for NewsUpdateState {
    fn drop(&mut self) {
        self.join();
    }
}

impl App<'_> {
    pub fn take_restart_path(&mut self) -> Option<PathBuf> {
        if self.news_update.restart_requested {
            self.news_update.installed_path.take()
        } else {
            None
        }
    }

    pub(crate) fn restart_has_pending_work(&self) -> bool {
        // is_empty includes attachments, including whitespace-only text drafts.
        // Any status context also protects its private saved chat draft.
        !self.composer.is_empty()
            || self.composer.quote.is_some()
            || self.composer.status_draft.is_some()
            || self.status_composition != StatusCompositionState::Inactive
            || self.edit_message.is_some()
            || !self.pending_outgoing_text.is_empty()
            || self.pending_status_sends != 0
            || !self.pending_outgoing_status.is_empty()
            || self.logout_in_progress
            || self.pending_logout
    }

    pub(crate) fn handle_news_update_key(&mut self, key: &Key) -> bool {
        // Preserve ordinary quit even while our voluntary choices are showing.
        if key == &Key::ctrl('q') {
            return false;
        }
        if self.news_update.confirm.is_some() || self.news_update.restart_prompt {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.news_update.affirmative = true,
                KeyCode::Down | KeyCode::Char('j') => self.news_update.affirmative = false,
                KeyCode::Esc => {
                    self.news_update.confirm = None;
                    self.news_update.restart_prompt = false;
                    self.news_update.status = if self.news_update.installed_path.is_some() {
                        "Update installed. Restart deferred — press R to restart later".into()
                    } else {
                        "Update cancelled".into()
                    };
                }
                KeyCode::Enter => {
                    let affirmative = self.news_update.affirmative;
                    if let Some(target) = self.news_update.confirm.take() {
                        if affirmative {
                            self.start_news_update(target);
                        } else {
                            self.news_update.status = "Update cancelled".into();
                        }
                    } else {
                        self.news_update.restart_prompt = false;
                        if affirmative {
                            if self.restart_has_pending_work() {
                                self.news_update.status = "Restart deferred: finish or discard drafts or pending sends, then press R".into();
                            } else {
                                self.news_update.restart_requested = true;
                                self.should_quit = true;
                            }
                        } else {
                            self.news_update.status = "Update installed. Still running the old version — press R to restart later".into();
                        }
                    }
                }
                _ => {}
            }
            return true;
        }
        if key == &Key::c('R') && self.news_update.installed_path.is_some() {
            self.news_update.restart_prompt = true;
            self.news_update.affirmative = false;
            return true;
        }
        if key != &Key::c('u') {
            return false;
        }
        if self.news_update.working.is_some() || self.news_update.installed_path.is_some() {
            return true;
        }
        if self.news.selected == 0 {
            self.news_update.status = "Installed-version notes cannot be installed".into();
            return true;
        }
        let note = self.news.selected_note();
        let target = UpdateTarget {
            tag: note.tag.clone(),
            version: note.version.clone(),
        };
        if let Err(error) = target.validate() {
            self.news_update.status = error;
        } else {
            self.news_update.status = format!("Confirm installation of {}", target.version);
            self.news_update.confirm = Some(target);
            self.news_update.affirmative = false;
        }
        true
    }

    fn start_news_update(&mut self, target: UpdateTarget) {
        if self.news_update.working.is_some() {
            return;
        }
        self.news_update.join();
        self.news_update.working = Some(target.clone());
        self.news_update.status = format!(
            "Working: checking, downloading and installing {}…",
            target.version
        );
        let install = Arc::clone(&self.news_update.install);
        let tx = self.tx.clone();
        match std::thread::Builder::new().name("news-update".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| install(&target)))
                .unwrap_or_else(|_| Err("Update worker stopped unexpectedly; verify the installed binary before retrying".into()));
            let _ = tx.send(crate::app::events::AppInput::App(
                crate::app::events::AppEvent::NewsUpdateFinished { target, result }));
        }) {
            Ok(worker) => self.news_update.worker = Some(worker),
            Err(error) => {
                self.news_update.working = None;
                self.news_update.status = format!("Could not start updater: {error}");
            }
        }
    }

    pub(crate) fn finish_news_update(
        &mut self,
        target: UpdateTarget,
        result: Result<PathBuf, String>,
    ) {
        if self.news_update.working.as_ref() != Some(&target) {
            return;
        }
        self.news_update.join();
        self.news_update.working = None;
        match result {
            Ok(path) => {
                self.news_update.installed_path = Some(path);
                self.news_update.status =
                    format!("Installed {}. Restart now / Later", target.version);
                self.news_update.restart_prompt = true;
                self.news_update.affirmative = false;
            }
            Err(error) => self.news_update.status = format!("Update failed: {error}"),
        }
    }

    pub(crate) fn join_news_workers(&mut self) {
        self.news_update.join();
        self.news.join();
    }
}

#[cfg(test)]
#[path = "news_update_tests.rs"]
mod tests;
