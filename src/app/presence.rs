use std::collections::{HashMap, VecDeque};
use std::io::{self, Write};
use std::time::Duration;

use whatsrust as wr;

pub const RECENTLY_OFFLINE_SECONDS: i64 = 5 * 60;
pub const ONLINE_TTL_SECONDS: i64 = 5 * 60;
const MAX_CACHED_CONTACTS: usize = 256;
const MAX_SUBSCRIPTION_RETRY_SECONDS: i64 = 30;
const MAX_DIAGNOSTIC_ENTRIES: usize = 50;

#[derive(Clone, Debug, Default)]
pub struct PresenceDiagnostics {
    entries: Option<VecDeque<String>>,
    presence_events: usize,
}

impl PresenceDiagnostics {
    pub fn new(enabled: bool) -> Self {
        Self {
            entries: enabled.then(VecDeque::new),
            presence_events: 0,
        }
    }

    pub fn record(&mut self, entry: impl FnOnce() -> String) {
        let Some(entries) = self.entries.as_mut() else {
            return;
        };
        if entries.len() == MAX_DIAGNOSTIC_ENTRIES {
            entries.pop_front();
        }
        entries.push_back(entry());
    }

    pub fn record_presence(&mut self, entry: impl FnOnce() -> String) {
        if self.entries.is_some() {
            self.presence_events += 1;
        }
        self.record(entry);
    }

    pub fn write_report(&self, mut output: impl Write) -> io::Result<()> {
        let Some(entries) = self.entries.as_ref() else {
            return Ok(());
        };
        writeln!(output, "Presence diagnostics:")?;
        writeln!(
            output,
            "Rust translated Presence updates: {}",
            self.presence_events
        )?;
        for (index, entry) in entries.iter().enumerate() {
            writeln!(output, "{}. {entry}", index + 1)?;
        }
        Ok(())
    }

    pub fn write_raw_report(&self, mut output: impl Write, report: Option<&str>) -> io::Result<()> {
        if self.entries.is_none() {
            return Ok(());
        }
        let Some(report) = report else {
            return Ok(());
        };
        writeln!(output, "\nGo raw presence diagnostics:")?;
        write!(output, "{report}")
    }
}

pub fn debug_enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresenceMarker {
    Online,
    RecentlyOffline,
    Offline,
}

#[derive(Clone, Debug)]
struct PresenceObservation {
    online: bool,
    unavailable_since: Option<i64>,
    observed_at: i64,
}

#[derive(Clone, Debug, Default)]
pub struct SelectedPresence {
    selected: Option<wr::JID>,
    subscribed: Option<wr::JID>,
    ready: bool,
    terminal_rejection: bool,
    observations: HashMap<wr::JID, PresenceObservation>,
    observation_recency: VecDeque<wr::JID>,
    subscription_retry_at: Option<i64>,
    subscription_failures: u32,
}

impl SelectedPresence {
    pub fn select(&mut self, jid: Option<wr::JID>, now: i64) -> bool {
        let individual = jid.filter(is_individual);
        if let Some(jid) = individual.as_ref() {
            self.touch(jid);
            self.remove_expired_online(jid, now);
        }
        if self.selected != individual {
            self.selected = individual.clone();
            self.subscription_retry_at = None;
            self.subscription_failures = 0;
            self.terminal_rejection = false;
            return true;
        }
        false
    }

    pub fn subscription_due(&self, now: i64) -> Option<wr::JID> {
        if !self.ready
            || self.terminal_rejection
            || self.subscribed == self.selected
            || self
                .subscription_retry_at
                .is_some_and(|retry_at| retry_at > now)
        {
            return None;
        }
        self.selected.clone()
    }

    pub fn selected(&self) -> Option<&wr::JID> {
        self.selected.as_ref()
    }

    pub fn subscription_result(
        &mut self,
        jid: &wr::JID,
        result: wr::SubscribePresenceResult,
        now: i64,
    ) -> Option<i64> {
        if self.selected.as_ref() != Some(jid) {
            return None;
        }
        if result == wr::SubscribePresenceResult::Accepted {
            self.subscribed = Some(jid.clone());
            self.subscription_retry_at = None;
            self.subscription_failures = 0;
            return None;
        }

        if result == wr::SubscribePresenceResult::NoPrivacyToken {
            self.terminal_rejection = true;
            self.subscription_retry_at = None;
            return None;
        }

        let delay = 1_i64
            .checked_shl(self.subscription_failures.min(5))
            .unwrap_or(MAX_SUBSCRIPTION_RETRY_SECONDS)
            .min(MAX_SUBSCRIPTION_RETRY_SECONDS);
        self.subscription_failures = self.subscription_failures.saturating_add(1);
        self.subscription_retry_at = Some(now.saturating_add(delay));
        Some(delay)
    }

    pub fn ready(&mut self) {
        self.ready = true;
        self.subscribed = None;
        self.terminal_rejection = false;
        self.subscription_retry_at = None;
        self.subscription_failures = 0;
    }

    pub fn update(&mut self, from: &wr::JID, unavailable: bool, last_seen: i64, now: i64) -> bool {
        if !is_individual(from) {
            return false;
        }
        self.observations.insert(
            from.clone(),
            PresenceObservation {
                online: !unavailable,
                unavailable_since: unavailable.then_some(if last_seen > 0 {
                    last_seen
                } else {
                    now
                }),
                observed_at: now,
            },
        );
        self.touch(from);
        self.evict_oldest();
        self.selected.as_ref() == Some(from)
    }

    pub fn marker(&mut self, selected_chat: Option<&wr::JID>, now: i64) -> Option<PresenceMarker> {
        let selected_chat = selected_chat?;
        if !is_individual(selected_chat) || self.selected.as_ref() != Some(selected_chat) {
            return Some(PresenceMarker::Offline);
        }
        self.touch(selected_chat);
        self.remove_expired_online(selected_chat, now);
        let Some(observation) = self.observations.get(selected_chat) else {
            return Some(PresenceMarker::Offline);
        };
        if observation.online {
            return Some(PresenceMarker::Online);
        }
        Some(match observation.unavailable_since {
            Some(since) if now.saturating_sub(since) < RECENTLY_OFFLINE_SECONDS => {
                PresenceMarker::RecentlyOffline
            }
            _ => PresenceMarker::Offline,
        })
    }

    pub fn redraw_after(&self, now: i64) -> Option<Duration> {
        let expiry = self
            .selected
            .as_ref()
            .and_then(|jid| self.observations.get(jid))
            .and_then(|observation| {
                let expires_at = if observation.online {
                    observation.observed_at.saturating_add(ONLINE_TTL_SECONDS)
                } else {
                    observation
                        .unavailable_since?
                        .saturating_add(RECENTLY_OFFLINE_SECONDS)
                };
                let remaining = expires_at.saturating_sub(now);
                (remaining > 0).then_some(remaining)
            });
        let retry = self
            .subscription_retry_at
            .map(|retry_at| retry_at.saturating_sub(now).max(0));
        expiry
            .into_iter()
            .chain(retry)
            .min()
            .map(|seconds| Duration::from_secs(seconds as u64))
    }

    fn touch(&mut self, jid: &wr::JID) {
        if self.observations.contains_key(jid) {
            self.observation_recency.retain(|cached| cached != jid);
            self.observation_recency.push_back(jid.clone());
        }
    }

    fn remove_expired_online(&mut self, jid: &wr::JID, now: i64) {
        let expired = self.observations.get(jid).is_some_and(|observation| {
            observation.online && now.saturating_sub(observation.observed_at) >= ONLINE_TTL_SECONDS
        });
        if expired {
            self.observations.remove(jid);
            self.observation_recency.retain(|cached| cached != jid);
        }
    }

    fn evict_oldest(&mut self) {
        while self.observations.len() > MAX_CACHED_CONTACTS {
            if let Some(oldest) = self.observation_recency.pop_front() {
                self.observations.remove(&oldest);
            }
        }
    }
}

pub fn is_individual(jid: &wr::JID) -> bool {
    jid.0.ends_with("@s.whatsapp.net")
}

pub fn jid_for_log(jid: &wr::JID) -> String {
    jid.0.split_once('@').map_or_else(
        || "<redacted>".to_owned(),
        |(_, server)| format!("<redacted>@{server}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jid(value: &str) -> wr::JID {
        value.to_owned().into()
    }

    #[test]
    fn subscription_waits_for_ready_and_repeats_after_reconnect() {
        let alice = jid("alice@s.whatsapp.net");
        let mut state = SelectedPresence::default();

        state.select(Some(alice.clone()), 100);
        assert_eq!(state.subscription_due(100), None);
        state.ready();
        assert_eq!(state.subscription_due(100), Some(alice.clone()));
        state.subscription_result(&alice, wr::SubscribePresenceResult::Accepted, 100);
        assert_eq!(state.subscription_due(100), None);
        state.ready();
        assert_eq!(state.subscription_due(100), Some(alice));
    }

    #[test]
    fn failed_subscription_retries_with_backoff_without_hot_looping() {
        let alice = jid("alice@s.whatsapp.net");
        let mut state = SelectedPresence::default();
        state.select(Some(alice.clone()), 100);
        state.ready();

        assert_eq!(state.subscription_due(100), Some(alice.clone()));
        state.subscription_result(&alice, wr::SubscribePresenceResult::Rejected, 100);
        assert_eq!(state.subscription_due(100), None);
        assert_eq!(state.redraw_after(100), Some(Duration::from_secs(1)));
        assert_eq!(state.subscription_due(101), Some(alice.clone()));

        state.subscription_result(&alice, wr::SubscribePresenceResult::Rejected, 101);
        assert_eq!(state.subscription_due(102), None);
        assert_eq!(state.subscription_due(103), Some(alice.clone()));
        state.subscription_result(&alice, wr::SubscribePresenceResult::Accepted, 103);
        assert_eq!(state.subscription_due(103), None);
        assert_eq!(state.redraw_after(103), None);
    }

    #[test]
    fn missing_privacy_token_does_not_retry_until_next_ready_cycle() {
        let alice = jid("alice@s.whatsapp.net");
        let mut state = SelectedPresence::default();
        state.select(Some(alice.clone()), 100);
        state.ready();

        state.subscription_result(&alice, wr::SubscribePresenceResult::NoPrivacyToken, 100);
        assert_eq!(state.subscription_due(100), None);
        assert_eq!(state.subscription_due(1_000), None);
        assert_eq!(state.redraw_after(100), None);

        state.ready();
        assert_eq!(state.subscription_due(1_000), Some(alice));
    }

    #[test]
    fn updates_are_selected_contact_safe_and_groups_stay_unknown() {
        let alice = jid("alice@s.whatsapp.net");
        let bob = jid("bob@s.whatsapp.net");
        let group = jid("group@g.us");
        let mut state = SelectedPresence::default();
        state.select(Some(alice.clone()), 100);

        assert!(!state.update(&bob, false, 0, 100));
        assert_eq!(
            state.marker(Some(&alice), 100),
            Some(PresenceMarker::Offline)
        );
        state.select(Some(group.clone()), 100);
        assert_eq!(state.subscription_due(100), None);
        assert_eq!(
            state.marker(Some(&group), 100),
            Some(PresenceMarker::Offline)
        );
    }

    #[test]
    fn unavailable_marker_expires_at_the_exact_five_minute_boundary() {
        let alice = jid("alice@s.whatsapp.net");
        let mut state = SelectedPresence::default();
        state.select(Some(alice.clone()), 1_010);
        assert!(state.update(&alice, true, 1_000, 1_010));

        assert_eq!(
            state.marker(Some(&alice), 1_299),
            Some(PresenceMarker::RecentlyOffline)
        );
        assert_eq!(state.redraw_after(1_299), Some(Duration::from_secs(1)));
        assert_eq!(
            state.marker(Some(&alice), 1_300),
            Some(PresenceMarker::Offline)
        );
        assert_eq!(state.redraw_after(1_300), None);
    }

    #[test]
    fn canonical_pn_presence_event_marks_recent_disconnect_yellow() {
        let canonical_pn = jid("15551234567@s.whatsapp.net");
        let mut state = SelectedPresence::default();
        state.select(Some(canonical_pn.clone()), 1_010);

        assert!(state.update(&canonical_pn, true, 1_000, 1_010));
        assert_eq!(
            state.marker(Some(&canonical_pn), 1_299),
            Some(PresenceMarker::RecentlyOffline)
        );
    }

    #[test]
    fn hidden_last_seen_uses_the_observed_unavailable_transition_time() {
        let alice = jid("alice@s.whatsapp.net");
        let mut state = SelectedPresence::default();
        state.select(Some(alice.clone()), 2_000);
        state.update(&alice, true, 0, 2_000);

        assert_eq!(
            state.marker(Some(&alice), 2_000),
            Some(PresenceMarker::RecentlyOffline)
        );
    }

    #[test]
    fn online_survives_chat_switch_within_ttl() {
        let alice = jid("alice@s.whatsapp.net");
        let bob = jid("bob@s.whatsapp.net");
        let mut state = SelectedPresence::default();
        state.select(Some(alice.clone()), 1_000);
        assert!(state.update(&alice, false, 0, 1_000));

        state.select(Some(bob), 1_001);
        state.select(Some(alice.clone()), 1_002);

        assert_eq!(
            state.marker(Some(&alice), 1_002),
            Some(PresenceMarker::Online)
        );
        assert_eq!(state.redraw_after(1_002), Some(Duration::from_secs(298)));
    }

    #[test]
    fn unavailable_survives_chat_switch_and_expires_exactly() {
        let alice = jid("alice@s.whatsapp.net");
        let bob = jid("bob@s.whatsapp.net");
        let mut state = SelectedPresence::default();
        state.select(Some(alice.clone()), 1_000);
        assert!(state.update(&alice, true, 1_000, 1_000));

        state.select(Some(bob), 1_100);
        state.select(Some(alice.clone()), 1_299);
        assert_eq!(
            state.marker(Some(&alice), 1_299),
            Some(PresenceMarker::RecentlyOffline)
        );
        assert_eq!(state.redraw_after(1_299), Some(Duration::from_secs(1)));
        assert_eq!(
            state.marker(Some(&alice), 1_300),
            Some(PresenceMarker::Offline)
        );
        assert_eq!(state.redraw_after(1_300), None);
    }

    #[test]
    fn cached_online_expires_after_five_minutes_without_refresh() {
        let alice = jid("alice@s.whatsapp.net");
        let mut state = SelectedPresence::default();
        state.select(Some(alice.clone()), 1_000);
        state.update(&alice, false, 0, 1_000);

        assert_eq!(
            state.marker(Some(&alice), 1_299),
            Some(PresenceMarker::Online)
        );
        assert_eq!(
            state.marker(Some(&alice), 1_300),
            Some(PresenceMarker::Offline)
        );
        assert_eq!(state.redraw_after(1_300), None);
    }

    #[test]
    fn non_selected_individual_updates_are_cached_without_requesting_redraw() {
        let alice = jid("alice@s.whatsapp.net");
        let bob = jid("bob@s.whatsapp.net");
        let mut state = SelectedPresence::default();
        state.select(Some(alice), 100);

        assert!(!state.update(&bob, false, 0, 100));
        state.select(Some(bob.clone()), 101);
        assert_eq!(state.marker(Some(&bob), 101), Some(PresenceMarker::Online));
    }

    #[test]
    fn cache_is_bounded_and_evicts_least_recently_used_contact() {
        let mut state = SelectedPresence::default();
        let oldest = jid("user0@s.whatsapp.net");
        for index in 0..=MAX_CACHED_CONTACTS {
            let contact = jid(&format!("user{index}@s.whatsapp.net"));
            state.update(&contact, false, 0, index as i64);
        }

        assert_eq!(state.observations.len(), MAX_CACHED_CONTACTS);
        assert!(!state.observations.contains_key(&oldest));
        assert!(
            state
                .observations
                .contains_key(&jid("user256@s.whatsapp.net"))
        );
    }

    #[test]
    fn groups_and_unknown_servers_never_enter_the_cache() {
        let mut state = SelectedPresence::default();
        assert!(!state.update(&jid("group@g.us"), false, 0, 100));
        assert!(!state.update(&jid("user@example.test"), false, 0, 100));
        assert!(state.observations.is_empty());
    }

    #[test]
    fn subscription_tracks_only_selected_contact_and_is_deduplicated() {
        let alice = jid("alice@s.whatsapp.net");
        let bob = jid("bob@s.whatsapp.net");
        let mut state = SelectedPresence::default();
        state.ready();
        state.select(Some(alice.clone()), 100);
        assert_eq!(state.subscription_due(100), Some(alice.clone()));
        state.subscription_result(&alice, wr::SubscribePresenceResult::Accepted, 100);
        assert_eq!(state.subscription_due(100), None);

        state.select(Some(bob.clone()), 101);
        assert_eq!(state.subscription_due(101), Some(bob.clone()));
        state.subscription_result(&bob, wr::SubscribePresenceResult::Accepted, 101);
        assert_eq!(state.subscription_due(101), None);
        assert!(!state.update(&alice, false, 0, 102));
        assert_eq!(state.subscription_due(102), None);
    }

    #[test]
    fn diagnostic_jid_hides_the_user_portion() {
        assert_eq!(
            jid_for_log(&jid("15551234567@s.whatsapp.net")),
            "<redacted>@s.whatsapp.net"
        );
    }

    #[test]
    fn debug_config_only_accepts_one() {
        assert!(debug_enabled(Some("1")));
        assert!(!debug_enabled(None));
        assert!(!debug_enabled(Some("true")));
        assert!(!debug_enabled(Some("0")));
    }

    #[test]
    fn diagnostics_are_bounded_ordered_and_redacted() {
        let mut diagnostics = PresenceDiagnostics::new(true);
        for index in 0..=MAX_DIAGNOSTIC_ENTRIES {
            let redacted = jid_for_log(&jid(&format!("user{index}@s.whatsapp.net")));
            diagnostics.record(|| format!("selected jid={redacted} sequence={index}"));
        }

        let mut output = Vec::new();
        diagnostics.write_report(&mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(!output.contains("user"));
        assert!(!output.contains("sequence=0\n"));
        assert!(output.contains("1. selected jid=<redacted>@s.whatsapp.net sequence=1"));
        assert!(output.contains("50. selected jid=<redacted>@s.whatsapp.net sequence=50"));
    }

    #[test]
    fn disabled_diagnostics_are_a_no_op_and_print_nothing() {
        let mut diagnostics = PresenceDiagnostics::new(false);
        let mut evaluated = false;
        diagnostics.record(|| {
            evaluated = true;
            "not recorded".to_owned()
        });
        let mut output = Vec::new();
        diagnostics.write_report(&mut output).unwrap();

        assert!(!evaluated);
        assert!(output.is_empty());
    }

    #[test]
    fn report_format_is_concise_and_ordered() {
        let mut diagnostics = PresenceDiagnostics::new(true);
        diagnostics.record(|| "self presence available: ready".to_owned());
        diagnostics.record_presence(|| "presence event received".to_owned());
        let mut output = Vec::new();
        diagnostics.write_report(&mut output).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "Presence diagnostics:\nRust translated Presence updates: 1\n1. self presence available: ready\n2. presence event received\n"
        );
    }

    #[test]
    fn raw_report_is_separate_and_disabled_prints_nothing() {
        let enabled = PresenceDiagnostics::new(true);
        let mut output = Vec::new();
        enabled
            .write_raw_report(&mut output, Some("raw presence events received: 0\n"))
            .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "\nGo raw presence diagnostics:\nraw presence events received: 0\n"
        );

        let disabled = PresenceDiagnostics::new(false);
        let mut output = Vec::new();
        disabled
            .write_raw_report(&mut output, Some("raw presence events received: 1\n"))
            .unwrap();
        assert!(output.is_empty());
    }
}
