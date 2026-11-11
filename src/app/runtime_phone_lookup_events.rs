use super::App;
use super::actions::{ActionNotice, FocusPane, Section};
use super::events::AppEvent;
use whatsrust::PhoneLookupError;

impl App<'_> {
    fn phone_lookup_unavailable(&mut self, message: &str) {
        self.unavailable(message);
        if let Some(ActionNotice::Unavailable(message)) = self.action_notice.take() {
            self.action_notice = Some(ActionNotice::PhoneLookup(message));
        }
    }

    pub(crate) fn handle_phone_lookup_event(&mut self, event: AppEvent) -> bool {
        let AppEvent::PhoneLookupFinished {
            generation,
            phone,
            result,
        } = event
        else {
            return false;
        };
        if !self
            .pending_phone_lookup
            .as_ref()
            .is_some_and(|pending| pending.0 == generation && pending.1 == phone)
        {
            return false;
        }
        self.pending_phone_lookup = None;
        if !self.contact_search_active
            || self.contact_search.input.to_string() != phone
            || self.selected_section != Section::Chats
            || self.focus_pane != FocusPane::ChatList
            || self.logout_in_progress
        {
            return false;
        }
        self.phone_lookup_observation = None;
        match result {
            Ok(Some(jid)) => {
                let expected = phone
                    .strip_prefix('+')
                    .map(|digits| format!("{digits}@s.whatsapp.net"));
                if expected.as_deref() != Some(jid.0.as_ref()) {
                    self.phone_lookup_unavailable("Could not verify number");
                    return true;
                }
                self.contact_search.clean();
                self.contact_search_active = false;
                self.open_chat_by_jid(jid.clone());
                self.select_chat(Some(jid));
                self.focus_pane = FocusPane::Conversation;
                true
            }
            Ok(None) => {
                self.phone_lookup_unavailable("Number is not on WhatsApp");
                true
            }
            Err(error) => {
                let message = match error {
                    PhoneLookupError::InvalidNumber => "Use + followed by 7-15 digits",
                    PhoneLookupError::ClientUnavailable => "WhatsApp is not connected",
                    PhoneLookupError::RequestFailed => "Could not check number",
                    PhoneLookupError::InvalidBridgeResult => "Could not verify number",
                    PhoneLookupError::TimedOut => "Number check timed out",
                    PhoneLookupError::EmptyResponse => "WhatsApp returned no number result",
                    PhoneLookupError::ResponseMismatch => {
                        "WhatsApp returned a mismatched number result"
                    }
                    PhoneLookupError::QueryMismatch => {
                        "WhatsApp returned a mismatched query result"
                    }
                    PhoneLookupError::LidOnly => {
                        "WhatsApp returned only a LID without a phone number"
                    }
                    PhoneLookupError::DifferentPhoneNumber => {
                        "WhatsApp returned a different phone number"
                    }
                    PhoneLookupError::ResponseObservation(observation) => {
                        self.phone_lookup_observation = Some(
                            super::phone_lookup_observation::ObservationView::new(observation),
                        );
                        "Local number response shown; Esc closes"
                    }
                    PhoneLookupError::ResponseComparison { phone, jid } => {
                        let notice = format!(
                            "Lookup comparison PN(d/n/i)={}/{}/{} JID(d/n/i)={}/{}/{}",
                            u8::from(phone[0]),
                            u8::from(phone[1]),
                            u8::from(phone[2]),
                            u8::from(jid[0]),
                            u8::from(jid[1]),
                            u8::from(jid[2])
                        );
                        self.phone_lookup_unavailable(&notice);
                        return true;
                    }
                };
                self.phone_lookup_unavailable(message);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::TestApp;
    use whatsrust::{JID, LocalPhoneLookupObservation};

    fn awaiting_lookup() -> TestApp {
        let mut app = TestApp::new();
        app.contact_search_active = true;
        app.contact_search.input = "+15551234567".into();
        app.pending_phone_lookup = Some((1, "+15551234567".into()));
        app
    }

    fn completed(generation: u64, result: Result<Option<JID>, PhoneLookupError>) -> AppEvent {
        AppEvent::PhoneLookupFinished {
            generation,
            phone: "+15551234567".into(),
            result,
        }
    }

    fn awaiting_number_lookup(phone: &str) -> TestApp {
        let mut app = awaiting_lookup();
        app.contact_search.input = phone.into();
        app.pending_phone_lookup = Some((1, phone.into()));
        app
    }

    fn confirmed_number_event(generation: u64, phone: &str, confirmed: &str) -> AppEvent {
        AppEvent::PhoneLookupFinished {
            generation,
            phone: phone.into(),
            result: Ok(Some(JID::from(confirmed.to_owned()))),
        }
    }

    #[test]
    fn equivalent_confirmed_number_opens_server_pn_without_sending() {
        for (phone, confirmed) in [
            ("+525512345678", "5215512345678"),
            ("+5215512345678", "525512345678"),
            ("+541112345678", "5491112345678"),
            ("+5491112345678", "541112345678"),
            ("+525512345678", "525512345678"),
            ("+5215512345678", "5215512345678"),
            ("+541112345678", "541112345678"),
            ("+5491112345678", "5491112345678"),
            ("+593991234567", "593991234567"),
        ] {
            let mut app = awaiting_number_lookup(phone);
            let confirmed = format!("{confirmed}@s.whatsapp.net");
            let jid = JID::from(confirmed.clone());
            assert!(app.handle_phone_lookup_event(confirmed_number_event(1, phone, &confirmed)));
            assert_eq!(app.open_chat(), Some(jid.clone()), "{phone} -> {confirmed}");
            assert_eq!(app.get_selected_chat(), Some(jid.clone()));
            assert_eq!(app.chats.len(), 1);
            assert!(app.chats.contains_key(&jid));
            assert!(app.chat_messages.is_empty());
            assert!(app.pending_phone_lookup.is_none());
            assert!(app.contact_search.input.is_empty());
            assert!(!app.contact_search_active);
            assert_eq!(app.focus_pane, FocusPane::Conversation);
            assert!(app.action_notice.is_none());
        }
    }

    fn assert_confirmed_number_rejected(phone: &str, confirmed: &str) {
        let mut app = awaiting_number_lookup(phone);
        assert!(app.handle_phone_lookup_event(confirmed_number_event(1, phone, confirmed)));
        assert!(app.chats.is_empty(), "{phone} -> {confirmed}");
        assert_eq!(app.open_chat(), None);
        assert_eq!(app.get_selected_chat(), None);
        assert!(app.chat_messages.is_empty());
        assert!(app.pending_phone_lookup.is_none());
        assert!(app.contact_search_active);
        assert_eq!(app.contact_search.input.to_string(), phone);
        assert_eq!(app.focus_pane, FocusPane::ChatList);
        assert_eq!(app.selected_section, Section::Chats);
        assert!(matches!(
            app.action_notice,
            Some(ActionNotice::PhoneLookup(ref message)) if message == "Could not verify number"
        ));
    }

    #[test]
    fn unsupported_confirmed_numbers_leave_chat_and_search_state_unchanged() {
        for (left, right) in [
            ("525512345678", "5215512345679"),
            ("541112345678", "5491112345679"),
            ("52551234567", "521551234567"),
            ("54111234567", "549111234567"),
            ("5255123456789", "52155123456789"),
            ("5411123456789", "54911123456789"),
            ("593991234567", "5931991234567"),
            ("535512345678", "5315512345678"),
            ("525512345678", "5495512345678"),
            ("54111512345678", "5491112345678"),
            ("5401112345678", "5491112345678"),
        ] {
            for (requested, confirmed) in [(left, right), (right, left)] {
                assert_confirmed_number_rejected(
                    &format!("+{requested}"),
                    &format!("{confirmed}@s.whatsapp.net"),
                );
            }
        }
        for (phone, confirmed) in [
            ("+525512345678", "5215512345678"),
            ("+5215512345678", "525512345678"),
            ("+541112345678", "5491112345678"),
            ("+5491112345678", "541112345678"),
            ("+525512345678", "525512345678"),
            ("+541112345678", "541112345678"),
        ] {
            for jid in [
                format!("{confirmed}@lid"),
                format!("{confirmed}@c.us"),
                format!("{confirmed}@g.us"),
                format!("{confirmed}@S.WHATSAPP.NET"),
                format!("{confirmed}:1@s.whatsapp.net"),
                format!("{confirmed}.1:1@s.whatsapp.net"),
                format!("{confirmed}_1@s.whatsapp.net"),
                format!("+{confirmed}@s.whatsapp.net"),
            ] {
                assert_confirmed_number_rejected(phone, &jid);
            }
        }
    }

    #[test]
    fn stale_equivalent_confirmed_numbers_cannot_open_a_chat() {
        for (phone, confirmed) in [
            ("+525512345678", "5215512345678"),
            ("+5215512345678", "525512345678"),
            ("+541112345678", "5491112345678"),
            ("+5491112345678", "541112345678"),
        ] {
            for stale in ["generation", "pending phone", "edited search"] {
                let mut app = awaiting_number_lookup(phone);
                match stale {
                    "generation" => app.pending_phone_lookup = Some((2, phone.into())),
                    "pending phone" => {
                        app.pending_phone_lookup = Some((1, "+593991234567".into()));
                    }
                    _ => app.contact_search.input = "+593991234567".into(),
                }
                let pending = app.pending_phone_lookup.clone();
                let search = app.contact_search.input.to_string();
                let confirmed = format!("{confirmed}@s.whatsapp.net");
                assert!(
                    !app.handle_phone_lookup_event(confirmed_number_event(1, phone, &confirmed))
                );
                assert!(app.chats.is_empty());
                assert_eq!(app.open_chat(), None);
                assert!(app.chat_messages.is_empty());
                assert!(app.contact_search_active);
                assert_eq!(app.contact_search.input.to_string(), search);
                assert_eq!(app.focus_pane, FocusPane::ChatList);
                assert!(app.action_notice.is_none());
                if stale == "edited search" {
                    assert!(app.pending_phone_lookup.is_none());
                } else {
                    assert_eq!(app.pending_phone_lookup, pending);
                }
            }
        }
    }

    #[test]
    fn verified_number_opens_chat_without_sending_a_message() {
        let mut app = awaiting_lookup();
        let jid = JID::from("15551234567@s.whatsapp.net".to_owned());
        assert!(app.handle_phone_lookup_event(completed(1, Ok(Some(jid.clone())))));
        assert_eq!(app.open_chat(), Some(jid.clone()));
        assert!(app.chats.contains_key(&jid));
        assert!(app.chat_messages.is_empty());
        assert_eq!(app.focus_pane, FocusPane::Conversation);
        assert!(app.contact_search.input.is_empty());
    }

    #[test]
    fn unregistered_and_failed_lookups_leave_chats_unchanged() {
        for (result, notice) in [
            (Ok(None), "Number is not on WhatsApp"),
            (
                Err(PhoneLookupError::RequestFailed),
                "Could not check number",
            ),
            (
                Err(PhoneLookupError::InvalidNumber),
                "Use + followed by 7-15 digits",
            ),
            (
                Err(PhoneLookupError::ClientUnavailable),
                "WhatsApp is not connected",
            ),
            (
                Err(PhoneLookupError::InvalidBridgeResult),
                "Could not verify number",
            ),
            (Err(PhoneLookupError::TimedOut), "Number check timed out"),
            (
                Err(PhoneLookupError::EmptyResponse),
                "WhatsApp returned no number result",
            ),
            (
                Err(PhoneLookupError::ResponseMismatch),
                "WhatsApp returned a mismatched number result",
            ),
            (
                Err(PhoneLookupError::QueryMismatch),
                "WhatsApp returned a mismatched query result",
            ),
            (
                Err(PhoneLookupError::LidOnly),
                "WhatsApp returned only a LID without a phone number",
            ),
            (
                Err(PhoneLookupError::DifferentPhoneNumber),
                "WhatsApp returned a different phone number",
            ),
            (
                Err(PhoneLookupError::ResponseComparison {
                    phone: [false, true, true],
                    jid: [false, false, true],
                }),
                "Lookup comparison PN(d/n/i)=0/1/1 JID(d/n/i)=0/0/1",
            ),
            (
                Err(PhoneLookupError::ResponseComparison {
                    phone: [false, false, true],
                    jid: [false, true, true],
                }),
                "Lookup comparison PN(d/n/i)=0/0/1 JID(d/n/i)=0/1/1",
            ),
            (
                Err(PhoneLookupError::ResponseComparison {
                    phone: [true, true, false],
                    jid: [false, false, true],
                }),
                "Lookup comparison PN(d/n/i)=1/1/0 JID(d/n/i)=0/0/1",
            ),
            (
                Err(PhoneLookupError::ResponseComparison {
                    phone: [false, false, true],
                    jid: [true, true, false],
                }),
                "Lookup comparison PN(d/n/i)=0/0/1 JID(d/n/i)=1/1/0",
            ),
            (
                Err(PhoneLookupError::ResponseComparison {
                    phone: [true, false, true],
                    jid: [false, true, true],
                }),
                "Lookup comparison PN(d/n/i)=1/0/1 JID(d/n/i)=0/1/1",
            ),
            (
                Err(PhoneLookupError::ResponseComparison {
                    phone: [false, true, true],
                    jid: [false, true, true],
                }),
                "Lookup comparison PN(d/n/i)=0/1/1 JID(d/n/i)=0/1/1",
            ),
            (
                Ok(Some(JID::from("15559876543@s.whatsapp.net".to_owned()))),
                "Could not verify number",
            ),
        ] {
            let mut app = awaiting_lookup();
            assert!(app.handle_phone_lookup_event(completed(1, result)));
            assert!(app.chats.is_empty());
            assert_eq!(app.open_chat(), None);
            assert!(app.chat_messages.is_empty());
            assert!(app.pending_phone_lookup.is_none());
            assert!(app.contact_search_active);
            assert_eq!(app.contact_search.input.to_string(), "+15551234567");
            assert_eq!(app.focus_pane, FocusPane::ChatList);
            assert!(
                matches!(app.action_notice, Some(ActionNotice::PhoneLookup(ref message)) if message == notice)
            );
        }
    }

    #[test]
    fn local_response_is_visible_only_for_current_rejected_lookup() {
        for generation in [1, 2] {
            let mut app = awaiting_lookup();
            app.pending_phone_lookup = Some((generation, "+15551234567".into()));
            let raw = "Requested: \"+15551234567\"\nPhoneNumber.User: \"15559876543\"";
            let observation = LocalPhoneLookupObservation::from_display_text(raw.into()).unwrap();
            let event = completed(1, Err(PhoneLookupError::ResponseObservation(observation)));
            assert!(!format!("{event:?}").contains("15559876543"));
            assert_eq!(app.handle_phone_lookup_event(event), generation == 1);
            assert!(app.chats.is_empty());
            assert_eq!(app.open_chat(), None);
            assert!(app.chat_messages.is_empty());
            assert!(app.contact_search_active);
            assert_eq!(app.contact_search.input.to_string(), "+15551234567");
            assert_eq!(app.focus_pane, FocusPane::ChatList);
            if generation == 1 {
                let view = app.phone_lookup_observation.as_ref().unwrap();
                assert_eq!(view.observation.display_text(), raw);
                assert!(app.pending_phone_lookup.is_none());
                assert!(matches!(
                    app.action_notice,
                    Some(ActionNotice::PhoneLookup(ref text))
                        if text == "Local number response shown; Esc closes"
                ));
            } else {
                assert!(app.phone_lookup_observation.is_none());
                assert_eq!(app.pending_phone_lookup.as_ref().unwrap().0, 2);
                assert!(app.action_notice.is_none());
            }
        }
    }

    #[test]
    fn stale_lookup_after_search_edit_cannot_open_a_chat() {
        let mut app = awaiting_lookup();
        app.pending_phone_lookup = Some((2, "+15551234567".into()));
        assert!(!app.handle_phone_lookup_event(completed(
            1,
            Ok(Some(JID::from("15551234567@s.whatsapp.net".to_owned())))
        )));
        assert_eq!(
            app.pending_phone_lookup.as_ref().map(|pending| pending.0),
            Some(2)
        );
        assert!(app.chats.is_empty());
        assert_eq!(app.open_chat(), None);
        assert!(app.chat_messages.is_empty());
        assert!(app.action_notice.is_none());
    }

    #[test]
    fn stale_failure_cannot_replace_notice_or_consume_pending_lookup() {
        let mut app = awaiting_lookup();
        app.pending_phone_lookup = Some((2, "+15551234567".into()));
        app.action_notice = Some(ActionNotice::PhoneLookup("Checking number...".into()));
        assert!(!app.handle_phone_lookup_event(completed(1, Err(PhoneLookupError::RequestFailed))));
        assert_eq!(
            app.pending_phone_lookup.as_ref().map(|pending| pending.0),
            Some(2)
        );
        assert!(matches!(
            app.action_notice,
            Some(ActionNotice::PhoneLookup(ref message)) if message == "Checking number..."
        ));
        assert!(app.chats.is_empty());
        assert_eq!(app.open_chat(), None);
        assert!(app.chat_messages.is_empty());
    }
}
