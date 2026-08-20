use super::{
    PersistenceAction, Readiness, ReceiptCandidate, ReceiptKey, ReceiptKind, ReceiptSendStatus,
    RepositoryError,
};
use crate::app::App;
use crate::app::actions::Section;
use crate::app::runtime_diagnostics::Phase;
use whatsrust as wr;

impl App<'_> {
    pub fn initialize_read_receipts(&mut self, enabled: bool) {
        self.read_receipts.set_enabled(enabled);
        self.read_receipt_worker.set_enabled(enabled);
        if enabled {
            self.request_restore_load();
        }
    }
    pub fn enable_read_receipts(&mut self, enabled: bool) {
        self.initialize_read_receipts(enabled);
    }
    pub fn observe_visible_message(&mut self, message: &wr::Message, visible: bool) {
        let started = self.runtime_diagnostics.phase_started();
        let is_status =
            message.info.chat.0.as_ref() == super::super::status_projection::STATUS_BROADCAST_CHAT;
        let active = active_view(
            is_status,
            self.selected_section,
            self.open_chat.as_ref().map(|jid| jid.0.as_ref()),
            self.open_status_contact.as_ref().map(|jid| jid.0.as_ref()),
            message.info.chat.0.as_ref(),
            message.info.sender.0.as_ref(),
        );
        let pane_visible = conversation_pane_is_visible(
            self.rail_on_logout,
            self.pending_logout,
            self.logout_in_progress,
            self.attachment_viewer.is_some(),
            self.share_picker.is_some(),
            self.url_picker.is_some(),
            self.file_picker.is_some(),
            self.message_menu.is_some(),
            self.reaction_picker.is_some(),
        );
        self.read_receipts.observe(
            ReceiptCandidate {
                chat: message.info.chat.0.to_string(),
                sender: message.info.sender.0.to_string(),
                message_id: message.info.id.to_string(),
                timestamp: message.info.timestamp,
                kind: if is_status {
                    ReceiptKind::Status
                } else {
                    ReceiptKind::Chat
                },
                from_me: message.info.is_from_me,
                unsupported: message.info.chat.0.ends_with("@newsletter"),
                visible,
                active: active && pane_visible,
            },
            self.now(),
        );
        if let Some(started) = started {
            self.runtime_diagnostics
                .record_phase_finished(Phase::ReadReceiptObservationDispatch, started);
        }
    }
    pub fn set_read_receipt_readiness(&mut self, readiness: Readiness) {
        self.read_receipts.set_readiness(readiness);
    }
    pub fn complete_read_receipt(&mut self, key: &ReceiptKey, status: ReceiptSendStatus) {
        if let Some(action) = self.read_receipts.bridge_result(key, status, self.now()) {
            let retry = action.clone();
            let queued = match action {
                PersistenceAction::Complete(key) => self.read_receipt_worker.complete(key),
                PersistenceAction::Reject(key) => self.read_receipt_worker.reject(key),
            };
            if !queued {
                self.read_receipts.requeue_action(retry);
                self.read_receipts
                    .set_durability_error(RepositoryError::Unavailable);
            }
        }
    }
    pub fn dispatch_read_receipts(&mut self) {
        let started = self.runtime_diagnostics.phase_started();
        self.request_restore_load();
        while let Some(action) = self.read_receipts.take_action() {
            let retry = action.clone();
            let queued = match action {
                PersistenceAction::Complete(key) => self.read_receipt_worker.complete(key),
                PersistenceAction::Reject(key) => self.read_receipt_worker.reject(key),
            };
            if !queued {
                self.read_receipts.requeue_action(retry);
                break;
            }
        }
        while let Some(candidate) = self.read_receipts.take_staged() {
            if !self.read_receipt_worker.persist(candidate.clone()) {
                self.read_receipts.requeue_staged(candidate);
                break;
            }
        }
        self.read_receipts
            .dispatch(self.now(), &self.read_receipt_worker, None);
        if let Some(started) = started {
            self.runtime_diagnostics
                .record_phase_finished(Phase::ReadReceiptObservationDispatch, started);
        }
    }
    pub(crate) fn request_restore_load(&mut self) {
        let now = self.now();
        if self.read_receipts.restore_load_allowed(now) {
            if self.read_receipt_worker.load() {
                self.read_receipts.restore_load_submitted();
            } else {
                self.read_receipts.restore_retry_scheduled(now);
            }
        }
    }
    pub fn shutdown_read_receipt_worker(&mut self) {
        self.read_receipts.set_enabled(false);
        self.read_receipt_worker.set_enabled(false);
        self.read_receipt_worker.shutdown();
    }
}

pub fn active_view(
    is_status: bool,
    section: Section,
    open_chat: Option<&str>,
    open_status: Option<&str>,
    chat: &str,
    sender: &str,
) -> bool {
    match (is_status, section) {
        (true, Section::Status) => open_status == Some(sender),
        (false, Section::Chats) => open_chat == Some(chat),
        _ => false,
    }
}

pub fn conversation_pane_is_visible(
    rail_on_logout: bool,
    pending_logout: bool,
    logout_in_progress: bool,
    attachment: bool,
    share: bool,
    url: bool,
    file: bool,
    message_menu: bool,
    reaction_picker: bool,
) -> bool {
    !(rail_on_logout
        || pending_logout
        || logout_in_progress
        || attachment
        || share
        || url
        || file
        || message_menu
        || reaction_picker)
}
