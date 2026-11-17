/// Validate a server-confirmed canonical PN against the requested international number.
///
/// Accepts exact identities and only the symmetric Mexico 52/521 and Argentina
/// 54/549 forms with the same ten national digits. This checks identity, not
/// registration; callers must still require a successful server lookup.
/// Device, integrator, LID, and noncanonical namespace strings are not PNs.
pub fn is_confirmed_phone_lookup_identity(phone: &str, jid: &str) -> bool {
    let Some(requested) = phone.strip_prefix('+') else {
        return false;
    };
    let Some(confirmed) = jid.strip_suffix("@s.whatsapp.net") else {
        return false;
    };
    if !canonical_digits(requested) || !canonical_digits(confirmed) {
        return false;
    }
    if requested == confirmed {
        return true;
    }
    for (country, mobile) in [("52", "521"), ("54", "549")] {
        if equivalent_national_digits(requested, confirmed, country, mobile)
            || equivalent_national_digits(confirmed, requested, country, mobile)
        {
            return true;
        }
    }
    false
}

fn canonical_digits(digits: &str) -> bool {
    (7..=15).contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn equivalent_national_digits(short: &str, long: &str, country: &str, mobile: &str) -> bool {
    let Some(national) = short.strip_prefix(country) else {
        return false;
    };
    national.len() == 10 && long.strip_prefix(mobile) == Some(national)
}
