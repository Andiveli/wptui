mod hydration;
pub mod hydration_port;
mod read_state;
mod receipts;
mod storage;

use whatsrust as wr;

use super::App;

impl App<'_> {
    pub fn add_message(&mut self, message: wr::Message) {
        self.with_chat_list_mutation(|app| app.add_message_inner(message));
    }

    fn add_message_inner(&mut self, message: wr::Message) {
        let chat_jid = message.info.chat.clone();
        let is_open_chat = self.open_chat.as_ref() == Some(&chat_jid);
        let is_inbound = !message.info.is_from_me && !App::is_status_chat(&chat_jid);
        let is_confirmed_own_message = message.info.is_from_me && !App::is_status_chat(&chat_jid);
        let was_at_latest = self.is_viewing_latest_message(&chat_jid);
        let projection_changed = self.add_message_without_sort(message);
        self.sort_chat_messages(chat_jid.clone());
        if !projection_changed {
            return;
        }
        if is_open_chat {
            if is_confirmed_own_message || (is_inbound && was_at_latest) {
                self.catch_up_to_latest(&chat_jid);
            } else {
                if is_inbound {
                    self.timeline
                        .entry(chat_jid.clone())
                        .or_default()
                        .pending_new_messages += 1;
                }
                self.reanchor_message_selection(&chat_jid);
            }
        } else if is_confirmed_own_message {
            self.mark_chat_read_at_latest(&chat_jid);
        } else if is_inbound {
            self.timeline
                .entry(chat_jid)
                .or_default()
                .pending_new_messages += 1;
        }
        if projection_changed {
            self.invalidate_chat_list();
        }
    }
}

#[cfg(test)]
mod tests;
