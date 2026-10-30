use whatsrust as wr;

use super::App;
use super::actions::{ConversationMode, FocusPane, Section};
use crate::input_key::{Key, KeyCode};

pub(crate) struct ConversationSearch {
    pub(crate) chat: wr::JID,
    pub(crate) query: String,
    pub(crate) editing: bool,
}

impl App<'_> {
    pub(crate) fn handle_conversation_search_input(&mut self, key: Key) -> bool {
        if !self.in_chat_history() {
            return false;
        }

        if let Some(search) = &mut self.conversation_search {
            if self.open_chat.as_ref() == Some(&search.chat) {
                if search.editing {
                    match key.code {
                        KeyCode::Esc => self.conversation_search = None,
                        KeyCode::Enter => {
                            if search.query.is_empty() {
                                self.conversation_search = None;
                            } else {
                                search.editing = false;
                            }
                        }
                        KeyCode::Char(character) => search.query.push(character),
                        KeyCode::Backspace => {
                            search.query.pop();
                        }
                        _ => {}
                    }
                    return true;
                }
                if key == Key::k(KeyCode::Esc) {
                    self.conversation_search = None;
                    return true;
                }
                if key == Key::c('n') || key == Key::c('N') {
                    self.jump_to_search_match(key == Key::c('n'));
                    return true;
                }
            }
        }

        if key == Key::c('/') {
            self.conversation_search = self.open_chat.as_ref().map(|chat| ConversationSearch {
                chat: chat.clone(),
                query: String::new(),
                editing: true,
            });
            self.kh.key_buffer.clear();
            self.kh.key_sequence_active = false;
            return true;
        }
        false
    }

    fn jump_to_search_match(&mut self, toward_older: bool) {
        let Some(search) = self.conversation_search.as_ref() else {
            return;
        };
        let Some(ids) = self.chat_messages.get(&search.chat) else {
            return;
        };
        let matches = ids
            .iter()
            .rev()
            .filter(|id| self.messages.contains_key(*id))
            .enumerate()
            .filter_map(|(index, id)| match &self.messages[id].message {
                wr::MessageContent::Text(body) if body.contains(&search.query) => Some(index),
                _ => None,
            })
            .collect::<Vec<_>>();
        let current = self.message_list_state.selected;
        let next = if toward_older {
            matches
                .iter()
                .copied()
                .find(|index| current.is_none_or(|current| *index > current))
                .or_else(|| matches.first().copied())
        } else {
            matches
                .iter()
                .copied()
                .rev()
                .find(|index| current.is_none_or(|current| *index < current))
                .or_else(|| matches.last().copied())
        };
        if let Some(index) = next {
            self.message_list_state.select(Some(index));
        }
    }

    fn in_chat_history(&self) -> bool {
        self.focus_pane == FocusPane::Conversation
            && matches!(self.selected_section, Section::Chats | Section::Communities)
            && self.conversation_mode == ConversationMode::MessageNavigation
            && self.open_chat.is_some()
    }
}
