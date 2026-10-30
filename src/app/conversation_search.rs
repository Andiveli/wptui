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
            if search.chat == self.open_chat.as_ref().unwrap().clone() {
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

    fn in_chat_history(&self) -> bool {
        self.focus_pane == FocusPane::Conversation
            && matches!(self.selected_section, Section::Chats | Section::Communities)
            && self.conversation_mode == ConversationMode::MessageNavigation
            && self.open_chat.is_some()
    }
}
