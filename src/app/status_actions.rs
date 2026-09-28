use crate::app::App;
use crate::app::actions::{
    ActionNotice, ComposerAction, ConversationMode, FocusPane, STATUS_REACTION, Section,
    StatusCompositionState,
};
use crate::app::composer::ComposerOutcome;
use crate::app::composer_input_mapping::composer_action_for_editing_key;
use crate::app::composer_input_paste::apply_clipboard_paste;
use crate::input_key::{Key, KeyCode};
use whatsrust as wr;

pub(crate) fn status_content_is_supported(content: &wr::MessageContent) -> bool {
    matches!(
        content,
        wr::MessageContent::Text(_)
            | wr::MessageContent::File(wr::FileContent {
                kind: wr::FileKind::Image | wr::FileKind::Video,
                ..
            })
    )
}

pub(crate) fn status_attachment_is_supported(kind: &wr::FileKind) -> bool {
    matches!(kind, wr::FileKind::Image | wr::FileKind::Video)
}

impl App<'_> {
    pub(crate) fn start_status_composition(&mut self) {
        if self.selected_section == Section::Status && self.focus_pane == FocusPane::ChatList {
            self.message_list_state.reset();
            self.composer.set_blocked(false);
            self.status_composition = StatusCompositionState::Authoring;
        }
    }

    pub(crate) fn cancel_status_composition(&mut self) {
        self.status_composition = StatusCompositionState::Inactive;
        self.file_picker = None;
        self.composer.replace_text("");
        self.composer.apply(ComposerAction::CancelReply);
        self.composer.pending.clear();
        self.composer.set_blocked(self.composer_blocked());
        self.action_notice = Some(ActionNotice::Cancelled);
    }

    pub(crate) fn handle_status_composer_input(&mut self, key: Key) -> bool {
        if self.file_picker.is_some() {
            return false;
        }
        if self.status_composition == StatusCompositionState::Navigating {
            if key == Key::k(KeyCode::Esc) {
                self.cancel_status_composition();
            } else if key == Key::c('i') {
                self.status_composition = StatusCompositionState::Authoring;
            } else {
                match self.kh.resolve(key) {
                    crate::keybindings::SequenceResolution::Complete(action)
                        if matches!(
                            action,
                            crate::app::actions::AppAction::SelectNext
                                | crate::app::actions::AppAction::SelectPrevious
                                | crate::app::actions::AppAction::JumpTop
                                | crate::app::actions::AppAction::JumpBottom
                                | crate::app::actions::AppAction::HalfPageDown
                                | crate::app::actions::AppAction::HalfPageUp
                                | crate::app::actions::AppAction::Quit
                                | crate::app::actions::AppAction::ToggleLogs
                        ) =>
                    {
                        self.dispatch_action(action)
                    }
                    _ => {}
                }
            }
            return true;
        }
        if self.status_composition != StatusCompositionState::Authoring {
            return false;
        }
        if key == Key::k(KeyCode::Esc) {
            self.status_composition = StatusCompositionState::Navigating;
            self.focus_pane = FocusPane::Conversation;
            if self.status_message_count() > 0 && self.message_list_state.selected.is_none() {
                self.message_list_state.select(Some(0));
            }
        } else if key == Key::ctrl('o') {
            self.dispatch_file_picker_action(crate::app::actions::AppAction::AttachFile);
        } else {
            self.dispatch_status_composer_action(composer_action_for_editing_key(&key));
        }
        true
    }

    pub(crate) fn dispatch_status_composer_action(&mut self, action: ComposerAction) {
        if self.status_composition != StatusCompositionState::Authoring {
            return;
        }
        if matches!(action, ComposerAction::Paste) {
            let paste = self.clipboard_reader.read_paste();
            if let Err(error) = apply_clipboard_paste(&mut self.composer, &self.media_path, paste) {
                self.unavailable(&format!("Could not paste clipboard content: {error:?}"));
            }
            self.reject_unsupported_status_attachments();
            return;
        }
        if let ComposerOutcome::Submit { messages, .. } = self.composer.apply_with_direction(
            action,
            self.composer_direction,
            self.composer_viewport_width,
        ) {
            self.submit_status_messages(messages);
        }
    }

    pub(crate) fn reject_unsupported_status_attachments(&mut self) {
        let pending_before = self.composer.pending.len();
        self.composer
            .pending
            .retain(|attachment| status_attachment_is_supported(&attachment.kind));
        if self.composer.pending.len() != pending_before {
            self.action_notice = Some(ActionNotice::Unsupported(
                "Statuses support only text, images, and videos".into(),
            ));
        }
    }

    fn submit_status_messages(&mut self, messages: Vec<wr::MessageContent>) {
        if messages
            .iter()
            .any(|content| !status_content_is_supported(content))
        {
            self.action_notice = Some(ActionNotice::Unsupported(
                "Statuses support only text, images, and videos".into(),
            ));
            return;
        }

        self.pending_status_sends = 0;
        self.status_send_failure = None;
        for content in messages {
            if self.status_send_worker.enqueue(content) {
                self.pending_status_sends += 1;
            }
        }
        if self.pending_status_sends == 0 {
            self.unavailable("Could not publish status");
            return;
        }
        self.status_composition = StatusCompositionState::Submitting;
    }

    pub(crate) fn status_send_succeeded(&mut self) -> bool {
        self.finish_status_send(None)
    }

    pub(crate) fn status_send_failed(&mut self, result: wr::StatusSendResult) -> bool {
        self.finish_status_send(Some(result))
    }

    fn finish_status_send(&mut self, failure: Option<wr::StatusSendResult>) -> bool {
        if self.status_composition != StatusCompositionState::Submitting
            || self.pending_status_sends == 0
        {
            return false;
        }
        self.pending_status_sends -= 1;
        if self.status_send_failure.is_none() {
            self.status_send_failure = failure;
        }
        if self.pending_status_sends != 0 {
            return false;
        }
        if let Some(result) = self.status_send_failure.take() {
            self.status_composition = StatusCompositionState::Authoring;
            self.unavailable(&format!("Could not publish status: {result:?}"));
        } else {
            self.status_composition = StatusCompositionState::Inactive;
            self.composer.set_blocked(self.composer_blocked());
            self.action_notice = Some(ActionNotice::StatusPublished);
        }
        true
    }

    /// Reply from a status: switches to the contact's private chat with
    /// the status quoted, so the answer lands in the inbox.
    pub(crate) fn reply_to_status(&mut self) {
        let Some(message) = self.selected_message().cloned() else {
            return self.unavailable("Reply is not available");
        };
        let contact = message.info.sender.clone();
        self.selected_section = Section::Chats;
        self.open_chat = Some(contact.clone());
        self.sort_chat_messages(contact);
        self.message_list_state.reset();
        self.composer.quote = Some(message);
        self.conversation_mode = ConversationMode::ComposerEditing;
        self.focus_pane = FocusPane::Conversation;
    }

    /// Reacts to the selected status with a heart directly. Statuses do not
    /// open the general reaction picker because WhatsApp only allows the heart.
    pub(crate) fn heart_selected_status(&mut self) {
        let Some(message) = self.selected_message().cloned() else {
            return self.unavailable("Reaction is not available");
        };
        if self
            .message_reactor
            .react_to_message_in_chat(
                &message.info.chat,
                &message.info.chat,
                &message.info.sender,
                &message.info.id,
                STATUS_REACTION,
            )
            .is_ok()
        {
            self.action_notice = Some(ActionNotice::Reacted);
        } else {
            self.unavailable("Could not react to message");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::{ActionNotice, ConversationMode};
    use crate::app::test_support::TestApp;

    fn status_message(id: &str) -> wr::Message {
        wr::Message {
            info: wr::MessageInfo {
                id: id.into(),
                chat: "status@broadcast".to_owned().into(),
                sender: "alice@s.whatsapp.net".to_owned().into(),
                mentions_self: false,
                timestamp: 100,
                forwarding: Default::default(),
                is_from_me: false,
                quote_id: None,
                read_by: 0,
            },
            message: wr::MessageContent::Text("status".into()),
        }
    }

    #[test]
    fn reply_moves_status_into_the_contact_chat_with_a_quote() {
        let mut app = TestApp::new();
        let message = status_message("status-1");
        let sender = message.info.sender.clone();
        app.add_message(message);
        app.message_list_state
            .set_selected_message("status-1".into());

        app.reply_to_status();

        assert_eq!(app.selected_section, Section::Chats);
        assert_eq!(app.open_chat(), Some(sender));
        assert_eq!(app.focus_pane, FocusPane::Conversation);
        assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
        assert_eq!(
            app.composer
                .quote
                .as_ref()
                .map(|item| item.info.id.as_ref()),
            Some("status-1")
        );
    }

    #[test]
    fn reaction_without_a_selected_status_reports_unavailability() {
        let mut app = TestApp::new();

        app.heart_selected_status();

        assert!(matches!(
            &app.action_notice,
            Some(ActionNotice::Unavailable(message)) if message == "Reaction is not available"
        ));
    }

    #[test]
    fn status_rejects_unsupported_media_kinds() {
        for kind in [
            wr::FileKind::Audio,
            wr::FileKind::Document,
            wr::FileKind::Sticker,
        ] {
            assert!(!status_attachment_is_supported(&kind), "{kind:?}");
        }
        assert!(!status_content_is_supported(
            &wr::MessageContent::ViewOnceUnavailable
        ));
    }

    #[test]
    fn typed_status_events_update_the_lifecycle_and_notice() {
        let mut app = TestApp::new();
        app.status_composition = StatusCompositionState::Submitting;
        app.pending_status_sends = 1;
        assert!(app.status_send_succeeded());
        assert_eq!(app.status_composition, StatusCompositionState::Inactive);
        assert_eq!(app.action_notice, Some(ActionNotice::StatusPublished));

        app.status_composition = StatusCompositionState::Submitting;
        app.pending_status_sends = 1;
        assert!(app.status_send_failed(wr::StatusSendResult::SendFailed));
        assert_eq!(app.status_composition, StatusCompositionState::Authoring);
        assert!(matches!(
            app.action_notice,
            Some(ActionNotice::Unavailable(_))
        ));
    }
}
