use std::io::Write;

use log::info;
use whatsrust as wr;

use super::App;
use crate::app::presence::jid_for_log;

#[cfg(test)]
mod tests;

impl App<'_> {
    pub(crate) fn mark_presence_ready(&mut self) {
        self.selected_presence.ready();
        self.presence_diagnostics
            .record(|| "self presence available: ready".to_owned());
    }

    pub(crate) fn handle_presence_update(&mut self, update: wr::PresenceUpdate) -> bool {
        let wr::PresenceUpdate {
            from,
            unavailable,
            last_seen,
        } = update;
        self.selected_presence
            .update(&from, unavailable, last_seen, self.now())
    }

    pub(crate) fn write_presence_diagnostics(&self, output: &mut impl Write) {
        let _ = self.presence_diagnostics.write_report(&mut *output);
        let raw_report = wr::drain_raw_presence_diagnostics();
        let _ = self
            .presence_diagnostics
            .write_raw_report(&mut *output, raw_report.as_deref());
    }

    pub(crate) fn sync_selected_presence(&mut self) {
        let now = self.now();
        if self.selected_presence.select(self.open_chat.clone(), now) {
            let selected = self
                .selected_presence
                .selected()
                .map(jid_for_log)
                .unwrap_or_else(|| "none".to_owned());
            self.presence_diagnostics
                .record(|| format!("selected canonical jid={selected}"));
        }
        let Some(jid) = self.selected_presence.subscription_due(now) else {
            return;
        };

        let diagnostic_jid = jid_for_log(&jid);
        self.presence_diagnostics
            .record(|| format!("presence subscription attempt: jid={diagnostic_jid}"));
        info!("Presence subscription attempt: jid={}", jid_for_log(&jid));
        let result = wr::subscribe_presence(&jid);
        let retry_delay = self
            .selected_presence
            .subscription_result(&jid, result, now);
        let diagnostic_jid = jid_for_log(&jid);
        self.presence_diagnostics.record(|| {
            if result == wr::SubscribePresenceResult::NoPrivacyToken {
                format!(
                    "presence subscription result: jid={diagnostic_jid}, result=rejected: no privacy token"
                )
            } else if let Some(delay) = retry_delay {
                format!(
                    "presence subscription result: jid={diagnostic_jid}, result=rejected, retry_in={delay}s"
                )
            } else {
                format!("presence subscription result: jid={diagnostic_jid}, result=accepted")
            }
        });
        info!(
            "Presence subscription result: jid={}, result={}",
            jid_for_log(&jid),
            match result {
                wr::SubscribePresenceResult::Accepted => "accepted",
                wr::SubscribePresenceResult::NoPrivacyToken => "rejected: no privacy token",
                wr::SubscribePresenceResult::Rejected => "rejected",
            }
        );
    }
}
