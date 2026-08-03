use std::collections::VecDeque;
use std::io::{self, Write};
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
    reported: bool,
}

impl MessageActionDiagnostics {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            state: Arc::new(Mutex::new(DiagnosticState::default())),
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
        assert_eq!(output.lines().count(), MAX_DIAGNOSTIC_ENTRIES + 3);
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
            "Message action diagnostics:\nNo message-action events captured.\nEvent census:\nNo whatsmeow events captured.\n"
        );
    }
}
