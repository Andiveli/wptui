use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use whatsrust::JID;

use super::App;

const TYPING_TTL: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct ChatTyping {
    chats: HashMap<JID, HashMap<JID, Instant>>,
}

impl ChatTyping {
    fn update(&mut self, chat: JID, sender: JID, composing: bool, now: Instant) -> bool {
        if composing {
            let previous = self
                .chats
                .entry(chat)
                .or_default()
                .insert(sender, now + TYPING_TTL);
            return previous.is_none_or(|deadline| deadline <= now);
        }
        let Some(senders) = self.chats.get_mut(&chat) else {
            return false;
        };
        let changed = senders.remove(&sender).is_some();
        if senders.is_empty() {
            self.chats.remove(&chat);
        }
        changed
    }

    fn active_senders(&self, chat: &JID, now: Instant) -> impl Iterator<Item = &JID> {
        self.chats
            .get(chat)
            .into_iter()
            .flatten()
            .filter_map(move |(sender, deadline)| (*deadline > now).then_some(sender))
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.chats.values().flat_map(HashMap::values).copied().min()
    }

    pub(super) fn expire(&mut self, active: Option<&JID>, now: Instant) -> bool {
        let mut redraw = false;
        self.chats.retain(|chat, senders| {
            let previous = senders.len();
            senders.retain(|_, deadline| *deadline > now);
            redraw |= active == Some(chat) && previous != senders.len();
            !senders.is_empty()
        });
        redraw
    }
}

impl App<'_> {
    pub(super) fn handle_chat_typing(&mut self, chat: JID, sender: JID, composing: bool) -> bool {
        let active = self.open_chat.as_ref() == Some(&chat);
        self.chat_typing
            .update(chat, sender, composing, Instant::now())
            && active
    }

    pub(crate) fn chat_typing_label(&self, chat: &JID) -> Option<(String, usize)> {
        let mut names: Vec<_> = self
            .chat_typing
            .active_senders(chat, Instant::now())
            .map(|sender| {
                let name = self.contact_name(sender);
                if name.trim().is_empty() {
                    "Someone".into()
                } else {
                    name
                }
            })
            .collect();
        names.sort_unstable();
        let label = match names.as_slice() {
            [] => return None,
            [one] => format!("{one} is typing…"),
            [first, second] => format!("{first}, {second} are typing…"),
            [first, second, rest @ ..] => {
                format!("{first}, {second} and {} more are typing…", rest.len())
            }
        };
        Some((label, names.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composing_refreshes_only_that_senders_expiry() {
        let chat = JID::from("123@g.us".to_owned());
        let alice = JID::from("456@s.whatsapp.net".to_owned());
        let bob = JID::from("789@s.whatsapp.net".to_owned());
        let start = Instant::now();
        let refreshed = start + Duration::from_secs(1);
        let mut typing = ChatTyping::default();
        assert!(typing.update(chat.clone(), alice.clone(), true, start));
        assert!(typing.update(chat.clone(), bob.clone(), true, start));
        assert!(!typing.update(chat.clone(), bob.clone(), true, refreshed));
        assert_eq!(typing.deadline(), Some(start + TYPING_TTL));
        assert!(typing.expire(Some(&chat), start + TYPING_TTL));
        assert_eq!(
            typing
                .active_senders(&chat, start + TYPING_TTL)
                .collect::<Vec<_>>(),
            vec![&bob]
        );
        assert_eq!(typing.deadline(), Some(refreshed + TYPING_TTL));
        assert!(typing.expire(Some(&chat), refreshed + TYPING_TTL));
        assert_eq!(typing.deadline(), None);
    }
}
