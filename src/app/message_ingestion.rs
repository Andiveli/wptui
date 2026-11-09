use std::sync::Arc;

use super::App;
use crate::app::notifications::{
    notification_eligibility, notification_is_muted, notification_projection,
};
use crate::app::runtime_diagnostics::Phase;
use log::{error, info};
use whatsrust as wr;

impl App<'_> {
    pub fn apply_reaction(&mut self, target: &wr::MessageId, participant: wr::JID, text: Arc<str>) {
        self.message_reaction_write
            .record(crate::app::message_reactions::RecordMessageReaction {
                message_id: target.clone(),
                participant: participant.clone(),
                emoji: text.clone(),
            });
        if text.is_empty() {
            if let Some(reactions) = self.reactions.get_mut(target) {
                reactions.remove(&participant);
                if reactions.is_empty() {
                    self.reactions.remove(target);
                }
            }
        } else {
            self.reactions
                .entry(target.clone())
                .or_default()
                .insert(participant, text);
        }
        self.message_height_cache.invalidate(target);
    }

    pub fn process_message(&mut self, message: wr::Message, is_sync: bool) -> bool {
        let mut chat_settings = (!is_sync && self.should_notify(&message)).then(|| {
            self.chat_settings_query
                .get_chat_settings(&message.info.chat)
        });
        self.process_message_with_lookup(message, is_sync, move |_| {
            chat_settings
                .take()
                .expect("chat settings are queried only for eligible messages")
        })
    }

    pub(crate) fn process_message_with_lookup(
        &mut self,
        message: wr::Message,
        is_sync: bool,
        lookup: impl FnMut(&wr::JID) -> wr::ChatSettings,
    ) -> bool {
        // Always consume, including non-text, outgoing, status and stale source.
        let source = match &message.message {
            wr::MessageContent::Text(text) => text.as_ref(),
            _ => "",
        };
        let native = wr::take_native_link_preview(&message.info, source)
            .and_then(|preview| super::native_link_previews::validated_preview(&message, preview))
            .map(
                |(_, preview)| super::native_link_previews::NativeLinkPreviewRecord {
                    chat: message.info.chat.clone(),
                    message_id: message.info.id.clone(),
                    source: source.into(),
                    preview,
                },
            );
        self.record_phase(Phase::MessageIngestionDb, |app| {
            if !is_sync {
                app.handle_notification_with_lookup(&message, lookup);
            }

            let message_for_persistence = message.clone();
            if is_sync {
                let chat = message.info.chat.clone();
                app.with_chat_list_mutation(|app| {
                    let changed = app.add_message_without_sort(message);
                    app.sort_chat_messages(chat);
                    if changed {
                        app.invalidate_chat_list();
                    }
                });
            } else {
                app.add_message(message);
            }
            let chat = app
                .chats
                .get(&message_for_persistence.info.chat)
                .expect("message ingestion creates its chat")
                .clone();
            let id = &message_for_persistence.info.id;
            app.prune_native_link_preview(id);
            if let Some(native) = native {
                app.restore_native_link_preview(native);
            }
            let preview = app
                .native_link_preview(id)
                .map(|(_, preview)| preview.clone());
            app.chat_store_write.persist_with_native_preview(
                crate::app::chat_store::write_port::PersistChatMessage {
                    chat,
                    message: message_for_persistence,
                },
                preview,
            );

            app.sort_chats();
            !is_sync
        })
    }

    fn handle_notification_with_lookup(
        &self,
        message: &wr::Message,
        mut lookup: impl FnMut(&wr::JID) -> wr::ChatSettings,
    ) {
        if !self.should_notify(message) {
            return;
        }

        let chat_settings = lookup(&message.info.chat);
        info!(
            "Chat settings for {:?}: {:?}",
            message.info.chat, chat_settings
        );
        if chat_settings.found && notification_is_muted(true, chat_settings.muted_until, self.now())
        {
            return;
        }

        let play_sound = self
            .startup_unix_seconds
            .is_none_or(|started| message.info.timestamp >= started);
        let notification =
            notification_projection(message, self.contact_name(&message.info.sender), play_sound);
        if let Err(err) = self.notifier.show(&notification) {
            error!("Failed to show desktop notification: {err}");
        }
    }

    fn should_notify(&self, message: &wr::Message) -> bool {
        notification_eligibility(message, self.open_chat.as_ref())
    }
}

#[cfg(test)]
mod tests;
