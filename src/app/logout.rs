use crate::input_key::KeyCode;

use crate::app::App;
use crate::app::actions::{ActionNotice, Section};
use crate::input_key::Key;
use whatsrust as wr;

fn logout_after_stopping_read_sync(stop_read_sync: impl FnOnce(), logout: impl FnOnce()) {
    stop_read_sync();
    logout();
}

impl App<'_> {
    pub(crate) fn handle_logout_input(&mut self, key: Key) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.pending_logout = false;
                self.confirm_logout();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.pending_logout = false;
                self.logout_menu_index = 0;
                self.action_notice = Some(ActionNotice::Cancelled);
            }
            KeyCode::Enter => {
                if self.logout_menu_index == 0 {
                    self.pending_logout = false;
                    self.confirm_logout();
                } else {
                    self.pending_logout = false;
                    self.logout_menu_index = 0;
                    self.action_notice = Some(ActionNotice::Cancelled);
                }
            }
            KeyCode::Char('j') | KeyCode::Char('k') | KeyCode::Down | KeyCode::Up => {
                self.logout_menu_index = (self.logout_menu_index + 1) % 2;
            }
            _ => {}
        }
    }

    pub(crate) fn begin_logout_confirmation(&mut self) {
        self.pending_logout = true;
        self.logout_menu_index = 0;
    }

    fn confirm_logout(&mut self) {
        self.pending_logout = true;
        self.logout_in_progress = true;
        logout_after_stopping_read_sync(|| self.shutdown_read_sync_worker(), wr::logout);
    }

    pub(crate) fn handle_logout_result(&mut self, status: wr::LogoutStatus) -> bool {
        match status {
            wr::LogoutStatus::LoggedOut | wr::LogoutStatus::NotLoggedIn => {
                self.finish_logout();
            }
            wr::LogoutStatus::LocalOnly => {
                log::warn!(
                    "Logout: device was not unlinked on the phone; remove it manually in WhatsApp → Linked devices"
                );
                self.reset_logout_prompt();
                self.unavailable(
                    "Logged out locally, but the device is still linked on the phone — remove it in WhatsApp (Settings → Linked devices), then log out again to finish",
                );
            }
            wr::LogoutStatus::Failed => {
                self.reset_logout_prompt();
                self.unavailable("Could not log out");
            }
        }
        true
    }

    pub(crate) fn finish_logout(&mut self) {
        self.reset_logout_prompt();
        self.db_handler.stop();
        wipe_sqlite_file(&self.whatsmeow_db);
        wipe_sqlite_file(&self.whatsmeow_db.with_file_name("whatsapp.db"));
        clear_media_dir(&self.media_path);
        self.should_quit = true;
    }

    fn reset_logout_prompt(&mut self) {
        self.pending_logout = false;
        self.logout_in_progress = false;
        self.logout_menu_index = 0;
    }

    pub(crate) fn move_logout_selection_next(&mut self) -> bool {
        if self.rail_on_logout {
            self.rail_on_logout = false;
            self.selected_section = Section::Chats;
            true
        } else if self.selected_section == Section::Communities {
            self.rail_on_logout = true;
            true
        } else {
            false
        }
    }

    pub(crate) fn move_logout_selection_previous(&mut self) -> bool {
        if self.rail_on_logout {
            self.rail_on_logout = false;
            self.selected_section = Section::Communities;
            true
        } else if self.selected_section == Section::Chats {
            self.rail_on_logout = true;
            true
        } else {
            false
        }
    }

    pub(crate) fn jump_logout_selection_top(&mut self) {
        self.selected_section = Section::Chats;
        self.rail_on_logout = false;
    }

    pub(crate) fn jump_logout_selection_bottom(&mut self) {
        self.selected_section = Section::Communities;
        self.rail_on_logout = true;
    }
}

fn wipe_sqlite_file(path: &std::path::Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut os = path.as_os_str().to_os_string();
        os.push(suffix);
        let file = std::path::PathBuf::from(os);
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => log::warn!("Failed to remove {}: {err}", file.display()),
        }
    }
}

fn clear_media_dir(media_path: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(media_path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let _ = std::fs::remove_dir_all(path);
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod ordering_tests {
    use std::cell::RefCell;

    use super::logout_after_stopping_read_sync;

    #[test]
    fn remote_logout_stops_read_sync_before_requesting_bridge_logout() {
        let events = RefCell::new(Vec::new());

        logout_after_stopping_read_sync(
            || events.borrow_mut().push("stop read sync"),
            || events.borrow_mut().push("request remote logout"),
        );

        assert_eq!(
            events.into_inner(),
            ["stop read sync", "request remote logout"]
        );
    }

    #[test]
    fn local_fallback_logout_stops_read_sync_before_requesting_bridge_logout() {
        let events = RefCell::new(Vec::new());

        logout_after_stopping_read_sync(
            || events.borrow_mut().push("stop read sync"),
            || {
                events
                    .borrow_mut()
                    .push("request logout that may return local fallback")
            },
        );

        assert_eq!(
            events.into_inner(),
            [
                "stop read sync",
                "request logout that may return local fallback"
            ]
        );
    }
}

#[cfg(test)]
mod tests;
