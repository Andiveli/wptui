use crate::app::App;
use crate::app::share_picker::{SharePicker, is_forwardable_recipient};

impl App<'_> {
    pub(crate) fn open_share_picker(&mut self) {
        if self.selected_message_is_deleted() {
            return self.unavailable("This message was deleted.");
        }
        let Some(message) = self.selected_message() else {
            return self.unavailable("Forward is not available");
        };
        if !is_forwardable_recipient(&message.info.chat) {
            return self.unavailable("Forward is not available");
        }
        let contacts = self
            .contacts
            .keys()
            .filter(|jid| is_forwardable_recipient(jid))
            .cloned()
            .collect();
        let labels = self
            .contacts
            .iter()
            .map(|(jid, name)| (jid.clone(), name.to_string()))
            .collect();
        let recency = self
            .chats
            .iter()
            .filter_map(|(jid, chat)| chat.last_message_time.map(|time| (jid.clone(), time)))
            .collect();
        self.share_picker = Some(SharePicker::new(contacts, labels, recency));
    }

    pub(crate) fn move_share_picker(&mut self, delta: isize) {
        if let Some(picker) = self.share_picker.as_mut() {
            picker.move_selection(delta);
        }
    }

    pub(crate) fn toggle_share_recipient(&mut self) {
        let Some(picker) = self.share_picker.as_mut() else {
            return;
        };
        picker.toggle_selected();
    }

    pub(crate) fn share_search_backspace(&mut self) {
        if let Some(picker) = self.share_picker.as_mut() {
            picker.search_backspace();
        }
    }

    pub(crate) fn share_search_character(&mut self, character: char) {
        if let Some(picker) = self.share_picker.as_mut() {
            picker.search_character(character);
        }
    }

    pub(crate) fn confirm_share(&mut self) {
        let Some(picker) = self.share_picker.as_ref() else {
            return;
        };
        let destinations = picker.destinations();
        if destinations.is_empty() {
            return self.unavailable("Select at least one contact");
        }
        let Some(message) = self.selected_message().cloned() else {
            self.share_picker = None;
            return self.unavailable("Forward is not available");
        };
        self.share_picker = None;
        let report = self
            .message_forwarder
            .forward_message(&message, &destinations);
        self.action_notice = Some(crate::app::actions::ActionNotice::Forwarded {
            succeeded: report.succeeded,
            failed: report.failed,
            failure: report.failure,
        });
    }
}
