use whatsrust as wr;

pub trait ChatReadSyncPort: Send + 'static {
    fn schedule(
        &mut self,
        chat: &wr::JID,
        message_id: &wr::MessageId,
        timestamp: i64,
        from_me: bool,
        participant: Option<&wr::JID>,
    ) -> bool;

    /// Schedules chat-level app-state, not message receipts. `None` means no messages.
    /// Legacy ports can handle reads with identity; unsupported states return false.
    fn schedule_state(
        &mut self,
        chat: &wr::JID,
        message_id: Option<&wr::MessageId>,
        read: bool,
        timestamp: i64,
        from_me: bool,
        participant: Option<&wr::JID>,
    ) -> bool {
        read && message_id
            .is_some_and(|id| self.schedule(chat, id, timestamp, from_me, participant))
    }

    fn shutdown(&mut self);

    fn restart(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct LegacyPort(usize);

    impl ChatReadSyncPort for LegacyPort {
        fn schedule(
            &mut self,
            _: &wr::JID,
            _: &wr::MessageId,
            _: i64,
            _: bool,
            _: Option<&wr::JID>,
        ) -> bool {
            self.0 += 1;
            true
        }
        fn shutdown(&mut self) {}
        fn restart(&mut self) {}
    }

    #[test]
    fn state_default_preserves_legacy_reads_without_sending_unsupported_states() {
        let mut port = LegacyPort::default();
        let chat = wr::JID::from("chat@s.whatsapp.net".to_owned());
        let id = wr::MessageId::from("latest");
        assert!(port.schedule(&chat, &id, 42, false, None));
        assert!(port.schedule_state(&chat, Some(&id), true, 42, false, None));
        assert_eq!(port.0, 2);
        assert!(!port.schedule_state(&chat, Some(&id), false, 42, false, None));
        assert!(!port.schedule_state(&chat, None, true, 0, false, None));
        assert_eq!(port.0, 2);
    }
}
