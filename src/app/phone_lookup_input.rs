use super::App;
use super::actions::{FocusPane, Section};

fn looks_like_phone(query: &str) -> bool {
    query.starts_with('+') || query.as_bytes().first().is_some_and(u8::is_ascii_digit)
}

fn valid_phone(query: &str) -> bool {
    query.strip_prefix('+').is_some_and(|digits| {
        (7..=15).contains(&digits.len()) && digits.bytes().all(|digit| digit.is_ascii_digit())
    })
}

impl App<'_> {
    /// Returns true when Enter was consumed by an explicit number lookup.
    pub(crate) fn start_phone_lookup_from_search(&mut self) -> bool {
        let phone = self.contact_search.input.to_string();
        if self.selected_section != Section::Chats
            || self.focus_pane != FocusPane::ChatList
            || !looks_like_phone(&phone)
        {
            return false;
        }
        if self.chat_filter != super::ChatFilter::All {
            self.unavailable("Switch to All to search a new number");
        } else if !valid_phone(&phone) {
            self.unavailable("Use + followed by 7-15 digits");
        } else {
            let generation = self.next_phone_lookup_generation.wrapping_add(1);
            if self.phone_lookup_worker.enqueue(generation, phone.clone()) {
                self.next_phone_lookup_generation = generation;
                self.pending_phone_lookup = Some((generation, phone));
                self.action_notice = None;
            } else {
                self.unavailable("Number lookup is unavailable");
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::ActionNotice;
    use crate::app::events::{AppEvent, AppInput};
    use crate::app::phone_lookup_port::PhoneLookupPort;
    use crate::app::phone_lookup_worker::Worker;
    use crate::app::test_support::TestApp;
    use crate::input_key::{Key, KeyCode};
    use std::time::Duration;
    use whatsrust::{JID, PhoneLookupError};

    struct Registered;

    impl PhoneLookupPort for Registered {
        fn lookup(&mut self, phone: &str) -> Result<Option<JID>, PhoneLookupError> {
            assert_eq!(phone, "+15551234567");
            Ok(Some(JID::from("15551234567@s.whatsapp.net".to_owned())))
        }
    }

    #[test]
    fn search_enter_looks_up_new_number_then_opens_verified_chat() {
        let mut app = TestApp::new();
        app.phone_lookup_worker = Worker::new(app.tx.clone(), Box::new(Registered));
        app.contact_search_active = true;
        app.contact_search.input = "+15551234567".into();
        app.handle_chat_search_key(Key::k(KeyCode::Enter));
        assert_eq!(
            app.pending_phone_lookup.as_ref().map(|pending| pending.0),
            Some(1)
        );
        assert!(app.chats.is_empty());
        let AppInput::App(event @ AppEvent::PhoneLookupFinished { .. }) =
            app.rx.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("unexpected lookup event");
        };
        assert!(app.handle_phone_lookup_event(event));
        assert_eq!(
            app.open_chat(),
            Some(JID::from("15551234567@s.whatsapp.net".to_owned()))
        );
        assert!(app.chat_messages.is_empty());
    }

    #[test]
    fn invalid_number_does_not_start_lookup_or_create_chat() {
        let mut app = TestApp::new();
        app.contact_search_active = true;
        app.contact_search.input = "+12x".into();
        app.handle_chat_search_key(Key::k(KeyCode::Enter));
        assert!(app.pending_phone_lookup.is_none());
        assert!(app.chats.is_empty());
        assert!(
            matches!(app.action_notice, Some(ActionNotice::Unavailable(ref message)) if message == "Use + followed by 7-15 digits")
        );
    }
}
