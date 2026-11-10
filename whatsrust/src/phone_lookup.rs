use std::ffi::{CStr, CString};

use crate::abi::{C_FreeLookupPhoneResult, C_LookupPhone};
use crate::models::JID;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneLookupError {
    InvalidNumber,
    ClientUnavailable,
    RequestFailed,
    InvalidBridgeResult,
    TimedOut,
    EmptyResponse,
    ResponseMismatch,
    QueryMismatch,
    LidOnly,
    DifferentPhoneNumber,
    // Temporary independent comparisons: digits, allowed domain, integrator.
    ResponseComparison { phone: [bool; 3], jid: [bool; 3] },
}

pub fn lookup_phone(phone: &str) -> Result<Option<JID>, PhoneLookupError> {
    let phone = CString::new(phone).map_err(|_| PhoneLookupError::InvalidNumber)?;
    let result = unsafe { C_LookupPhone(phone.as_ptr()) };
    let jid = if result.jid.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(result.jid) }
            .to_str()
            .ok()
            .map(str::to_owned)
    };
    let status = result.status;
    unsafe { C_FreeLookupPhoneResult(result) };
    lookup_result(status, phone.to_str().unwrap_or_default(), jid.as_deref())
}

fn lookup_result(
    status: u8,
    phone: &str,
    jid: Option<&str>,
) -> Result<Option<JID>, PhoneLookupError> {
    match status {
        0 => {
            let Some(digits) = phone.strip_prefix('+') else {
                return Err(PhoneLookupError::InvalidBridgeResult);
            };
            let expected = format!("{digits}@s.whatsapp.net");
            if jid != Some(expected.as_str()) {
                return Err(PhoneLookupError::InvalidBridgeResult);
            }
            Ok(Some(JID::from(expected)))
        }
        1 if jid.is_none() => Ok(None),
        2 if jid.is_none() => Err(PhoneLookupError::InvalidNumber),
        3 if jid.is_none() => Err(PhoneLookupError::ClientUnavailable),
        4 if jid.is_none() => Err(PhoneLookupError::RequestFailed),
        5 if jid.is_none() => Err(PhoneLookupError::TimedOut),
        6 if jid.is_none() => Err(PhoneLookupError::EmptyResponse),
        7 if jid.is_none() => Err(PhoneLookupError::ResponseMismatch),
        8 if jid.is_none() => Err(PhoneLookupError::QueryMismatch),
        9 if jid.is_none() => Err(PhoneLookupError::LidOnly),
        10 if jid.is_none() => Err(PhoneLookupError::DifferentPhoneNumber),
        128..=191 if jid.is_none() => Err(PhoneLookupError::ResponseComparison {
            phone: [status & 1 != 0, status & 2 != 0, status & 4 != 0],
            jid: [status & 8 != 0, status & 16 != 0, status & 32 != 0],
        }),
        _ => Err(PhoneLookupError::InvalidBridgeResult),
    }
}

// Keep local observation data separate from the recipient validation result.
fn lookup_observed_result(
    status: u8,
    phone: &str,
    jid: Option<&str>,
    _observation: Option<&str>,
) -> Result<Option<JID>, PhoneLookupError> {
    lookup_result(status, phone, jid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsolicited_or_invalid_bridge_results() {
        assert_eq!(
            lookup_result(0, "+15551234567", Some("9988@lid")),
            Err(PhoneLookupError::InvalidBridgeResult)
        );
        assert_eq!(
            lookup_result(0, "+15551234567", None),
            Err(PhoneLookupError::InvalidBridgeResult)
        );
        assert_eq!(
            lookup_result(1, "+15551234567", Some("15551234567@s.whatsapp.net")),
            Err(PhoneLookupError::InvalidBridgeResult)
        );
        assert_eq!(
            lookup_result(255, "+15551234567", None),
            Err(PhoneLookupError::InvalidBridgeResult)
        );
    }

    #[test]
    fn preserves_existing_failure_statuses() {
        for (status, expected) in [
            (2, PhoneLookupError::InvalidNumber),
            (3, PhoneLookupError::ClientUnavailable),
            (4, PhoneLookupError::RequestFailed),
        ] {
            assert_eq!(lookup_result(status, "+15551234567", None), Err(expected));
        }
    }

    #[test]
    fn exposes_safe_diagnostic_categories_without_bridge_payloads() {
        for (status, expected) in [
            (5, PhoneLookupError::TimedOut),
            (6, PhoneLookupError::EmptyResponse),
            (7, PhoneLookupError::ResponseMismatch),
        ] {
            assert_eq!(
                lookup_result(status, "+15551234567", None),
                Err(expected),
                "status {status}"
            );
        }
    }

    #[test]
    fn distinguishes_query_mismatch_without_bridge_payloads() {
        assert_eq!(
            lookup_result(8, "+15551234567", None),
            Err(PhoneLookupError::QueryMismatch)
        );
    }

    #[test]
    fn distinguishes_recipient_response_shapes_without_bridge_payloads() {
        for (status, expected) in [
            (9, PhoneLookupError::LidOnly),
            (10, PhoneLookupError::DifferentPhoneNumber),
        ] {
            assert_eq!(
                lookup_result(status, "+15551234567", None),
                Err(expected),
                "status {status}"
            );
        }
    }

    #[test]
    fn decodes_only_six_comparison_bits_without_a_recipient() {
        for bits in 0..64_u8 {
            let phone = [bits & 1 != 0, bits & 2 != 0, bits & 4 != 0];
            let jid = [bits & 8 != 0, bits & 16 != 0, bits & 32 != 0];
            assert_eq!(
                lookup_result(128 + bits, "+15551234567", None),
                Err(PhoneLookupError::ResponseComparison { phone, jid }),
                "comparison bits {bits}"
            );
            let recipient = Some("15551234567@s.whatsapp.net");
            assert_eq!(
                lookup_result(128 + bits, "+15551234567", recipient),
                Err(PhoneLookupError::InvalidBridgeResult)
            );
        }
        for status in [127, 192, 255] {
            assert_eq!(
                lookup_result(status, "+15551234567", None),
                Err(PhoneLookupError::InvalidBridgeResult)
            );
        }
    }

    #[test]
    fn diagnostic_statuses_cannot_carry_a_recipient() {
        for status in [5, 6, 7, 8, 9, 10] {
            assert_eq!(
                lookup_result(status, "+15551234567", Some("15551234567@s.whatsapp.net")),
                Err(PhoneLookupError::InvalidBridgeResult)
            );
        }
    }

    #[test]
    fn local_observation_is_separate_and_debug_redacted() {
        let text = "Requested: \"+15551234567\"\nPhoneNumber.User: \"15559876543\"";
        let result = lookup_observed_result(166, "+15551234567", None, Some(text));
        let debug = format!("{result:?}");
        assert!(debug.contains("ResponseObservation"));
        assert!(!debug.contains("15551234567"));
        assert!(!debug.contains("15559876543"));
    }

    #[test]
    fn local_observation_rejects_recipient_success_controls_and_oversize() {
        let recipient = Some("15551234567@s.whatsapp.net");
        let huge = "x".repeat(8193);
        for (status, jid, text) in [
            (0, recipient, "local text"),
            (166, recipient, "local text"),
            (10, None, "local text"),
            (255, None, "local text"),
            (166, None, "bad\u{1b}[2J"),
            (166, None, "bad\rtext"),
            (166, None, ""),
            (166, None, huge.as_str()),
        ] {
            assert_eq!(
                lookup_observed_result(status, "+15551234567", jid, Some(text)),
                Err(PhoneLookupError::InvalidBridgeResult)
            );
        }
    }

    #[test]
    fn distinguishes_unregistered_from_lookup_failure() {
        assert_eq!(lookup_result(1, "+15551234567", None), Ok(None));
        assert_eq!(
            lookup_result(4, "+15551234567", None),
            Err(PhoneLookupError::RequestFailed)
        );
        assert_eq!(
            lookup_result(0, "+15551234567", Some("15551234567@s.whatsapp.net")),
            Ok(Some(JID::from("15551234567@s.whatsapp.net".to_owned())))
        );
    }
}
