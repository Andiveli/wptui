//! Viewport-driven receipt policy and bounded durable working state.

use std::collections::{HashSet, VecDeque};

pub mod repository_port;
pub mod sqlite_repository;
pub mod viewport;
pub mod whatsapp_adapter;
pub mod worker;
pub use repository_port::{PendingReceiptRepository, RepositoryError};
pub use viewport::{active_view, conversation_pane_is_visible};

pub const MAX_PENDING: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptKind {
    Chat,
    Status,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptCandidate {
    pub chat: String,
    pub sender: String,
    pub message_id: String,
    pub timestamp: i64,
    pub kind: ReceiptKind,
    pub from_me: bool,
    pub unsupported: bool,
    pub visible: bool,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ReceiptKey {
    pub chat: String,
    pub sender: String,
    pub message_id: String,
}

impl ReceiptCandidate {
    pub fn key(&self) -> ReceiptKey {
        ReceiptKey {
            chat: self.chat.clone(),
            sender: self.sender.clone(),
            message_id: self.message_id.clone(),
        }
    }
    fn eligible(&self) -> bool {
        self.visible
            && self.active
            && !self.from_me
            && !self.unsupported
            && valid_jid(&self.chat)
            && valid_jid(&self.sender)
            && !self.message_id.is_empty()
            && !self.message_id.contains('\0')
    }
}

fn valid_jid(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\0')
        && value
            .split_once('@')
            .is_some_and(|(user, server)| !user.is_empty() && !server.is_empty())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistResult {
    Saved,
    AlreadySent,
    Failed(RepositoryError),
}

pub trait ReadReceiptPort {
    fn send(&self, candidate: &ReceiptCandidate) -> ReceiptSendStatus;
}
pub trait ReadReceiptDispatcher {
    fn dispatch(&self, candidate: ReceiptCandidate) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptSendStatus {
    Success,
    Disconnected,
    Transient,
    Permanent,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Readiness {
    Connected,
    Disconnected,
}
#[derive(Clone, Debug)]
pub enum PersistenceAction {
    Complete(ReceiptKey),
    Reject(ReceiptKey),
}

#[derive(Debug)]
struct Pending {
    candidate: ReceiptCandidate,
    attempts: u32,
    next_attempt: i64,
}

#[derive(Debug)]
pub struct Coordinator {
    enabled: bool,
    readiness: Readiness,
    pending: VecDeque<Pending>,
    staged: VecDeque<ReceiptCandidate>,
    volatile_retry: VecDeque<ReceiptCandidate>,
    durability_error: Option<RepositoryError>,
    deferred_actions: VecDeque<PersistenceAction>,
    restore_retry_at: Option<i64>,
    restore_attempts: u32,
    load_in_flight: bool,
    sent: HashSet<ReceiptKey>,
    in_flight: HashSet<ReceiptKey>,
}

impl Default for Coordinator {
    fn default() -> Self {
        Self {
            enabled: true,
            readiness: Readiness::Disconnected,
            pending: VecDeque::new(),
            staged: VecDeque::new(),
            volatile_retry: VecDeque::new(),
            durability_error: None,
            deferred_actions: VecDeque::new(),
            restore_retry_at: None,
            restore_attempts: 0,
            load_in_flight: false,
            sent: HashSet::new(),
            in_flight: HashSet::new(),
        }
    }
}

impl Coordinator {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.load_in_flight = false;
            self.restore_retry_at = None;
        }
    }
    pub fn set_readiness(&mut self, readiness: Readiness) {
        self.readiness = readiness;
    }
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
    pub fn durability_error(&self) -> Option<RepositoryError> {
        self.durability_error
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn restore_retry_due(&self, now: i64) -> bool {
        self.enabled && !self.load_in_flight && self.restore_retry_at.is_some_and(|at| at <= now)
    }
    pub fn restore_load_allowed(&self, now: i64) -> bool {
        self.enabled && !self.load_in_flight && self.restore_retry_at.is_none_or(|at| at <= now)
    }
    pub fn restore_load_submitted(&mut self) {
        self.load_in_flight = true;
    }
    pub fn restore_retry_scheduled(&mut self, now: i64) {
        let delay = (1_i64 << self.restore_attempts.min(5)).min(30);
        self.restore_attempts = self.restore_attempts.saturating_add(1);
        self.restore_retry_at = Some(now + delay);
    }
    pub(crate) fn set_durability_error(&mut self, error: RepositoryError) {
        self.durability_error = Some(error);
    }
    pub fn in_flight_len(&self) -> usize {
        self.in_flight.len()
    }
    pub fn is_sent(&self, key: &ReceiptKey) -> bool {
        self.sent.contains(key)
    }

    pub fn restore(&mut self, repository: &dyn PendingReceiptRepository, now: i64) {
        if !self.enabled {
            return;
        }
        let candidates = match repository.load() {
            Ok(candidates) => candidates,
            Err(error) => {
                self.durability_error = Some(error);
                return;
            }
        };
        for candidate in candidates {
            if self.pending.len() >= MAX_PENDING {
                break;
            }
            if !self.contains(&candidate.key())
                && repository.was_sent(&candidate.key()).ok() != Some(true)
            {
                self.pending.push_back(Pending {
                    candidate,
                    attempts: 0,
                    next_attempt: now,
                });
            }
        }
    }

    fn contains(&self, key: &ReceiptKey) -> bool {
        self.sent.contains(key)
            || self.in_flight.contains(key)
            || self.pending.iter().any(|item| item.candidate.key() == *key)
    }

    pub fn observe(&mut self, candidate: ReceiptCandidate, _now: i64) {
        if !self.enabled || !candidate.eligible() || self.contains(&candidate.key()) {
            return;
        }
        if self.staged.len() + self.volatile_retry.len() >= MAX_PENDING {
            self.durability_error = Some(RepositoryError::Unavailable);
            return;
        }
        self.staged.push_back(candidate);
    }

    pub fn restore_candidates(&mut self, candidates: Vec<ReceiptCandidate>, now: i64) {
        self.load_in_flight = false;
        if !self.enabled {
            return;
        }
        self.restore_retry_at = None;
        self.restore_attempts = 0;
        self.durability_error = None;
        for candidate in candidates {
            if self.pending.len() >= MAX_PENDING {
                break;
            }
            if !self.contains(&candidate.key()) {
                self.pending.push_back(Pending {
                    candidate,
                    attempts: 0,
                    next_attempt: now,
                });
            }
        }
    }
    pub fn restore_failed(&mut self, now: i64, error: RepositoryError) {
        self.load_in_flight = false;
        self.durability_error = Some(error);
        self.restore_retry_scheduled(now);
    }
    pub fn take_staged(&mut self) -> Option<ReceiptCandidate> {
        if self.enabled {
            self.volatile_retry
                .pop_front()
                .or_else(|| self.staged.pop_front())
        } else {
            None
        }
    }
    pub fn requeue_staged(&mut self, candidate: ReceiptCandidate) {
        self.staged.push_front(candidate);
    }
    pub fn requeue_action(&mut self, action: PersistenceAction) {
        self.deferred_actions.push_front(action);
    }
    pub fn take_action(&mut self) -> Option<PersistenceAction> {
        self.deferred_actions.pop_front()
    }
    pub fn persisted(&mut self, candidate: ReceiptCandidate, result: PersistResult, now: i64) {
        match result {
            PersistResult::Saved => {
                if self.pending.len() < MAX_PENDING {
                    self.pending.push_back(Pending {
                        candidate,
                        attempts: 0,
                        next_attempt: now,
                    });
                }
            }
            PersistResult::AlreadySent => {}
            PersistResult::Failed(error) => self.retain_persistence_failure(candidate, error),
        }
    }
    pub fn persistence_completed(&mut self, key: &ReceiptKey, result: Result<(), RepositoryError>) {
        match result {
            Ok(()) => {
                self.sent.insert(key.clone());
                self.pending.retain(|item| item.candidate.key() != *key);
            }
            Err(error) => self.durability_error = Some(error),
        }
    }
    pub fn persistence_rejected(&mut self, key: &ReceiptKey, result: Result<(), RepositoryError>) {
        match result {
            Ok(()) => self.pending.retain(|item| item.candidate.key() != *key),
            Err(error) => self.durability_error = Some(error),
        }
    }
    pub fn bridge_result(
        &mut self,
        key: &ReceiptKey,
        status: ReceiptSendStatus,
        now: i64,
    ) -> Option<PersistenceAction> {
        self.in_flight.remove(key);
        match status {
            ReceiptSendStatus::Success => Some(PersistenceAction::Complete(key.clone())),
            ReceiptSendStatus::Permanent => Some(PersistenceAction::Reject(key.clone())),
            ReceiptSendStatus::Disconnected => {
                self.readiness = Readiness::Disconnected;
                if let Some(item) = self
                    .pending
                    .iter_mut()
                    .find(|item| item.candidate.key() == *key)
                {
                    item.next_attempt = now + 2;
                }
                None
            }
            ReceiptSendStatus::Transient => {
                if let Some(item) = self
                    .pending
                    .iter_mut()
                    .find(|item| item.candidate.key() == *key)
                {
                    item.attempts = item.attempts.saturating_add(1);
                    item.next_attempt = now + i64::from(item.attempts.min(30));
                }
                None
            }
        }
    }

    pub fn persist(&mut self, now: i64, repository: &dyn PendingReceiptRepository) {
        if !self.enabled {
            return;
        }
        let staged = self.staged.len();
        for _ in 0..staged {
            let Some(candidate) = self.staged.pop_front() else {
                break;
            };
            match repository.was_sent(&candidate.key()) {
                Ok(true) => continue,
                Err(error) => {
                    self.retain_persistence_failure(candidate, error);
                    continue;
                }
                Ok(false) => {}
            }
            match repository.save(&candidate) {
                Ok(()) => {
                    self.durability_error = None;
                    if self.pending.len() < MAX_PENDING {
                        self.pending.push_back(Pending {
                            candidate,
                            attempts: 0,
                            next_attempt: now,
                        });
                    }
                }
                Err(error) => self.retain_persistence_failure(candidate, error),
            }
        }
        let retry = self.volatile_retry.len();
        for _ in 0..retry {
            let Some(candidate) = self.volatile_retry.pop_front() else {
                break;
            };
            match repository.save(&candidate) {
                Ok(()) => {
                    self.durability_error = None;
                    if self.pending.len() < MAX_PENDING {
                        self.pending.push_back(Pending {
                            candidate,
                            attempts: 0,
                            next_attempt: now,
                        });
                    }
                }
                Err(error) => self.retain_persistence_failure(candidate, error),
            }
        }
    }

    fn retain_persistence_failure(&mut self, candidate: ReceiptCandidate, error: RepositoryError) {
        self.durability_error = Some(error);
        if self.volatile_retry.len() < MAX_PENDING {
            self.volatile_retry.push_back(candidate);
        }
    }

    pub fn dispatch(
        &mut self,
        now: i64,
        dispatcher: &dyn ReadReceiptDispatcher,
        repository: Option<&dyn PendingReceiptRepository>,
    ) {
        if !self.enabled || self.readiness != Readiness::Connected {
            return;
        }
        let count = self.pending.len();
        for _ in 0..count {
            let Some(item) = self.pending.pop_front() else {
                break;
            };
            if let Some(repository) = repository {
                match repository.was_sent(&item.candidate.key()) {
                    Ok(true) => continue,
                    Ok(false) => {}
                    Err(error) => {
                        self.durability_error = Some(error);
                        self.pending.push_back(item);
                        continue;
                    }
                }
            }
            if item.next_attempt > now || self.in_flight.contains(&item.candidate.key()) {
                self.pending.push_back(item);
                continue;
            }
            let key = item.candidate.key();
            if dispatcher.dispatch(item.candidate.clone()) {
                self.in_flight.insert(key);
            }
            self.pending.push_back(item);
        }
    }

    #[cfg(test)]
    fn complete(
        &mut self,
        key: &ReceiptKey,
        status: ReceiptSendStatus,
        now: i64,
        repository: &dyn PendingReceiptRepository,
    ) {
        self.in_flight.remove(key);
        match status {
            ReceiptSendStatus::Success => {
                if repository.complete_success(key).is_ok() {
                    self.sent.insert(key.clone());
                    self.pending.retain(|item| item.candidate.key() != *key);
                    self.restore(repository, now);
                }
            }
            ReceiptSendStatus::Disconnected => {
                self.readiness = Readiness::Disconnected;
                if let Some(item) = self
                    .pending
                    .iter_mut()
                    .find(|item| item.candidate.key() == *key)
                {
                    item.next_attempt = now + 2;
                }
            }
            ReceiptSendStatus::Transient => {
                if let Some(item) = self
                    .pending
                    .iter_mut()
                    .find(|item| item.candidate.key() == *key)
                {
                    item.attempts = item.attempts.saturating_add(1);
                    item.next_attempt = now + i64::from(item.attempts.min(30));
                }
            }
            ReceiptSendStatus::Permanent => {
                if repository.reject(key).is_ok() {
                    self.pending.retain(|item| item.candidate.key() != *key);
                } else {
                    self.durability_error = Some(RepositoryError::Unavailable);
                }
            }
        }
    }
}

pub fn intersects(top: i64, bottom: i64, area_top: i64, area_bottom: i64) -> bool {
    top < area_bottom && bottom > area_top
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::Section;
    struct Repo(std::cell::RefCell<Vec<ReceiptCandidate>>);
    impl PendingReceiptRepository for Repo {
        fn load(&self) -> Result<Vec<ReceiptCandidate>, RepositoryError> {
            Ok(self.0.borrow().clone())
        }
        fn save(&self, c: &ReceiptCandidate) -> Result<(), RepositoryError> {
            self.0.borrow_mut().push(c.clone());
            Ok(())
        }
        fn was_sent(&self, _: &ReceiptKey) -> Result<bool, RepositoryError> {
            Ok(false)
        }
        fn complete_success(&self, _: &ReceiptKey) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn reject(&self, _: &ReceiptKey) -> Result<(), RepositoryError> {
            Ok(())
        }
    }
    struct Dispatcher(std::cell::Cell<usize>);
    impl ReadReceiptDispatcher for Dispatcher {
        fn dispatch(&self, _: ReceiptCandidate) -> bool {
            self.0.set(self.0.get() + 1);
            true
        }
    }
    struct SentRepo;
    impl PendingReceiptRepository for SentRepo {
        fn load(&self) -> Result<Vec<ReceiptCandidate>, RepositoryError> {
            Ok(vec![candidate("stale")])
        }
        fn save(&self, _: &ReceiptCandidate) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn was_sent(&self, _: &ReceiptKey) -> Result<bool, RepositoryError> {
            Ok(true)
        }
        fn complete_success(&self, _: &ReceiptKey) -> Result<(), RepositoryError> {
            Ok(())
        }
        fn reject(&self, _: &ReceiptKey) -> Result<(), RepositoryError> {
            Ok(())
        }
    }
    struct FailingRepo;
    impl PendingReceiptRepository for FailingRepo {
        fn load(&self) -> Result<Vec<ReceiptCandidate>, RepositoryError> {
            Ok(Vec::new())
        }
        fn save(&self, _: &ReceiptCandidate) -> Result<(), RepositoryError> {
            Err(RepositoryError::Busy)
        }
        fn was_sent(&self, _: &ReceiptKey) -> Result<bool, RepositoryError> {
            Err(RepositoryError::Busy)
        }
        fn complete_success(&self, _: &ReceiptKey) -> Result<(), RepositoryError> {
            Err(RepositoryError::Busy)
        }
        fn reject(&self, _: &ReceiptKey) -> Result<(), RepositoryError> {
            Err(RepositoryError::Busy)
        }
    }
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
    fn successful_send_deduplicates_only_after_success() {
        let r = Repo(std::cell::RefCell::new(Vec::new()));
        let mut c = Coordinator::default();
        c.observe(candidate("id"), 0);
        c.persist(0, &r);
        c.set_readiness(Readiness::Connected);
        let d = Dispatcher(std::cell::Cell::new(0));
        c.dispatch(0, &d, Some(&r));
        c.complete(&candidate("id").key(), ReceiptSendStatus::Success, 0, &r);
        assert!(c.is_sent(&candidate("id").key()));
        assert_eq!(c.pending_len(), 0);
    }
    #[test]
    fn transient_completion_is_retryable_indefinitely() {
        let r = Repo(std::cell::RefCell::new(Vec::new()));
        let mut c = Coordinator::default();
        c.observe(candidate("id"), 0);
        c.persist(0, &r);
        c.complete(&candidate("id").key(), ReceiptSendStatus::Transient, 1, &r);
        assert_eq!(c.pending_len(), 1);
    }
    #[test]
    fn durable_overflow_is_not_evicted() {
        let r = Repo(std::cell::RefCell::new(Vec::new()));
        let mut c = Coordinator::default();
        let total = MAX_PENDING + 10;
        for i in 0..total {
            let mut x = candidate(&i.to_string());
            x.sender = format!("sender-{i}@s.whatsapp.net");
            c.observe(x, 0);
            c.persist(0, &r);
        }
        assert_eq!(c.pending_len(), MAX_PENDING);
        assert_eq!(r.load().unwrap().len(), total);

        c.set_readiness(Readiness::Connected);
        let d = Dispatcher(std::cell::Cell::new(0));
        for i in 0..total {
            c.dispatch(0, &d, Some(&r));
            let mut sent = candidate(&i.to_string());
            sent.sender = format!("sender-{i}@s.whatsapp.net");
            c.complete(&sent.key(), ReceiptSendStatus::Success, 0, &r);
            assert!(c.pending_len() <= MAX_PENDING);
        }
        assert_eq!(d.0.get(), total);
        assert_eq!(c.pending_len(), 0);
    }
    #[test]
    fn no_duplicate_in_flight() {
        let r = Repo(std::cell::RefCell::new(Vec::new()));
        let d = Dispatcher(std::cell::Cell::new(0));
        let mut c = Coordinator::default();
        c.set_readiness(Readiness::Connected);
        c.observe(candidate("id"), 0);
        c.persist(0, &r);
        c.dispatch(0, &d, Some(&r));
        c.dispatch(0, &d, Some(&r));
        assert_eq!(d.0.get(), 1);
    }
    #[test]
    fn stale_pending_is_filtered_by_durable_sent_identity() {
        let r = SentRepo;
        let d = Dispatcher(std::cell::Cell::new(0));
        let mut c = Coordinator::default();
        c.restore(&r, 0);
        c.set_readiness(Readiness::Connected);
        c.dispatch(0, &d, Some(&r));
        assert_eq!(d.0.get(), 0);
    }
    #[test]
    fn offline_retention_dispatches_after_reconnect() {
        let r = Repo(std::cell::RefCell::new(Vec::new()));
        let d = Dispatcher(std::cell::Cell::new(0));
        let mut c = Coordinator::default();
        c.observe(candidate("id"), 0);
        c.persist(0, &r);
        c.dispatch(0, &d, Some(&r));
        assert_eq!(d.0.get(), 0);
        c.set_readiness(Readiness::Connected);
        c.dispatch(0, &d, Some(&r));
        assert_eq!(d.0.get(), 1);
    }
    #[test]
    fn exclusions_are_policy_rejections() {
        let r = Repo(std::cell::RefCell::new(Vec::new()));
        let mut c = Coordinator::default();
        for (from_me, unsupported, active) in [
            (true, false, true),
            (false, true, true),
            (false, false, false),
        ] {
            let mut x = candidate("id");
            x.from_me = from_me;
            x.unsupported = unsupported;
            x.active = active;
            c.observe(x, 0);
        }
        assert_eq!(r.load().unwrap().len(), 0);
    }
    #[test]
    fn malformed_jids_are_rejected_before_persistence() {
        let r = Repo(std::cell::RefCell::new(Vec::new()));
        let mut c = Coordinator::default();
        let mut invalid = candidate("id");
        invalid.sender = "not-a-jid".into();
        c.observe(invalid, 0);
        c.persist(0, &r);
        assert_eq!(r.load().unwrap().len(), 0);
    }
    #[test]
    fn persistence_failure_is_retained_and_observable() {
        let r = FailingRepo;
        let mut c = Coordinator::default();
        c.observe(candidate("id"), 0);
        c.persist(0, &r);
        assert_eq!(c.durability_error(), Some(RepositoryError::Busy));
    }
    #[test]
    fn disabled_policy_does_not_restore_persist_or_dispatch() {
        let r = Repo(std::cell::RefCell::new(vec![candidate("restored")]));
        let d = Dispatcher(std::cell::Cell::new(0));
        let mut c = Coordinator::default();
        c.set_enabled(false);
        c.restore(&r, 0);
        c.observe(candidate("new"), 0);
        c.persist(0, &r);
        c.set_readiness(Readiness::Connected);
        c.dispatch(0, &d, Some(&r));
        assert_eq!(d.0.get(), 0);
        assert_eq!(r.load().unwrap().len(), 1);
    }
    #[test]
    fn restore_load_failure_backoff_then_recovers_without_restart() {
        let mut c = Coordinator::default();
        assert!(c.restore_load_allowed(10));
        c.restore_load_submitted();
        assert!(!c.restore_load_allowed(10));
        assert!(!c.restore_load_allowed(11));
        c.restore_failed(10, RepositoryError::Busy);
        assert!(!c.restore_retry_due(10));
        assert!(c.restore_retry_due(11));
        c.restore_load_submitted();
        assert!(!c.restore_load_allowed(11));
        c.restore_candidates(vec![candidate("after-retry")], 11);
        assert_eq!(c.pending_len(), 1);
        assert_eq!(c.durability_error(), None);
    }
    #[test]
    fn geometry_requires_positive_intersection() {
        assert!(intersects(9, 11, 10, 20));
        assert!(!intersects(20, 25, 10, 20));
        assert!(!intersects(0, 10, 10, 20));
    }
    #[test]
    fn chat_and_status_identity_keys_do_not_collide() {
        let r = Repo(std::cell::RefCell::new(Vec::new()));
        let mut c = Coordinator::default();
        let mut status = candidate("same");
        status.kind = ReceiptKind::Status;
        status.chat = "status@broadcast".into();
        c.observe(candidate("same"), 0);
        c.observe(status, 0);
        c.persist(0, &r);
        assert_eq!(r.load().unwrap().len(), 2);
    }
    #[test]
    fn active_view_requires_matching_section_and_open_target() {
        assert!(active_view(
            false,
            Section::Chats,
            Some("chat"),
            None,
            "chat",
            "sender"
        ));
        assert!(!active_view(
            false,
            Section::Status,
            Some("chat"),
            None,
            "chat",
            "sender"
        ));
        assert!(active_view(
            true,
            Section::Status,
            None,
            Some("sender"),
            "status@broadcast",
            "sender"
        ));
    }
    #[test]
    fn overlays_and_logout_hide_conversation_pane() {
        assert!(conversation_pane_is_visible(
            false, false, false, false, false, false, false, false, false
        ));
        assert!(!conversation_pane_is_visible(
            false, false, false, true, false, false, false, false, false
        ));
        assert!(!conversation_pane_is_visible(
            true, false, false, false, false, false, false, false, false
        ));
        assert!(!conversation_pane_is_visible(
            false, false, false, false, false, false, false, true, false
        ));
        assert!(!conversation_pane_is_visible(
            false, false, false, false, false, false, false, false, true
        ));
    }
}
