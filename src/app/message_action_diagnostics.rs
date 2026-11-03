use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const MAX_DIAGNOSTIC_ENTRIES: usize = 100;
const GO_DIAGNOSTIC_PREFIX: &str = "MESSAGE_ACTION_DIAG ";

#[derive(Clone, Debug)]
pub struct MessageActionDiagnostics {
    enabled: bool,
    state: Arc<Mutex<DiagnosticState>>,
}

#[derive(Debug, Default)]
struct DiagnosticState {
    action_entries: VecDeque<String>,
    census_entries: VecDeque<String>,
    read_sync_entries: VecDeque<String>,
    reported: bool,
    trace: Option<File>,
    trace_count: usize,
}

#[derive(Clone, Copy)]
pub(crate) enum ReadSyncStage {
    Menu,
    ToggleBefore,
    ToggleAfter,
}

impl MessageActionDiagnostics {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            state: Arc::new(Mutex::new(DiagnosticState::default())),
        }
    }

    /// A separate, strict report; never exports the broader diagnostic buffers.
    /// Only a new report may be created; existing files are never overwritten.
    /// Failure silently disables capture.
    pub fn with_read_sync_trace(self, path: Option<PathBuf>) -> Self {
        if let Ok(mut state) = self.state.lock() {
            state.trace = path.and_then(|path| {
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .ok()
            });
            state.trace_count = 0;
        }
        self
    }

    pub(crate) fn trace_snapshot(
        &self,
        stage: ReadSyncStage,
        unread: bool,
        manual: bool,
        pending: usize,
    ) {
        let stage = match stage {
            ReadSyncStage::Menu => "menu",
            ReadSyncStage::ToggleBefore => "toggle_before",
            ReadSyncStage::ToggleAfter => "toggle_after",
        };
        self.trace_line(|| {
            format!("stage={stage} unread={unread} manual={manual} pending={pending}\n")
        });
    }

    pub(crate) fn trace_queue(&self, accepted: bool, read: bool) {
        self.trace_line(|| format!("stage=queue accepted={accepted} read={read}\n"));
    }

    fn trace_line(&self, line: impl FnOnce() -> String) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.trace_count >= MAX_DIAGNOSTIC_ENTRIES {
            return;
        }
        if let Some(file) = state.trace.as_mut() {
            // One shared lock covers UI and bridge callback writes, including the cap.
            // Never hold this lock while invoking a bridge/network operation.
            if file.write_all(line().as_bytes()).is_err() {
                state.trace = None;
            }
            state.trace_count += 1;
        }
    }

    pub fn record(&self, entry: impl FnOnce() -> String) {
        if !self.enabled {
            return;
        }
        let mut state = self.state.lock().unwrap();
        push_bounded(&mut state.action_entries, entry());
    }

    pub fn record_go_log(&self, message: &str) {
        if let Some(entry) = message.strip_prefix("READ_SYNC_TRACE ") {
            // Reconstruct from an exact allowlist, not arbitrary Go log fields.
            if let Some(category) = entry.strip_prefix("stage=bridge status=2 category=") {
                if matches!(
                    category,
                    "cancelled"
                        | "timeout"
                        | "other"
                        | "missing_key"
                        | "key_lookup"
                        | "resync_after_send"
                        | "appstate_rejected"
                        | "appstate_with_cause"
                        | "iq_bad_request"
                        | "iq_not_authorized"
                        | "iq_forbidden"
                        | "iq_not_found"
                        | "iq_not_allowed"
                        | "iq_not_acceptable"
                        | "iq_gone"
                        | "iq_resource_limit"
                        | "iq_locked"
                        | "iq_rate_over_limit"
                        | "iq_internal_server_error"
                        | "iq_service_unavailable"
                        | "iq_partial_server_error"
                        | "iq_other"
                        | "missing_element"
                        | "disconnected"
                        | "not_logged_in"
                        | "iq_timeout"
                        | "client_nil"
                ) {
                    self.trace_line(|| format!("stage=bridge status=2 category={category}\n"));
                }
            } else if matches!(
                entry,
                "stage=bridge status=0 category=none"
                    | "stage=bridge status=1 category=unavailable"
            ) {
                self.trace_line(|| format!("{entry}\n"));
            }
            return;
        }
        if !self.enabled {
            return;
        }
        let Some(entry) = go_diagnostic_entry(message) else {
            return;
        };
        let mut state = self.state.lock().unwrap();
        match entry {
            GoDiagnosticEntry::Action(entry) => push_bounded(&mut state.action_entries, entry),
            GoDiagnosticEntry::Census(entry) => push_bounded(&mut state.census_entries, entry),
        }
    }

    pub fn record_read_sync(&self, entry: impl FnOnce() -> String) {
        if !self.enabled {
            return;
        }
        let mut state = self.state.lock().unwrap();
        push_bounded(&mut state.read_sync_entries, entry());
    }

    pub(crate) fn record_unread_group_reply(
        &self,
        chat: &str,
        message: &str,
        quote: Option<&str>,
        lookup_hit: bool,
        target_chat_equal: bool,
        target_is_from_me: bool,
        attention_applied: bool,
    ) {
        self.record(|| {
            format!(
                "source=rust classifier=unread_group_reply chat={} message_id={} quote_id={} quote_present={} lookup={} target_chat_equal={} target_is_from_me={} attention_applied={}",
                identifier_for_log(chat),
                identifier_for_log(message),
                quote.map_or_else(|| "<missing>".to_owned(), identifier_for_log),
                quote.is_some(),
                if lookup_hit { "hit" } else { "miss" },
                target_chat_equal,
                target_is_from_me,
                attention_applied,
            )
        });
    }

    pub fn write_report(&self, mut output: impl Write) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let mut state = self.state.lock().unwrap();
        if state.reported {
            return Ok(());
        }
        state.reported = true;
        writeln!(output, "Message action diagnostics:")?;
        if state.action_entries.is_empty() {
            writeln!(output, "No message-action events captured.")?;
        } else {
            for (index, entry) in state.action_entries.iter().enumerate() {
                writeln!(output, "{}. {entry}", index + 1)?;
            }
        }
        writeln!(output, "Event census:")?;
        if state.census_entries.is_empty() {
            writeln!(output, "No whatsmeow events captured.")?;
        } else {
            for (index, entry) in state.census_entries.iter().enumerate() {
                writeln!(output, "{}. {entry}", index + 1)?;
            }
        }
        writeln!(output, "Chat read/status diagnostics:")?;
        if state.read_sync_entries.is_empty() {
            writeln!(output, "No chat read/status events captured.")?;
        } else {
            for (index, entry) in state.read_sync_entries.iter().enumerate() {
                writeln!(output, "{}. {entry}", index + 1)?;
            }
        }
        Ok(())
    }
}

pub fn debug_enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

pub fn identifier_for_log(identifier: &str) -> String {
    let hash = identifier
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
        });
    format!("<id:{hash:08x}>")
}

fn push_bounded(entries: &mut VecDeque<String>, entry: String) {
    if entries.len() == MAX_DIAGNOSTIC_ENTRIES {
        entries.pop_front();
    }
    entries.push_back(entry);
}

enum GoDiagnosticEntry {
    Action(String),
    Census(String),
}

fn go_diagnostic_entry(message: &str) -> Option<GoDiagnosticEntry> {
    let fields = message.strip_prefix(GO_DIAGNOSTIC_PREFIX)?;
    if let Some(census) = census_entry(fields) {
        return Some(GoDiagnosticEntry::Census(census));
    }
    if let Some(status_protocol) = status_protocol_entry(fields) {
        return Some(GoDiagnosticEntry::Action(status_protocol));
    }
    let mut classifier = None;
    let mut result = None;
    let mut details = Vec::new();

    for field in fields.split_ascii_whitespace() {
        let (key, value) = field.split_once('=')?;
        match key {
            "classifier" => classifier = safe_value(value),
            "result" => result = safe_value(value),
            "kind" | "reason" | "branch" => {
                details.push(format!("{key}={}", safe_value(value)?));
            }
            "action_id" | "target_id" | "chat" | "sender" => {
                let identifier = if is_redacted_identifier(value) {
                    value.to_owned()
                } else {
                    identifier_for_log(value)
                };
                details.push(format!("{key}={identifier}"));
            }
            _ => {}
        }
    }

    let classifier = classifier?;
    let mut entry = format!("source=go classifier={classifier}");
    if let Some(result) = result {
        entry.push_str(&format!(" result={result}"));
    }
    for identifier in details {
        entry.push(' ');
        entry.push_str(&identifier);
    }
    Some(GoDiagnosticEntry::Action(entry))
}

fn status_protocol_entry(fields: &str) -> Option<String> {
    let mut status_protocol = None;
    let mut details = Vec::new();

    for field in fields.split_ascii_whitespace() {
        let (key, value) = field.split_once('=')?;
        match key {
            "status_protocol" if matches!(value, "reaction" | "context") => {
                status_protocol = Some(value);
            }
            "chat" | "sender" | "remote_jid" | "participant" | "id" | "stanza_id"
            | "poster_status_id" => {
                let identifier = if is_redacted_identifier(value) {
                    value.to_owned()
                } else {
                    identifier_for_log(value)
                };
                details.push(format!("{key}={identifier}"));
            }
            "from_me"
            | "key_from_me"
            | "quoted_message_present"
            | "status_source_type_present"
            | "status_attribution_type_present"
            | "is_group_status_present"
            | "is_group_status"
                if matches!(value, "true" | "false") =>
            {
                details.push(format!("{key}={value}"));
            }
            "status_source_type" | "status_attribution_type" if value.parse::<i32>().is_ok() => {
                details.push(format!("{key}={value}"));
            }
            "content" | "quoted_message_kind" if safe_value(value).is_some() => {
                details.push(format!("{key}={value}"));
            }
            "emoji_codepoints" if safe_emoji_codepoints(value) => {
                details.push(format!("{key}={value}"));
            }
            "emoji" if safe_emoji(value) => details.push(format!("{key}={value}")),
            _ => return None,
        }
    }

    let status_protocol = status_protocol?;
    (!details.is_empty()).then(|| {
        format!(
            "source=go status_protocol={status_protocol} {}",
            details.join(" ")
        )
    })
}

fn safe_emoji_codepoints(value: &str) -> bool {
    value == "none"
        || (!value.is_empty()
            && value.split(',').all(|codepoint| {
                codepoint.strip_prefix("U+").is_some_and(|hex| {
                    !hex.is_empty() && hex.chars().all(|character| character.is_ascii_hexdigit())
                })
            }))
}

fn safe_emoji(value: &str) -> bool {
    value == "<empty>"
        || (!value.is_empty()
            && value.chars().count() <= 32
            && value
                .chars()
                .all(|character| !character.is_control() && !character.is_whitespace()))
}

fn census_entry(fields: &str) -> Option<String> {
    let mut details = Vec::new();
    for field in fields.split_ascii_whitespace() {
        let (key, value) = field.split_once('=')?;
        match key {
            "census" if value == "event" => {}
            "seq"
            | "count"
            | "secret_enc_type_number"
            | "secret_payload_length"
            | "secret_iv_length"
                if value.parse::<u64>().is_ok() =>
            {
                details.push(format!("{key}={value}"));
            }
            "is_edit"
            | "is_history"
            | "protocol_present"
            | "protocol_key_id"
            | "secret_target_present"
            | "target_key_present"
                if matches!(value, "true" | "false") =>
            {
                details.push(format!("{key}={value}"));
            }
            "event_type"
            | "subtype"
            | "app_state"
            | "protocol_type"
            | "secret_enc_type"
            | "decrypt_result"
            | "decrypted_content_kind"
            | "secret_edit_result"
            | "error_class"
                if safe_value(value).is_some() =>
            {
                details.push(format!("{key}={value}"));
            }
            "roots" | "raw_kinds" | "message_kinds" | "source_kinds" | "wrappers"
                if safe_census_list(value) =>
            {
                details.push(format!("{key}={value}"));
            }
            "info_id" | "chat" | "sender" | "source_key" | "action_id" | "target_id"
            | "secret_target_id" => {
                let identifier = if is_redacted_identifier(value) || value == "<missing>" {
                    value.to_owned()
                } else {
                    identifier_for_log(value)
                };
                details.push(format!("{key}={identifier}"));
            }
            _ => return None,
        }
    }
    (!details.is_empty()).then(|| format!("source=go {}", details.join(" ")))
}

fn safe_census_list(value: &str) -> bool {
    value.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '_' | ':' | ',')
    })
}

fn is_redacted_identifier(value: &str) -> bool {
    value
        .strip_prefix("<id:")
        .and_then(|value| value.strip_suffix('>'))
        .is_some_and(|value| {
            !value.is_empty() && value.chars().all(|character| character.is_ascii_hexdigit())
        })
}

fn safe_value(value: &str) -> Option<&str> {
    value
        .chars()
        .all(|character| character.is_ascii_lowercase() || character == '_' || character == '-')
        .then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_sync_trace_callback_allowlist_and_concurrent_cap_share_one_sink() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trace.txt");
        let diagnostics =
            MessageActionDiagnostics::new(true).with_read_sync_trace(Some(path.clone()));
        for malicious in [
            "READ_SYNC_TRACE stage=bridge status=2 category=other secret=private",
            "READ_SYNC_TRACE stage=bridge status=0 category=missing_key",
            "READ_SYNC_TRACE stage=bridge status=2 category=private-message-id",
            "READ_SYNC_TRACE stage=bridge status=2 category=other\nstolen=true",
            "MESSAGE_ACTION_DIAG classifier=raw chat=private@s.whatsapp.net",
        ] {
            diagnostics.record_go_log(malicious);
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let diagnostics = &diagnostics;
                scope.spawn(move || {
                    for _ in 0..60 {
                        diagnostics.trace_snapshot(ReadSyncStage::Menu, true, false, 1);
                        diagnostics
                            .record_go_log("READ_SYNC_TRACE stage=bridge status=2 category=other");
                    }
                });
            }
        });
        let report = std::fs::read_to_string(&path).unwrap();
        assert_eq!(report.lines().count(), 100);
        assert!(report.lines().all(|line| matches!(
            line,
            "stage=menu unread=true manual=false pending=1"
                | "stage=bridge status=2 category=other"
        )));
        assert!(report.len() < 16 * 1024);
    }

    #[test]
    fn read_sync_trace_accepts_closed_bridge_categories_exactly() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trace.txt");
        let diagnostics =
            MessageActionDiagnostics::new(false).with_read_sync_trace(Some(path.clone()));
        let mut expected = String::new();
        for category in [
            "cancelled",
            "timeout",
            "other",
            "missing_key",
            "key_lookup",
            "resync_after_send",
            "appstate_rejected",
            "appstate_with_cause",
            "iq_bad_request",
            "iq_not_authorized",
            "iq_forbidden",
            "iq_not_found",
            "iq_not_allowed",
            "iq_not_acceptable",
            "iq_gone",
            "iq_resource_limit",
            "iq_locked",
            "iq_rate_over_limit",
            "iq_internal_server_error",
            "iq_service_unavailable",
            "iq_partial_server_error",
            "iq_other",
            "missing_element",
            "disconnected",
            "not_logged_in",
            "iq_timeout",
            "client_nil",
        ] {
            let record = format!("stage=bridge status=2 category={category}");
            diagnostics.record_go_log(&format!("READ_SYNC_TRACE {record}"));
            expected.push_str(&format!("{record}\n"));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        }
        for record in [
            "stage=bridge status=0 category=none",
            "stage=bridge status=1 category=unavailable",
        ] {
            diagnostics.record_go_log(&format!("READ_SYNC_TRACE {record}"));
            expected.push_str(&format!("{record}\n"));
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    }

    #[test]
    fn read_sync_trace_rejects_malformed_typed_categories_without_consuming_cap() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trace.txt");
        let diagnostics =
            MessageActionDiagnostics::new(false).with_read_sync_trace(Some(path.clone()));
        for _ in 0..MAX_DIAGNOSTIC_ENTRIES {
            for invalid in [
                "stage=bridge status=2 category=iq_forbidden private=secret",
                "stage=bridge status=2 category=iq_forbidden\nprivate=secret",
                "stage=bridge status=2 category=iq_forbidden\n",
                "stage=bridge status=2 category=iq_forbidden ",
                "stage=bridge status=2 category=IQ_FORBIDDEN",
                "stage=bridge status=2 category=iq_987654",
                "stage=bridge status=2 category=private-chat@s.whatsapp.net",
                "stage=bridge status=2 category=missing_element tag=private-message-id",
                "stage=bridge status=2 category=disconnected node=private-payload",
                "stage=bridge status=0 category=iq_forbidden",
                "stage=bridge status=1 category=disconnected",
            ] {
                diagnostics.record_go_log(&format!("READ_SYNC_TRACE {invalid}"));
            }
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
        for _ in 0..MAX_DIAGNOSTIC_ENTRIES {
            diagnostics.record_go_log("READ_SYNC_TRACE stage=bridge status=2 category=other");
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "stage=bridge status=2 category=other\n".repeat(MAX_DIAGNOSTIC_ENTRIES)
        );
    }

    #[test]
    fn disabled_diagnostics_do_no_work_or_print() {
        let diagnostics = MessageActionDiagnostics::new(false);
        let mut evaluated = false;
        diagnostics.record(|| {
            evaluated = true;
            "not recorded".to_owned()
        });
        diagnostics.record_go_log("MESSAGE_ACTION_DIAG classifier=raw result=classified");
        let mut output = Vec::new();
        diagnostics.write_report(&mut output).unwrap();

        assert!(!evaluated);
        assert!(output.is_empty());
    }

    #[test]
    fn diagnostics_are_bounded_ordered_and_redacted() {
        let diagnostics = MessageActionDiagnostics::new(true);
        for index in 0..=MAX_DIAGNOSTIC_ENTRIES {
            diagnostics.record(|| {
                format!(
                    "source=rust sequence={index} target={}",
                    identifier_for_log(&format!("target-{index}"))
                )
            });
        }
        let mut output = Vec::new();
        diagnostics.write_report(&mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(!output.contains("target-"));
        assert!(!output.contains("sequence=0"));
        assert!(output.contains("1. source=rust sequence=1 target=<id:"));
        assert!(output.contains("100. source=rust sequence=100 target=<id:"));
        assert_eq!(output.lines().count(), MAX_DIAGNOSTIC_ENTRIES + 5);
    }

    #[test]
    fn go_log_filtering_keeps_only_prefixed_message_action_entries() {
        let diagnostics = MessageActionDiagnostics::new(true);
        diagnostics.record_go_log("unrelated Go log");
        diagnostics.record_go_log("MESSAGE_ACTION_DIAG classifier=raw result=classified kind=edit action_id=action-1 target_id=target-1 chat=15551234567@s.whatsapp.net sender=15557654321@s.whatsapp.net");
        let mut output = Vec::new();
        diagnostics.write_report(&mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(!output.contains("unrelated"));
        assert!(!output.contains("1555"));
        assert!(!output.contains("action-1"));
        assert!(output.contains("source=go classifier=raw result=classified kind=edit"));
        assert!(output.contains("action_id=<id:"));
    }

    #[test]
    fn census_keeps_structural_fields_and_redacts_untrusted_identifiers() {
        let diagnostics = MessageActionDiagnostics::new(true);
        diagnostics.record_go_log("MESSAGE_ACTION_DIAG census=event seq=7 event_type=events_message is_edit=true is_history=false info_id=message-secret chat=15551234567@s.whatsapp.net sender=15557654321@s.whatsapp.net roots=raw:true,message:true,source_web_msg:false raw_kinds=conversation message_kinds=conversation source_kinds=none wrappers=raw protocol_present=false protocol_type=none protocol_key_id=false source_key=<missing> secret_enc_type=message_edit secret_enc_type_number=2 secret_target_present=true secret_target_id=target-secret secret_payload_length=21 secret_iv_length=13 decrypt_result=success decrypted_content_kind=conversation");
        let mut output = Vec::new();
        diagnostics.write_report(&mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("Event census:\n1. source=go seq=7 event_type=events_message"));
        for secret in ["message-secret", "15551234567", "15557654321"] {
            assert!(!output.contains(secret));
        }
        assert!(output.contains("info_id=<id:"));
        assert!(output.contains("raw_kinds=conversation"));
        assert!(output.contains("secret_enc_type=message_edit"));
        assert!(output.contains("secret_target_id=<id:"));
        assert!(output.contains("decrypt_result=success"));
    }

    #[test]
    fn status_protocol_go_logs_are_retained_and_redact_identifiers() {
        let diagnostics = MessageActionDiagnostics::new(true);
        diagnostics.record_go_log("MESSAGE_ACTION_DIAG status_protocol=reaction chat=mobile@lid sender=mobile@lid from_me=true remote_jid=status@broadcast participant=author@s.whatsapp.net key_from_me=true id=status-id emoji_codepoints=U+2764,U+FE0F emoji=❤️");
        diagnostics.record_go_log("MESSAGE_ACTION_DIAG status_protocol=context chat=mobile@lid sender=mobile@lid from_me=true content=extended_text stanza_id=status-id participant=author@s.whatsapp.net remote_jid=status@broadcast poster_status_id=poster-status-id quoted_message_present=true quoted_message_kind=image status_source_type_present=true status_source_type=1 status_attribution_type_present=false status_attribution_type=0 is_group_status_present=true is_group_status=false");
        let mut output = Vec::new();
        diagnostics.write_report(&mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("source=go status_protocol=reaction"));
        assert!(output.contains("source=go status_protocol=context"));
        assert!(output.contains("content=extended_text"));
        assert!(output.contains("quoted_message_present=true quoted_message_kind=image"));
        assert!(output.contains("emoji_codepoints=U+2764,U+FE0F emoji=❤️"));
        assert!(output.contains("chat=<id:") && output.contains("stanza_id=<id:"));
        for private in [
            "mobile@lid",
            "author@s.whatsapp.net",
            "status@broadcast",
            "status-id",
            "poster-status-id",
        ] {
            assert!(!output.contains(private));
        }
    }

    #[test]
    fn report_is_numbered_and_reports_empty_capture() {
        let diagnostics = MessageActionDiagnostics::new(true);
        let mut output = Vec::new();
        diagnostics.write_report(&mut output).unwrap();
        diagnostics.write_report(&mut output).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "Message action diagnostics:\nNo message-action events captured.\nEvent census:\nNo whatsmeow events captured.\nChat read/status diagnostics:\nNo chat read/status events captured.\n"
        );
    }
}
