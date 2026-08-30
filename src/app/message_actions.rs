use std::path::PathBuf;
use std::sync::Arc;

use super::{App, MessageMenuAction};
use crate::app::media_support::remove_owned_media_files;
use crate::app::message_action_diagnostics::identifier_for_log;
use crate::app::notifications::now_or;
use crate::db::MessageActionPersistence;
use log::info;
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

#[derive(Clone, Debug, Eq, PartialEq)]
enum MessageActionPersistenceIntent {
    Record,
    Reconcile { local_action_id: Arc<str> },
}

#[derive(Clone, Debug)]
struct MessageActionDiagnostic {
    base_exists: bool,
    persistence: MessageActionPersistence,
    reconciliation_attempted: bool,
    action_count: usize,
    projection: &'static str,
}

#[derive(Clone, Debug)]
struct MessageActionProjection {
    actions: Option<Vec<MessageAction>>,
    message: Option<wr::Message>,
    media_files: Vec<PathBuf>,
    remove_metadata: bool,
    invalidate_image_cache: bool,
    invalidate_message_height: bool,
    invalidate_chat_list: bool,
    writeback: Option<wr::Message>,
    diagnostic: Option<MessageActionDiagnostic>,
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

fn persistence_intent_for(pending_local: Option<Arc<str>>) -> MessageActionPersistenceIntent {
    match pending_local {
        Some(local_action_id) => MessageActionPersistenceIntent::Reconcile { local_action_id },
        None => MessageActionPersistenceIntent::Record,
    }
}

fn project_message_action(
    current: Option<&wr::Message>,
    current_actions: &[MessageAction],
    action: &MessageAction,
    pending_local: Option<Arc<str>>,
    persistence: MessageActionPersistence,
) -> MessageActionProjection {
    let base_exists = current.is_some();
    let reconciliation_attempted = pending_local.is_some();
    if matches!(persistence, MessageActionPersistence::DuplicateActionID) {
        return MessageActionProjection {
            actions: None,
            message: None,
            media_files: Vec::new(),
            remove_metadata: false,
            invalidate_image_cache: false,
            invalidate_message_height: false,
            invalidate_chat_list: false,
            writeback: None,
            diagnostic: None,
        };
    }

    let mut actions = current_actions.to_vec();
    match persistence {
        MessageActionPersistence::Inserted => actions.push(action.clone()),
        MessageActionPersistence::Reconciled => {
            let local_action_id = pending_local.expect("reconciliation requires local action");
            actions.retain(|existing| existing.action_id != local_action_id);
            actions.push(action.clone());
        }
        MessageActionPersistence::DuplicateActionID => unreachable!(),
    }
    if matches!(action.kind, MessageActionKind::Delete) {
        actions.retain(|existing| matches!(existing.kind, MessageActionKind::Delete));
    }

    let (message, media_files, remove_metadata, invalidate_image_cache, projection) =
        project_message(current, &actions);
    let action_count = actions.len();
    let writeback = message.clone();
    MessageActionProjection {
        actions: Some(actions),
        message,
        media_files,
        remove_metadata,
        invalidate_image_cache,
        invalidate_message_height: base_exists,
        invalidate_chat_list: true,
        writeback,
        diagnostic: Some(MessageActionDiagnostic {
            base_exists,
            persistence,
            reconciliation_attempted,
            action_count,
            projection,
        }),
    }
}

fn project_message(
    current: Option<&wr::Message>,
    actions: &[MessageAction],
) -> (Option<wr::Message>, Vec<PathBuf>, bool, bool, &'static str) {
    let Some(current) = current else {
        return (None, Vec::new(), false, false, "unchanged");
    };
    let mut projected = current.clone();
    if status_for_actions(actions).deleted {
        let media_files = match &projected.message {
            wr::MessageContent::File(file) => vec![PathBuf::from(file.path.as_ref())],
            _ => Vec::new(),
        };
        projected.message = wr::MessageContent::Text(DELETED_MESSAGE_TEXT.into());
        projected.info.quote_id = None;
        projected.info.forwarding = Default::default();
        (Some(projected), media_files, true, true, "refreshed")
    } else {
        if let wr::MessageContent::Text(body) = &mut projected.message {
            for action in sorted_actions(actions.to_vec()) {
                if let MessageActionKind::Edit { replacement } = action.kind
                    && !replacement.is_empty()
                {
                    *body = replacement;
                }
            }
        }
        (Some(projected), Vec::new(), false, false, "refreshed")
    }
}

impl App<'_> {
    pub fn apply_message_action(&mut self, action: MessageAction) {
        let target = action.target_message_id.clone();
        let pending_local = self.pending_local_action_for(&action);
        let persistence_intent = persistence_intent_for(pending_local.clone());
        let persistence = match persistence_intent {
            MessageActionPersistenceIntent::Record => {
                self.db_handler.record_message_action(&action)
            }
            MessageActionPersistenceIntent::Reconcile {
                ref local_action_id,
            } => self
                .db_handler
                .reconcile_message_action(local_action_id, &action),
        };
        let projection = project_message_action(
            self.messages.get(&target),
            self.message_actions.get(&target).map_or(&[], Vec::as_slice),
            &action,
            pending_local,
            persistence,
        );
        self.execute_message_action_projection(&target, projection, &action);
    }

    fn execute_message_action_projection(
        &mut self,
        target: &wr::MessageId,
        projection: MessageActionProjection,
        action: &MessageAction,
    ) {
        if let Some(actions) = projection.actions {
            self.message_actions.insert(target.clone(), actions);
        }
        if !projection.media_files.is_empty() {
            remove_owned_media_files(&self.media_path, &projection.media_files);
        }
        if let Some(message) = projection.message {
            if projection.remove_metadata {
                self.metadata.remove(target);
            }
            if projection.invalidate_image_cache {
                self.image_cache.remove(target);
            }
            self.messages.insert(target.clone(), message);
        }
        if projection.invalidate_message_height {
            self.message_height_cache.invalidate(target);
        }
        if let Some(message) = projection.writeback.as_ref() {
            self.db_handler.add_message(message);
        }
        if projection.invalidate_chat_list {
            self.invalidate_chat_list();
        }
        if let Some(diagnostic) = projection.diagnostic {
            let kind = match &action.kind {
                MessageActionKind::Edit { .. } => "edit",
                MessageActionKind::Delete => "delete",
            };
            self.message_action_diagnostics.record(|| {
                format!(
                    "source=rust kind={kind} action_id={} target_id={} base_exists={} persistence={:?} reconciliation={} action_count={} projection={}",
                    identifier_for_log(&action.action_id),
                    identifier_for_log(target),
                    diagnostic.base_exists,
                    diagnostic.persistence,
                    if diagnostic.reconciliation_attempted { "attempted" } else { "none" },
                    diagnostic.action_count,
                    diagnostic.projection,
                )
            });
            info!(
                "message action action_id={} target_id={} base_exists={} persistence={:?} action_count={}",
                action.action_id,
                target,
                diagnostic.base_exists,
                diagnostic.persistence,
                diagnostic.action_count
            );
        }
    }

    pub(crate) fn record_local_message_edit(
        &mut self,
        message: &wr::Message,
        replacement: Arc<str>,
    ) {
        self.local_action_sequence = self.local_action_sequence.saturating_add(1);
        let occurred_at = now_or(message.info.timestamp, &*self.clock);
        self.apply_message_action(MessageAction {
            action_id: format!(
                "local-edit:{}:{}",
                message.info.id, self.local_action_sequence
            )
            .into(),
            target_message_id: message.info.id.clone(),
            chat: message.info.chat.clone(),
            sender: message.info.sender.clone(),
            kind: MessageActionKind::Edit { replacement },
            occurred_at,
            arrival_order: self.local_action_sequence,
        });
    }

    pub(crate) fn record_local_message_delete(&mut self, message: &wr::Message) {
        self.local_action_sequence = self.local_action_sequence.saturating_add(1);
        let occurred_at = now_or(message.info.timestamp, &*self.clock);
        self.apply_message_action(MessageAction {
            action_id: format!(
                "local-delete:{}:{}",
                message.info.id, self.local_action_sequence
            )
            .into(),
            target_message_id: message.info.id.clone(),
            chat: message.info.chat.clone(),
            sender: message.info.sender.clone(),
            kind: MessageActionKind::Delete,
            occurred_at,
            arrival_order: self.local_action_sequence,
        });
    }

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

    #[cfg(test)]
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
        let (message, media_files, remove_metadata, invalidate_image_cache, projection) =
            project_message(
                self.messages.get(id),
                self.message_actions.get(id).map_or(&[], Vec::as_slice),
            );
        let message = message?;
        if !media_files.is_empty() {
            remove_owned_media_files(&self.media_path, &media_files);
        }
        if remove_metadata {
            self.metadata.remove(id);
        }
        if invalidate_image_cache {
            self.image_cache.remove(id);
        }
        self.messages.insert(id.clone(), message.clone());
        self.message_height_cache.invalidate(id);
        if self
            .message_actions
            .get(id)
            .is_some_and(|actions| !actions.is_empty())
        {
            self.db_handler.add_message(&message);
        }
        Some(projection)
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
mod integration_tests;
#[cfg(test)]
mod tests;
