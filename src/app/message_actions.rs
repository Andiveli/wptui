use std::path::PathBuf;
use std::sync::Arc;

use super::{App, MessageMenuAction};
use crate::app::media_support::remove_owned_media_files;
use whatsrust as wr;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessageActionKind {
    Edit { replacement: Arc<str> },
    Delete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageAction {
    pub action_id: Arc<str>,
    pub target_message_id: wr::MessageId,
    pub chat: wr::JID,
    pub sender: wr::JID,
    pub kind: MessageActionKind,
    pub occurred_at: i64,
    pub arrival_order: u64,
}

pub const DELETED_MESSAGE_TEXT: &str = "This message was deleted.";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MessageStatus {
    pub edited: bool,
    pub deleted: bool,
}

fn status_for_actions(actions: &[MessageAction]) -> MessageStatus {
    MessageStatus {
        edited: actions
            .iter()
            .any(|action| matches!(action.kind, MessageActionKind::Edit { .. })),
        deleted: actions
            .iter()
            .any(|action| matches!(action.kind, MessageActionKind::Delete)),
    }
}

fn sorted_actions(mut actions: Vec<MessageAction>) -> Vec<MessageAction> {
    actions.sort_by(|left, right| {
        (left.occurred_at, left.arrival_order, &left.action_id).cmp(&(
            right.occurred_at,
            right.arrival_order,
            &right.action_id,
        ))
    });
    actions
}

impl App<'_> {
    pub fn selected_message(&self) -> Option<&wr::Message> {
        self.message_list_state
            .get_selected_message()
            .and_then(|message_id| self.messages.get(&message_id))
    }

    pub fn message_status(&self, message_id: &wr::MessageId) -> MessageStatus {
        self.message_actions
            .get(message_id)
            .map(|actions| status_for_actions(actions))
            .unwrap_or_default()
    }

    pub(crate) fn sorted_message_actions(&self, message_id: &wr::MessageId) -> Vec<MessageAction> {
        sorted_actions(
            self.message_actions
                .get(message_id)
                .cloned()
                .unwrap_or_default(),
        )
    }

    pub(crate) fn pending_local_action_for(&self, action: &MessageAction) -> Option<Arc<str>> {
        if action.action_id.starts_with("local-")
            || !self
                .messages
                .get(&action.target_message_id)
                .is_some_and(|message| message.info.is_from_me)
        {
            return None;
        }
        let prefix = match &action.kind {
            MessageActionKind::Edit { .. } => "local-edit:",
            MessageActionKind::Delete => "local-delete:",
        };
        let matches = self
            .message_actions
            .get(&action.target_message_id)?
            .iter()
            .filter(|local| {
                local.action_id.starts_with(prefix)
                    && local.chat == action.chat
                    && local.kind == action.kind
            });
        let mut matches = matches.map(|local| local.action_id.clone());
        let local = matches.next()?;
        matches.next().is_none().then_some(local)
    }

    pub(crate) fn refresh_message_projection(
        &mut self,
        id: &wr::MessageId,
    ) -> Option<&'static str> {
        let Some(current) = self.messages.get(id).cloned() else {
            return None;
        };
        let mut projected = current;
        if self.message_status(id).deleted {
            if let wr::MessageContent::File(file) = &projected.message {
                remove_owned_media_files(&self.media_path, &[PathBuf::from(file.path.as_ref())]);
            }
            projected.message = wr::MessageContent::Text(DELETED_MESSAGE_TEXT.into());
            projected.info.quote_id = None;
            projected.info.forwarding = Default::default();
            self.metadata.remove(id);
            self.image_cache.remove(id);
        } else if let wr::MessageContent::Text(body) = &mut projected.message {
            for action in self.sorted_message_actions(id) {
                if let MessageActionKind::Edit { replacement } = action.kind
                    && !replacement.is_empty()
                {
                    *body = replacement;
                }
            }
        }
        self.messages.insert(id.clone(), projected.clone());
        self.message_height_cache.invalidate(id);
        if self
            .message_actions
            .get(id)
            .is_some_and(|actions| !actions.is_empty())
        {
            self.db_handler.add_message(&projected);
        }
        Some("refreshed")
    }

    pub fn follow_selected_reference(&mut self) -> bool {
        let Some(reference_id) = self
            .selected_message()
            .and_then(|message| message.info.quote_id.clone())
            .filter(|reference_id| self.messages.contains_key(reference_id))
        else {
            return false;
        };

        self.message_list_state.set_selected_message(reference_id);
        self.message_list_state.update_selected = true;
        true
    }

    pub fn message_menu_actions(&self) -> Option<Vec<MessageMenuAction>> {
        self.message_menu
            .as_ref()
            .map(|(actions, _)| actions.clone())
    }
}

#[cfg(test)]
mod tests;
