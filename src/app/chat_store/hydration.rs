use std::sync::Arc;

use log::{info, warn};
use whatsrust as wr;

use super::App;

use super::hydration_port::ChatStoreHydration;

impl App<'_> {
    pub fn load_data_from_db(&mut self) {
        info!("Reading database");
        let ChatStoreHydration {
            chats,
            contacts,
            messages,
            reactions,
        } = self.chat_store_hydration.load();
        for chat in chats {
            self.chats.insert(chat.jid.clone(), chat);
        }
        for (jid, name) in contacts {
            self.contacts.insert(jid, name);
        }
        let diagnostics = self.message_action_diagnostics.clone();
        match self.status_cursor.load() {
            Ok(cursors) => {
                diagnostics.record_read_sync(|| {
                    format!(
                        "source=rust event=status_cursor_read rows={}",
                        cursors.len()
                    )
                });
                for (contact, timestamp) in cursors {
                    let contact_id =
                        crate::app::message_action_diagnostics::identifier_for_log(&contact.0);
                    diagnostics.record_read_sync(|| {
                        format!(
                            "source=rust event=status_cursor_restored contact={contact_id} timestamp={timestamp}"
                        )
                    });
                    self.status_last_seen.insert(contact, timestamp);
                }
            }
            Err(error) => log::error!("status cursor read failed: {error}"),
        }
        self.restore_read_cursors();

        for action in self.db_handler.get_message_actions() {
            self.local_action_sequence = self.local_action_sequence.max(
                action
                    .action_id
                    .rsplit_once(':')
                    .and_then(|(_, sequence)| sequence.parse().ok())
                    .unwrap_or_default(),
            );
            self.message_actions
                .entry(action.target_message_id.clone())
                .or_default()
                .push(action);
        }

        for message in messages {
            self.add_message_without_sort(message);
        }
        let chat_ids = self.chat_messages.keys().cloned().collect::<Vec<_>>();
        for chat_id in chat_ids {
            self.sort_chat_messages(chat_id);
        }
        for (message_id, participant, emoji) in reactions {
            self.reactions
                .entry(message_id)
                .or_default()
                .insert(participant, emoji);
        }
        warn!(
            "Finished reading database with {} chats and {} messages",
            self.chats.len(),
            self.messages.len()
        );
        self.invalidate_chat_list();
    }

    /// Display name for a JID (chat or sender). Falls back to the JID string if not in contacts.
    pub fn contact_name(&self, jid: &wr::JID) -> Arc<str> {
        self.contacts
            .get(jid)
            .map(|name| canonical_contact_name(name))
            .unwrap_or_else(|| jid.0.clone())
    }

    pub fn message_sender_name(&self, message: &wr::Message) -> Arc<str> {
        self.contacts
            .get(&message.info.sender)
            .map(|name| canonical_contact_name(name))
            .or_else(|| {
                wr::message_push_name(&message.info.id).map(|name| canonical_contact_name(&name))
            })
            .unwrap_or_else(|| self.contact_name(&message.info.sender))
    }

    pub(crate) fn get_contacts(&mut self) {
        let contacts = self.contact_source.get_contacts();
        self.apply_contact_refresh(contacts);
    }

    pub(crate) fn apply_contact_refresh(&mut self, contacts: Vec<(wr::JID, Arc<str>)>) {
        let mut changed = false;
        for (jid, name) in contacts {
            changed |= self.contacts.get(&jid) != Some(&name);
            self.contacts.insert(jid.clone(), name.clone());
            self.contact_write
                .persist(super::PersistContact { jid, name });
        }
        if changed {
            self.invalidate_chat_list();
        }
    }
}

pub(super) fn canonical_contact_name(name: &str) -> Arc<str> {
    let name = name.trim();
    ["~ ", "+ "]
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))
        .unwrap_or(name)
        .trim()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::TestApp;
    use whatsrust as wr;

    fn message(id: &str, sender: &str) -> wr::Message {
        wr::Message {
            info: wr::MessageInfo {
                id: id.into(),
                chat: sender.to_owned().into(),
                sender: sender.to_owned().into(),
                mentions_self: false,
                timestamp: 0,
                is_from_me: false,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text("body".into()),
        }
    }

    #[test]
    fn canonicalizes_protocol_contact_prefixes() {
        assert_eq!(canonical_contact_name("~ Alice").as_ref(), "Alice");
        assert_eq!(canonical_contact_name("+ Bob").as_ref(), "Bob");
    }

    #[test]
    fn contact_name_falls_back_to_the_jid() {
        let app = TestApp::new();
        let jid = wr::JID::from("alice@example.test".to_owned());

        assert_eq!(app.contact_name(&jid).as_ref(), "alice@example.test");
    }

    #[test]
    fn local_contact_name_wins_over_message_push_name() {
        let mut app = TestApp::new();
        let sender = wr::JID::from("123@s.whatsapp.net".to_owned());
        app.contacts
            .insert(sender.clone(), "Saved Full Name".into());
        let message = message("local-name", sender.0.as_ref());
        wr::store_message_push_name(&message.info.id, "WhatsApp Profile");

        assert_eq!(
            app.message_sender_name(&message).as_ref(),
            "Saved Full Name"
        );
    }

    #[test]
    fn unsaved_message_push_name_is_plain_and_numeric_is_final_fallback() {
        let app = TestApp::new();
        let with_push = message("push-name", "123@s.whatsapp.net");
        wr::store_message_push_name(&with_push.info.id, "WhatsApp Profile");
        assert_eq!(
            app.message_sender_name(&with_push).as_ref(),
            "WhatsApp Profile"
        );

        let without_push = message("numeric-name", "456@s.whatsapp.net");
        assert_eq!(
            app.message_sender_name(&without_push).as_ref(),
            "456@s.whatsapp.net"
        );
    }
}
