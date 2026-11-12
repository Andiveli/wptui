use std::ffi::{CStr, CString};

use crate::abi::{C_FreeLookupPhoneResult, C_LookupPhone};
use crate::models::JID;
use crate::phone_lookup_identity::is_confirmed_phone_lookup_identity;
use crate::phone_lookup_observation::LocalPhoneLookupObservation;

#[derive(Clone, Debug, Eq, PartialEq)]
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
    ResponseObservation(LocalPhoneLookupObservation),
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
    let observation = if result.observation.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(result.observation) }
            .to_str()
            .ok()
            .map(str::to_owned)
    };
    let invalid_text = (!result.jid.is_null() && jid.is_none())
        || (!result.observation.is_null() && observation.is_none());
    let status = result.status;
    unsafe { C_FreeLookupPhoneResult(result) };
    if invalid_text {
        return Err(PhoneLookupError::InvalidBridgeResult);
    }
    lookup_observed_result(
        status,
        phone.to_str().unwrap_or_default(),
        jid.as_deref(),
        observation.as_deref(),
    )
}

fn lookup_result(
    status: u8,
    phone: &str,
    jid: Option<&str>,
) -> Result<Option<JID>, PhoneLookupError> {
    match status {
        0 => {
            let Some(confirmed) = jid else {
                return Err(PhoneLookupError::InvalidBridgeResult);
            };
            if !is_confirmed_phone_lookup_identity(phone, confirmed) {
                return Err(PhoneLookupError::InvalidBridgeResult);
            }
            Ok(Some(JID::from(confirmed.to_owned())))
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
    observation: Option<&str>,
) -> Result<Option<JID>, PhoneLookupError> {
    let result = lookup_result(status, phone, jid);
    let Some(text) = observation else {
        return result;
    };
    if !matches!(&result, Err(PhoneLookupError::ResponseComparison { .. })) {
        return Err(PhoneLookupError::InvalidBridgeResult);
    }
    let observation = LocalPhoneLookupObservation::from_display_text(text.to_owned())
        .ok_or(PhoneLookupError::InvalidBridgeResult)?;
    Err(PhoneLookupError::ResponseObservation(observation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_documented_equivalent_confirmed_pns() {
        for (phone, confirmed) in [
            ("+525512345678", "5215512345678"),
            ("+5215512345678", "525512345678"),
            ("+541112345678", "5491112345678"),
            ("+5491112345678", "541112345678"),
            ("+525512345678", "525512345678"),
            ("+5215512345678", "5215512345678"),
            ("+541112345678", "541112345678"),
            ("+5491112345678", "5491112345678"),
            ("+593991234567", "593991234567"),
            ("+15551234567", "15551234567"),
        ] {
            let confirmed = format!("{confirmed}@s.whatsapp.net");
            assert_eq!(
                lookup_result(0, phone, Some(&confirmed)),
                Ok(Some(JID::from(confirmed.clone()))),
                "{phone} -> {confirmed}"
            );
        }
    }

    #[test]
    fn rejects_unsupported_number_equivalences_in_both_directions() {
        for (left, right) in [
            ("525512345678", "5215512345679"), // Different national suffix.
            ("541112345678", "5491112345679"),
            ("52551234567", "521551234567"), // Nine national digits.
            ("54111234567", "549111234567"),
            ("5255123456789", "52155123456789"), // Eleven national digits.
            ("5411123456789", "54911123456789"),
            ("593991234567", "5931991234567"), // Unsupported country.
            ("535512345678", "5315512345678"),
            ("525512345678", "5495512345678"), // Different country.
            ("54111512345678", "5491112345678"), // Domestic 15 is not stripped.
            ("5401112345678", "5491112345678"), // Domestic zero is not stripped.
        ] {
            for (requested, confirmed) in [(left, right), (right, left)] {
                let phone = format!("+{requested}");
                let jid = format!("{confirmed}@s.whatsapp.net");
                assert_eq!(
                    lookup_result(0, &phone, Some(&jid)),
                    Err(PhoneLookupError::InvalidBridgeResult),
                    "{phone} -> {jid}"
                );
            }
        }
    }

    #[test]
    fn rejects_noncanonical_confirmed_pn_strings() {
        for (phone, confirmed) in [
            ("+525512345678", "5215512345678"),
            ("+5215512345678", "525512345678"),
            ("+541112345678", "5491112345678"),
            ("+5491112345678", "541112345678"),
            ("+525512345678", "525512345678"),
            ("+541112345678", "541112345678"),
        ] {
            for jid in [
                format!("{confirmed}@lid"),
                format!("{confirmed}@c.us"),
                format!("{confirmed}@g.us"),
                format!("{confirmed}@S.WHATSAPP.NET"),
                format!("{confirmed}:1@s.whatsapp.net"),
                format!("{confirmed}.1:1@s.whatsapp.net"),
                format!("{confirmed}_1@s.whatsapp.net"),
                format!("+{confirmed}@s.whatsapp.net"),
            ] {
                assert_eq!(
                    lookup_result(0, phone, Some(&jid)),
                    Err(PhoneLookupError::InvalidBridgeResult),
                    "{phone} -> {jid}"
                );
            }
        }
    }

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
        let Err(PhoneLookupError::ResponseObservation(observation)) = result else {
            panic!("expected a local observation");
        };
        assert_eq!(observation.display_text(), text);
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
