use ratatui::crossterm::event::KeyCode;

use super::App;
use super::actions::{AppAction, FocusPane, Section};
use crate::key_handler::Key;
use whatsrust as wr;

impl App<'_> {
    pub fn select_chat(&mut self, jid: Option<wr::JID>) {
        let rows = self.visible_chat_rows();
        crate::crash_diagnostics::breadcrumb("chat-selection", &format!("rows={}", rows.len()));
        if let Some(jid) = jid
            && let Some(index) = rows
                .iter()
                .position(|row| row.target == jid || row.members.contains(&jid))
        {
            self.chat_list_state.select(Some(index));
        } else if rows.is_empty() {
            self.chat_list_state.select(None);
        } else {
            self.chat_list_state.select(Some(0));
        }
    }

    pub(crate) fn update_filtered_chats(&mut self) {
        self.filtered_chats = self
            .visible_chat_rows()
            .into_iter()
            .map(|row| row.target)
            .collect();
        self.chat_list_state
            .select((!self.filtered_chats.is_empty()).then_some(0));
    }

    pub(crate) fn handle_chat_search_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => {
                let chat = self.get_selected_chat();
                self.contact_search_active = false;
                self.contact_search.clean();
                self.select_chat(chat);
            }
            KeyCode::Enter => {
                self.contact_search_active = false;
                self.dispatch_action(AppAction::OpenChat);
            }
            KeyCode::Char(character) => {
                self.contact_search.enter_char(character);
                self.update_filtered_chats();
            }
            KeyCode::Backspace => {
                self.contact_search.delete_char();
                self.update_filtered_chats();
            }
            KeyCode::Left => self.contact_search.move_cursor_left(),
            KeyCode::Right => self.contact_search.move_cursor_right(),
            _ => {}
        }
    }

    pub(crate) fn move_selection_next(&mut self) {
        if self.focus_pane == FocusPane::ChatList {
            self.move_chat_selection(1);
        }
    }

    pub(crate) fn move_selection_previous(&mut self) {
        if self.focus_pane == FocusPane::ChatList {
            self.move_chat_selection(-1);
        }
    }

    fn move_chat_selection(&mut self, delta: isize) {
        match self.selected_section {
            Section::Status => {
                if delta > 0 {
                    self.status_selection.select_next();
                } else {
                    self.status_selection.select_previous();
                }
                self.clamp_status_selection();
            }
            Section::Communities => {
                if delta > 0 {
                    self.chat_list_state.select_next();
                } else {
                    self.chat_list_state.select_previous();
                }
                self.clamp_community_selection();
            }
            Section::Chats => {
                if delta > 0 {
                    self.chat_list_state.select_next();
                } else {
                    self.chat_list_state.select_previous();
                }
                self.clamp_chat_selection();
            }
        }
    }

    pub(crate) fn jump_selection_top(&mut self) {
        match self.focus_pane {
            FocusPane::ChatList => match self.selected_section {
                Section::Status => {
                    self.status_selection.select_first();
                    self.clamp_status_selection();
                }
                Section::Chats | Section::Communities => self.chat_list_state.select_first(),
            },
            FocusPane::Conversation | FocusPane::SectionRail => {}
        }
    }

    pub(crate) fn jump_selection_bottom(&mut self) {
        match self.focus_pane {
            FocusPane::ChatList => match self.selected_section {
                Section::Status => {
                    self.status_selection.select_last();
                    self.clamp_status_selection();
                }
                Section::Chats | Section::Communities => self.chat_list_state.select_last(),
            },
            FocusPane::Conversation | FocusPane::SectionRail => {}
        }
        self.clamp_chat_selection();
    }

    fn clamp_chat_selection(&mut self) {
        let count = if self.contact_search.input.is_empty() {
            self.visible_chat_rows().len()
        } else {
            self.filtered_chats.len()
        };
        clamp_list_state(&mut self.chat_list_state, count);
    }

    fn clamp_community_selection(&mut self) {
        let count = self.selectable_community_nodes().len();
        clamp_list_state(&mut self.chat_list_state, count);
    }
}

fn clamp_list_state(state: &mut ratatui::widgets::ListState, count: usize) {
    match (count, state.selected()) {
        (0, _) => state.select(None),
        (len, Some(selected)) if selected >= len => state.select(Some(len - 1)),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
