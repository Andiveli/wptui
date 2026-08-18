use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use crate::app::App;
use crate::key_handler::Key;

impl App<'_> {
    pub(crate) fn toggle_logs(&mut self) {
        self.show_logs = !self.show_logs;
        tui_logger::set_default_level(log_level_for_logs(self.show_logs));
    }
}

pub(crate) fn is_toggle_logs_key(key: &Key) -> bool {
    matches!(key.code, KeyCode::Char('l' | 'L'))
        && key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT
}

fn log_level_for_logs(show_logs: bool) -> tui_logger::LevelFilter {
    if show_logs {
        tui_logger::LevelFilter::Info
    } else {
        tui_logger::LevelFilter::Warn
    }
}

#[cfg(test)]
mod tests {
    use super::{is_toggle_logs_key, log_level_for_logs};
    use crate::key_handler::Key;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};
    use tui_logger::LevelFilter;

    #[test]
    fn recognizes_both_log_toggle_key_cases_only_with_ctrl_shift() {
        for code in [KeyCode::Char('l'), KeyCode::Char('L')] {
            assert!(is_toggle_logs_key(&Key {
                code,
                modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            }));
        }
        assert!(!is_toggle_logs_key(&Key::c('l')));
    }

    #[test]
    fn log_panel_uses_info_and_restores_warn_when_closed() {
        assert_eq!(log_level_for_logs(true), LevelFilter::Info);
        assert_eq!(log_level_for_logs(false), LevelFilter::Warn);
    }
}
