use std::collections::HashSet;

use super::{
    PersistenceAction, Readiness, ReceiptCandidate, ReceiptKey, ReceiptKind, ReceiptSendStatus,
    RepositoryError,
};
use crate::app::App;
use crate::app::actions::Section;
use crate::app::runtime_diagnostics::Phase;
use whatsrust as wr;

/// Visibility facts collected during one successful terminal draw.
///
/// Rendering records only message identity and geometry-derived visibility here.
/// Receipt policy, persistence, and dispatch remain runtime responsibilities.
#[derive(Default)]
pub struct VisibilityPlan {
    observations: Vec<VisibleMessage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VisibleMessage {
    chat: String,
    sender: String,
    message_id: String,
    timestamp: i64,
    from_me: bool,
}

impl VisibilityPlan {
    pub fn record_visible_message(&mut self, message: &wr::Message) {
        self.observations.push(VisibleMessage {
            chat: message.info.chat.0.to_string(),
            sender: message.info.sender.0.to_string(),
            message_id: message.info.id.to_string(),
            timestamp: message.info.timestamp,
            from_me: message.info.is_from_me,
        });
    }

    fn into_observations(self) -> Vec<VisibleMessage> {
        let mut seen = HashSet::new();
        self.observations
            .into_iter()
            .filter(|message| {
                seen.insert((
                    message.chat.clone(),
                    message.sender.clone(),
                    message.message_id.clone(),
                ))
            })
            .collect()
    }
}

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
    pub fn apply_visibility_plan(&mut self, plan: VisibilityPlan) {
        for message in plan.into_observations() {
            self.apply_visible_message(message);
        }
    }
    fn apply_visible_message(&mut self, message: VisibleMessage) {
        let is_status = message.chat == super::super::status_projection::STATUS_BROADCAST_CHAT;
        let active = active_view(
            is_status,
            self.selected_section,
            self.open_chat.as_ref().map(|jid| jid.0.as_ref()),
            self.open_status_contact.as_ref().map(|jid| jid.0.as_ref()),
            &message.chat,
            &message.sender,
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
                unsupported: message.chat.ends_with("@newsletter"),
                chat: message.chat,
                sender: message.sender,
                message_id: message.message_id,
                timestamp: message.timestamp,
                kind: if is_status {
                    ReceiptKind::Status
                } else {
                    ReceiptKind::Chat
                },
                from_me: message.from_me,
                visible: true,
                active: active && pane_visible,
            },
            self.now(),
        );
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
        self.optimistic_text_send_worker.shutdown();
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

#[cfg(test)]
mod tests {
    use super::VisibilityPlan;
    use crate::app::test_support::{TestApp, message};
    use whatsrust as wr;

    #[test]
    fn uncommitted_visibility_plan_does_not_stage_receipts() {
        let mut app = TestApp::new();
        let chat = wr::JID::from("alice@example.test".to_owned());
        let mut plan = VisibilityPlan::default();
        plan.record_visible_message(&message(&chat, "visible", 1));

        assert_eq!(app.read_receipts.take_staged(), None);
    }

    #[test]
    fn applying_visibility_plan_stages_active_chat_receipt_once() {
        let mut app = TestApp::new();
        let chat = wr::JID::from("alice@example.test".to_owned());
        app.open_chat_by_jid(chat.clone());
        let message = message(&chat, "visible", 1);
        let mut plan = VisibilityPlan::default();
        plan.record_visible_message(&message);
        plan.record_visible_message(&message);

        app.apply_visibility_plan(plan);

        let candidate = app.read_receipts.take_staged().expect("receipt is staged");
        assert_eq!(candidate.message_id, "visible");
        assert_eq!(app.read_receipts.take_staged(), None);
    }
}
