use ratatui::crossterm::event::KeyCode;

use crate::app::App;
use crate::app::actions::{AppAction, ComposerAction, ConversationMode, FocusPane, Section};
use crate::app::composer_input_mapping::composer_action_for_editing_key;
use crate::app::composer_input_paste::apply_clipboard_paste;
use crate::key_handler::Key;
use whatsrust as wr;

impl App<'_> {
    /// Handles the composer-owned part of terminal input.
    ///
    /// Returns `true` when the composer owns the current input context.
    pub(crate) fn handle_composer_input(&mut self, key: Key) -> bool {
        if self.focus_pane != FocusPane::Conversation
            || self.selected_section != Section::Chats
            || !matches!(
                self.conversation_mode,
                ConversationMode::ComposerEditing | ConversationMode::EditingMessage
            )
        {
            return false;
        }

        if key == Key::k(KeyCode::Esc) && !self.composer.mention_picker_active() {
            if self.conversation_mode == ConversationMode::EditingMessage {
                self.cancel_message_edit();
            } else {
                self.composer.apply(ComposerAction::CancelReply);
                self.composer.pending.clear();
                self.conversation_mode = ConversationMode::MessageNavigation;
            }
        } else if key == Key::ctrl('o') {
            self.dispatch_action(AppAction::AttachFile);
        } else if self.composer.mention_picker_active() {
            match key.code {
                KeyCode::Esc => self.composer.cancel_mention_picker(),
                KeyCode::Enter => self.composer.confirm_mention(),
                KeyCode::Up => self.composer.move_mention_selection(-1),
                KeyCode::Down => self.composer.move_mention_selection(1),
                _ => self.dispatch_composer_action(composer_action_for_editing_key(&key)),
            }
        } else if !self.composer_blocked() {
            self.dispatch_composer_action(composer_action_for_editing_key(&key));
        }

        true
    }

    pub(crate) fn dispatch_composer_action(&mut self, action: ComposerAction) {
        if self.composer_blocked() {
            return;
        }
        if self.conversation_mode == ConversationMode::EditingMessage
            && matches!(action, ComposerAction::Submit)
        {
            return self.submit_message_edit();
        }
        match action {
            ComposerAction::StartEdit => {
                // InsertMode is now the canonical way; StartEdit is unused.
            }
            ComposerAction::Paste => {
                let paste = self.clipboard_reader.read_paste();
                if let Err(error) =
                    apply_clipboard_paste(&mut self.composer, &self.media_path, paste)
                {
                    self.unavailable(&format!("Could not paste clipboard content: {error:?}"));
                }
            }
            action => match self.composer.apply(action) {
                crate::app::composer::ComposerOutcome::Idle => {}
                crate::app::composer::ComposerOutcome::Submit {
                    messages,
                    quote,
                    mentions,
                } => {
                    if let Some(chat) = self.open_chat() {
                        for message in messages {
                            wr::send_message(&chat, &message, quote.as_ref(), &mentions);
                        }
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::TestApp;

    #[test]
    fn non_composer_context_is_left_for_the_central_router() {
        let mut app = TestApp::new();
        assert!(!app.handle_composer_input(Key::c('x')));

        assert_eq!(app.conversation_mode, ConversationMode::MessageNavigation);
        assert!(app.composer.text().is_empty());
    }

    #[test]
    fn composer_context_consumes_blocked_input_without_mutation() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.selected_section = Section::Chats;
        app.conversation_mode = ConversationMode::ComposerEditing;
        app.composer.set_blocked(true);

        assert!(app.handle_composer_input(Key::c('x')));
        assert!(app.composer.text().is_empty());
    }
}
