use crate::app::events::AppEvent;
use crate::app::App;

impl App<'_> {
    pub(crate) fn handle_read_receipt_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::ReadReceiptResult(key, status) => {
                self.complete_read_receipt(&key, status);
                false
            }
            AppEvent::ReadReceiptRestored(result) => {
                match result {
                    Ok(candidates) => self
                        .read_receipts
                        .restore_candidates(candidates, self.now()),
                    Err(error) => self.read_receipts.restore_failed(self.now(), error),
                }
                false
            }
            AppEvent::ReadReceiptPersisted(candidate, result) => {
                self.read_receipts.persisted(candidate, result, self.now());
                false
            }
            AppEvent::ReadReceiptCompleted(key, result) => {
                let success = result.is_ok();
                self.read_receipts.persistence_completed(&key, result);
                if success && self.read_receipts.enabled() {
                    self.read_receipts.restore_load_needed();
                    self.request_restore_load();
                }
                false
            }
            AppEvent::ReadReceiptRejected(key, result) => {
                let success = result.is_ok();
                self.read_receipts.persistence_rejected(&key, result);
                if success && self.read_receipts.enabled() {
                    self.read_receipts.restore_load_needed();
                    self.request_restore_load();
                }
                false
            }
            _ => unreachable!("runtime_loop must route only ReadReceipt events to handle_read_receipt_event"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::events::AppEvent;
    use crate::app::read_receipts::{ReceiptCandidate, ReceiptKind, RepositoryError};
    use crate::app::test_support::TestApp;

    fn candidate(id: &str) -> ReceiptCandidate {
        ReceiptCandidate {
            chat: "chat@s.whatsapp.net".into(),
            sender: "sender@s.whatsapp.net".into(),
            message_id: id.into(),
            timestamp: 1,
            kind: ReceiptKind::Chat,
            from_me: false,
            unsupported: false,
            visible: true,
            active: true,
        }
    }

    #[test]
    fn result_receipts_route_without_redraw() {
        let mut app = TestApp::new();
        let key = candidate("result").key();

        assert!(!app.handle_read_receipt_event(AppEvent::ReadReceiptResult(
            key,
            crate::app::read_receipts::ReceiptSendStatus::Transient,
        )));
    }

    #[test]
    fn restored_receipts_route_without_redraw() {
        let mut app = TestApp::new();
        app.read_receipts.set_enabled(true);

        assert!(!app.handle_read_receipt_event(AppEvent::ReadReceiptRestored(Ok(vec![
            candidate("restored"),
        ]))));
        assert_eq!(app.read_receipts.pending_len(), 1);
    }

    #[test]
    fn restore_failures_preserve_durability_error_without_redraw() {
        let mut app = TestApp::new();
        app.read_receipts.set_enabled(true);

        assert!(!app.handle_read_receipt_event(AppEvent::ReadReceiptRestored(Err(
            RepositoryError::Busy,
        ))));
        assert_eq!(app.read_receipts.durability_error(), Some(RepositoryError::Busy));
    }

    #[test]
    fn persisted_receipts_preserve_pending_state_without_redraw() {
        let mut app = TestApp::new();
        app.read_receipts.set_enabled(true);

        assert!(!app.handle_read_receipt_event(AppEvent::ReadReceiptPersisted(
            candidate("persisted"),
            crate::app::read_receipts::PersistResult::Saved,
        )));
        assert_eq!(app.read_receipts.pending_len(), 1);
    }

    #[test]
    fn completed_receipts_route_without_redraw_and_request_restore() {
        let mut app = TestApp::new();
        app.read_receipts.set_enabled(true);
        let candidate = candidate("completed");
        let key = candidate.key();
        let now = app.now();
        app.read_receipts.restore_candidates(vec![candidate], now);

        assert!(!app.handle_read_receipt_event(AppEvent::ReadReceiptCompleted(key.clone(), Ok(()))));
        assert!(app.read_receipts.is_sent(&key));
        assert_eq!(app.read_receipts.pending_len(), 0);
        assert!(!app.read_receipts.restore_load_allowed(app.now()));
    }

    #[test]
    fn rejected_receipts_route_without_redraw_and_request_restore() {
        let mut app = TestApp::new();
        app.read_receipts.set_enabled(true);
        let candidate = candidate("rejected");
        let key = candidate.key();
        let now = app.now();
        app.read_receipts.restore_candidates(vec![candidate], now);

        assert!(!app.handle_read_receipt_event(AppEvent::ReadReceiptRejected(key, Ok(()))));
        assert_eq!(app.read_receipts.pending_len(), 0);
        assert!(!app.read_receipts.restore_load_allowed(app.now()));
    }
}
