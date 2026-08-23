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
        self.db_handler
            .record_reaction(target, participant.clone(), text.clone());
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

    pub(crate) fn process_message(&mut self, message: wr::Message, is_sync: bool) -> bool {
        self.process_message_with_lookup(message, is_sync, wr::get_chat_settings)
    }

    pub(crate) fn process_message_with_lookup(
        &mut self,
        message: wr::Message,
        is_sync: bool,
        lookup: impl FnMut(&wr::JID) -> wr::ChatSettings,
    ) -> bool {
        self.record_phase(Phase::MessageIngestionDb, |app| {
            if !is_sync {
                app.handle_notification_with_lookup(&message, lookup);
            }

            app.db_handler.add_message(&message);
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

            let chat_jid = app.get_selected_chat();
            app.sort_chats();
            app.select_chat(chat_jid);
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

        let notification =
            notification_projection(message, self.contact_name(&message.info.sender));
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
