use std::ffi::{CStr, CString};

use crate::abi::{C_FreeLookupPhoneResult, C_LookupPhone};
use crate::models::JID;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhoneLookupError {
    InvalidNumber,
    ClientUnavailable,
    RequestFailed,
    InvalidBridgeResult,
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
        _ => Err(PhoneLookupError::InvalidBridgeResult),
    }
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
