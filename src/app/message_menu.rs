use super::App;
use crate::app::actions::MessageMenuAction;
use whatsrust as wr;

impl App<'_> {
    pub(crate) fn open_message_menu(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let Some(message) = self.selected_message() else {
            return self.unavailable("Menu is not available");
        };
        let mut actions = vec![MessageMenuAction::Reply];
        if matches!(message.message, wr::MessageContent::Text(_)) {
            actions.insert(0, MessageMenuAction::CopyText);
        }
        if message.info.quote_id.is_some() {
            actions.push(MessageMenuAction::GoToReference);
        }
        if self.contacts.contains_key(&message.info.sender) {
            actions.push(MessageMenuAction::SenderDetails);
        }
        if self
            .reactions
            .get(&message.info.id)
            .is_some_and(|items| !items.is_empty())
        {
            actions.push(MessageMenuAction::ReactedUsers);
        }
        self.message_menu = Some((actions, 0));
    }

    pub(crate) fn confirm_message_menu(&mut self) {
        let action = self
            .message_menu
            .take()
            .and_then(|(actions, selected)| actions.get(selected).copied());
        match action {
            Some(MessageMenuAction::CopyText) => self.copy_selected_text(),
            Some(MessageMenuAction::Reply) => self.reply_to_selected(),
            Some(MessageMenuAction::GoToReference) if !self.follow_selected_reference() => {
                self.unavailable("Reference is not available")
            }
            Some(MessageMenuAction::GoToReference) | None => {}
            Some(MessageMenuAction::SenderDetails) => {
                if let Some(message) = self.selected_message()
                    && let Some(name) = self.contacts.get(&message.info.sender)
                {
                    self.action_notice = Some(crate::app::actions::ActionNotice::SenderDetails(
                        name.to_string(),
                    ));
                }
            }
            Some(MessageMenuAction::ReactedUsers) => {
                if let Some(message) = self.selected_message() {
                    let users = self
                        .reactions
                        .get(&message.info.id)
                        .into_iter()
                        .flat_map(|items| items.keys())
                        .map(|jid| self.contact_name(jid).to_string())
                        .collect();
                    self.action_notice =
                        Some(crate::app::actions::ActionNotice::ReactedUsers(users));
                }
            }
        }
    }

    pub(crate) fn move_menu(&mut self, delta: isize) {
        if let Some((actions, selected)) = &mut self.message_menu {
            *selected = selected.saturating_add_signed(delta).min(actions.len() - 1);
        }
    }
}
