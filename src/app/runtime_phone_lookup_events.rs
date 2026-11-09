use super::App;
use super::actions::{FocusPane, Section};
use super::events::AppEvent;
use whatsrust::PhoneLookupError;

impl App<'_> {
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
        match result {
            Ok(Some(jid)) => {
                let expected = phone
                    .strip_prefix('+')
                    .map(|digits| format!("{digits}@s.whatsapp.net"));
                if expected.as_deref() != Some(jid.0.as_ref()) {
                    self.unavailable("Could not verify number");
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
                self.unavailable("Number is not on WhatsApp");
                true
            }
            Err(error) => {
                self.unavailable(match error {
                    PhoneLookupError::InvalidNumber => "Use + followed by 7-15 digits",
                    PhoneLookupError::ClientUnavailable => "WhatsApp is not connected",
                    PhoneLookupError::RequestFailed => "Could not check number",
                    PhoneLookupError::InvalidBridgeResult => "Could not verify number",
                    PhoneLookupError::TimedOut => "Number check timed out",
                    PhoneLookupError::EmptyResponse => "WhatsApp returned no number result",
                    PhoneLookupError::ResponseMismatch => {
                        "WhatsApp returned a mismatched number result"
                    }
                });
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::ActionNotice;
    use crate::app::test_support::TestApp;
    use whatsrust::JID;

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
                matches!(app.action_notice, Some(ActionNotice::Unavailable(ref message)) if message == notice)
            );
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
        app.action_notice = Some(ActionNotice::Unavailable("Checking number...".into()));
        assert!(!app.handle_phone_lookup_event(completed(1, Err(PhoneLookupError::RequestFailed))));
        assert_eq!(
            app.pending_phone_lookup.as_ref().map(|pending| pending.0),
            Some(2)
        );
        assert!(matches!(
            app.action_notice,
            Some(ActionNotice::Unavailable(ref message)) if message == "Checking number..."
        ));
        assert!(app.chats.is_empty());
        assert_eq!(app.open_chat(), None);
        assert!(app.chat_messages.is_empty());
    }
}
