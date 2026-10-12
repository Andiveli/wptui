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

    /// Display name for a JID (chat or sender). A LID is not a phone number.
    fn fresh_contact_name(&self, jid: &wr::JID) -> Option<&Arc<str>> {
        self.fresh_contact_jids
            .as_ref()
            .is_none_or(|fresh| fresh.contains(jid))
            .then(|| self.contacts.get(jid))
            .flatten()
    }

    pub fn contact_name(&self, jid: &wr::JID) -> Arc<str> {
        let saved = self.fresh_contact_name(jid);
        saved
            .filter(|name| !phone_like_name(name))
            .or_else(|| self.profile_names.get(jid))
            .or_else(|| self.verified_phones.get(jid))
            .or_else(|| saved.filter(|_| !jid.0.ends_with("@lid")))
            .map(|name| canonical_contact_name(name))
            .unwrap_or_else(|| {
                let Some((user, server)) = jid.0.split_once('@') else {
                    return jid.0.clone();
                };
                if server == "lid" {
                    return Arc::from("");
                }
                if server == "s.whatsapp.net" && user.chars().all(|digit| digit.is_ascii_digit()) {
                    return Arc::from(user);
                }
                jid.0.clone()
            })
    }

    pub fn message_sender_name(&self, message: &wr::Message) -> Arc<str> {
        self.fresh_contact_name(&message.info.sender)
            .filter(|name| !phone_like_name(name))
            .map(|name| canonical_contact_name(name))
            .or_else(|| {
                self.message_push_name
                    .lookup_push_name(&message.info.id)
                    .map(|name| canonical_contact_name(&name))
                    .filter(|name| !phone_like_name(name))
            })
            .unwrap_or_else(|| self.contact_name(&message.info.sender))
    }

    pub(crate) fn get_contacts(&mut self) {
        let contacts = self.contact_source.get_contacts();
        self.apply_contact_refresh(contacts);
        let lids = self
            .chats
            .keys()
            .chain(self.messages.values().map(|message| &message.info.sender))
            .filter(|jid| jid.0.ends_with("@lid"))
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let mut changed = false;
        for lid in lids {
            changed |= self.remember_verified_phone(&lid);
        }
        if changed {
            self.invalidate_chat_list();
            self.refresh_status_contacts();
        }
    }

    pub(crate) fn remember_verified_phone(&mut self, jid: &wr::JID) -> bool {
        if !jid.0.ends_with("@lid") || self.verified_phones.contains_key(jid) {
            return false;
        }
        let Some(phone) = self.dm_resolver.resolve_dm_chat(jid) else {
            return false;
        };
        let Some(user) = phone.0.strip_suffix("@s.whatsapp.net") else {
            return false;
        };
        if user.is_empty() || !user.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        self.verified_phones.insert(jid.clone(), Arc::from(user));
        true
    }

    pub(crate) fn apply_contact_refresh(&mut self, contacts: Vec<(wr::JID, Arc<str>)>) {
        let fresh = contacts.iter().map(|(jid, _)| jid.clone()).collect();
        let mut changed = self.fresh_contact_jids.as_ref() != Some(&fresh);
        self.fresh_contact_jids = Some(fresh);
        for (jid, name) in contacts {
            changed |= self.contacts.get(&jid) != Some(&name);
            self.contacts.insert(jid.clone(), name.clone());
            self.contact_write
                .persist(super::PersistContact { jid, name });
        }
        if changed {
            self.invalidate_chat_list();
            self.refresh_status_contacts();
        }
    }
}

pub(super) fn phone_like_name(name: &str) -> bool {
    let mut digits = false;
    for character in name.trim().chars() {
        if character.is_ascii_digit() {
            digits = true;
        } else if !" +-().".contains(character) {
            return false;
        }
    }
    digits
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
    use crate::app::test_support::{FakeDmResolver, FakeMessagePushNamePort, TestApp};
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
    fn unknown_lid_is_not_a_phone_number_but_known_lid_uses_its_contact_name() {
        let mut app = TestApp::new();
        let lid = wr::JID::from("99887766@lid".to_owned());
        assert_eq!(app.contact_name(&lid).as_ref(), "");

        app.contacts.insert(lid.clone(), "Saved Name".into());
        assert_eq!(app.contact_name(&lid).as_ref(), "Saved Name");
        assert_eq!(
            app.contact_name(&wr::JID::from("15551234567@s.whatsapp.net".to_owned()))
                .as_ref(),
            "15551234567"
        );
    }

    #[test]
    fn profile_name_overrides_a_cached_verified_phone_but_not_a_saved_name() {
        let mut app = TestApp::new();
        let sender = wr::JID::from("99887766@lid".to_owned());
        app.contacts.insert(sender.clone(), "15551234567".into());
        app.profile_names
            .insert(sender.clone(), "Profile Name".into());
        assert_eq!(app.contact_name(&sender).as_ref(), "Profile Name");

        app.contacts.insert(sender.clone(), "Saved Name".into());
        assert_eq!(app.contact_name(&sender).as_ref(), "Saved Name");
    }

    #[test]
    fn message_profile_name_labels_an_unsaved_lid_after_ingestion() {
        let sender = wr::JID::from("99887766@lid".to_owned());
        let incoming = message("profile-from-message", sender.0.as_ref());
        let mut app = TestApp::with_message_push_name(Box::new(
            FakeMessagePushNamePort::with_name(incoming.info.id.clone(), "WhatsApp Profile"),
        ));
        app.add_message(incoming);
        assert_eq!(app.contact_name(&sender).as_ref(), "WhatsApp Profile");
    }

    #[test]
    fn verified_phone_labels_group_lid_without_any_contact_row() {
        let sender = wr::JID::from("99887766@lid".to_owned());
        let phone = wr::JID::from("15551234567@s.whatsapp.net".to_owned());
        let mut app = TestApp::with_dm_resolver(Box::new(FakeDmResolver::with_result(Some(phone))));
        let mut incoming = message("group-phone", sender.0.as_ref());
        incoming.info.chat = wr::JID::from("team@g.us".to_owned());

        app.add_message(incoming.clone());
        assert_eq!(app.contact_name(&sender).as_ref(), "15551234567");
        assert_eq!(app.message_sender_name(&incoming).as_ref(), "15551234567");
    }

    #[test]
    fn non_phone_resolution_cannot_be_displayed_as_a_verified_number() {
        let sender = wr::JID::from("99887766@lid".to_owned());
        let mut app =
            TestApp::with_dm_resolver(Box::new(FakeDmResolver::with_result(Some(sender.clone()))));
        app.add_message(message("unverified", sender.0.as_ref()));
        assert_eq!(app.contact_name(&sender).as_ref(), "");
        assert!(!app.verified_phones.contains_key(&sender));
    }

    #[test]
    fn history_lid_phone_is_retried_after_contact_sync() {
        let resolver = FakeDmResolver::default();
        let mut app = TestApp::with_dm_resolver(Box::new(resolver.clone()));
        let sender = wr::JID::from("99887766@lid".to_owned());
        let mut incoming = message("history-phone", sender.0.as_ref());
        incoming.info.chat = wr::JID::from("team@g.us".to_owned());
        app.add_message_without_sort(incoming);
        assert_eq!(app.contact_name(&sender).as_ref(), "");

        *resolver.result.lock().unwrap() =
            Some(wr::JID::from("15551234567@s.whatsapp.net".to_owned()));
        app.get_contacts();
        assert_eq!(app.contact_name(&sender).as_ref(), "15551234567");
    }

    #[test]
    fn refresh_keeps_cached_name_without_displaying_it_over_current_profile() {
        let mut app = TestApp::new();
        let sender = wr::JID::from("99887766@lid".to_owned());
        app.contacts.insert(sender.clone(), "Old Saved Name".into());
        app.profile_names
            .insert(sender.clone(), "Current Profile".into());
        app.apply_contact_refresh(vec![]);

        assert_eq!(app.contacts[&sender].as_ref(), "Old Saved Name");
        assert_eq!(app.contact_name(&sender).as_ref(), "Current Profile");
        assert_eq!(
            app.message_sender_name(&message("stale-profile", sender.0.as_ref()))
                .as_ref(),
            "Current Profile"
        );

        app.apply_contact_refresh(vec![(sender.clone(), "New Saved Name".into())]);
        assert_eq!(app.contact_name(&sender).as_ref(), "New Saved Name");
    }

    #[test]
    fn local_contact_name_wins_over_message_push_name() {
        let sender = wr::JID::from("123@s.whatsapp.net".to_owned());
        let message = message("local-name", sender.0.as_ref());
        let mut app = TestApp::with_message_push_name(Box::new(
            FakeMessagePushNamePort::with_name(message.info.id.clone(), "WhatsApp Profile"),
        ));
        app.contacts
            .insert(sender.clone(), "Saved Full Name".into());

        assert_eq!(
            app.message_sender_name(&message).as_ref(),
            "Saved Full Name"
        );
    }

    #[test]
    fn unsaved_message_push_name_is_plain_and_numeric_is_final_fallback() {
        let with_push = message("push-name", "123@s.whatsapp.net");
        let app = TestApp::with_message_push_name(Box::new(FakeMessagePushNamePort::with_name(
            with_push.info.id.clone(),
            "~ WhatsApp Profile",
        )));
        assert_eq!(
            app.message_sender_name(&with_push).as_ref(),
            "WhatsApp Profile"
        );

        let app = TestApp::new();
        let without_push = message("numeric-name", "456@s.whatsapp.net");
        assert_eq!(app.message_sender_name(&without_push).as_ref(), "456");
    }
}
