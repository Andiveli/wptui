use super::{App, Chat, MessageAction, MessageActionKind};
use whatsrust as wr;

pub(crate) fn handle(app: &mut App<'_>, event: wr::Event) -> bool {
    match event {
        wr::Event::AppStateSyncComplete => {
            app.get_contacts();
            app.load_communities();
            app.sort_chats();
            true
        }
        wr::Event::Chat {
            jid,
            last_message_time,
        } => {
            // History sync reports chats that may carry no messages. Keep
            // them so the chat list reflects the full account, not only
            // conversations that shipped a message in the sync batch.
            app.add_or_update_chat(
                Chat {
                    jid,
                    last_message_time: (last_message_time > 0).then_some(last_message_time),
                },
                |chat| {
                    if last_message_time > 0 && Some(last_message_time) > chat.last_message_time {
                        chat.last_message_time = Some(last_message_time);
                    }
                },
            );
            app.sort_chats();
            true
        }
        wr::Event::LogoutResult(status) => match status {
            wr::LogoutStatus::LoggedOut | wr::LogoutStatus::NotLoggedIn => {
                app.finish_logout();
                true
            }
            wr::LogoutStatus::LocalOnly => {
                // Remote revocation failed, so WhatsApp on the phone still
                // lists this device. The local session is already gone;
                // surface it instead of silently quitting, and let the
                // user retry (a second logout resolves as NotLoggedIn and
                // finishes) or remove the device manually.
                log::warn!(
                    "Logout: device was not unlinked on the phone; remove it manually in WhatsApp → Linked devices"
                );
                app.pending_logout = false;
                app.logout_in_progress = false;
                app.logout_menu_index = 0;
                app.unavailable(
                    "Logged out locally, but the device is still linked on the phone — remove it in WhatsApp (Settings → Linked devices), then log out again to finish",
                );
                true
            }
            wr::LogoutStatus::Failed => {
                // Even the local cleanup failed. Surface it and keep running.
                app.pending_logout = false;
                app.logout_in_progress = false;
                app.logout_menu_index = 0;
                app.unavailable("Could not log out");
                true
            }
        },
        wr::Event::SyncProgress(percent) => {
            app.history_sync_percent = Some(percent);
            true
        }
        wr::Event::Receipt {
            kind,
            chat,
            message_ids,
        } => {
            log::debug!(
                "Received receipt: {:?} for chat: {:?} with messages: {:?}",
                kind,
                chat,
                message_ids
            );
            for msg_id in message_ids {
                if let Some(message) = app.messages.get_mut(&msg_id) {
                    message.info.read_by += 1;
                    app.db_handler.add_message(message);
                }
            }
            true
        }
        wr::Event::Reaction {
            target_message_id,
            participant,
            text,
            ..
        } => {
            app.apply_reaction(&target_message_id, participant, text);
            true
        }
        wr::Event::Connected => {
            // Connected is emitted again after reconnects. The first
            // probe may race group metadata hydration, so AppStateSyncComplete
            // below remains the authoritative follow-up refresh.
            app.load_communities();
            app.mark_presence_ready();
            true
        }
        wr::Event::MessageAction {
            action_id,
            target_message_id,
            chat,
            sender,
            kind,
            occurred_at,
            arrival_order,
        } => {
            app.apply_message_action(MessageAction {
                action_id,
                target_message_id,
                chat,
                sender,
                kind: match kind {
                    wr::MessageActionKind::Edit { replacement } => {
                        MessageActionKind::Edit { replacement }
                    }
                    wr::MessageActionKind::Delete => MessageActionKind::Delete,
                },
                occurred_at,
                arrival_order,
            });
            true
        }
    }
}

#[cfg(test)]
mod tests;
