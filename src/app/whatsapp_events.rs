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
            let changed = app.add_or_update_chat(
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
            if changed {
                app.invalidate_chat_list();
            }
            app.sort_chats();
            true
        }
        wr::Event::LogoutResult(status) => app.handle_logout_result(status),
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
            app.set_read_receipt_readiness(crate::app::read_receipts::Readiness::Connected);
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
