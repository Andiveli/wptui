use ratatui::crossterm::event::KeyCode;

use super::App;
use super::actions::{FocusPane, Section};
use crate::key_handler::Key;

impl App<'_> {
    pub(crate) fn handle_chat_search_input(&mut self, key: Key) -> bool {
        if self.focus_pane == FocusPane::ChatList && self.contact_search_active {
            self.handle_chat_search_key(key);
            true
        } else {
            false
        }
    }

    pub(crate) fn start_chat_search(&mut self, key: &Key) -> bool {
        if self.focus_pane == FocusPane::ChatList
            && self.selected_section == Section::Chats
            && key.code == KeyCode::Char('/')
        {
            self.contact_search_active = true;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::TestApp;

    #[test]
    fn slash_starts_chat_search_only_in_the_chats_list() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::ChatList;
        app.selected_section = Section::Chats;

        assert!(app.start_chat_search(&Key::c('/')));
        assert!(app.contact_search_active);
    }

    #[test]
    fn active_chat_search_routes_characters_to_the_search_buffer() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::ChatList;
        app.contact_search_active = true;

        assert!(app.handle_chat_search_input(Key::c('a')));
        assert_eq!(app.contact_search.input.to_string(), "a");
    }
}
