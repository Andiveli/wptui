use std::ffi::CStr;
use std::sync::Arc;

use super::{C_FreeCommunities, C_FreeContacts, C_GetCommunities, C_GetContacts};
use super::{CommunitiesError, CommunityInfo, JID};

/// Returns all contacts and groups as (JID, display name). Includes LID aliases for contacts.
pub fn get_contacts() -> Vec<(JID, Arc<str>)> {
    let result = unsafe { C_GetContacts() };
    let entries = unsafe { std::slice::from_raw_parts(result.entries, result.size as usize) };

    let contacts = entries
        .iter()
        .map(|e| {
            let jid: JID = (&e.jid).into();
            let name = unsafe { CStr::from_ptr(e.name) }
                .to_string_lossy()
                .into_owned()
                .into();
            (jid, name)
        })
        .collect();
    unsafe { C_FreeContacts(result) };
    contacts
}

const COMMUNITY_ANNOUNCEMENT_UNKNOWN: u8 = 0;
const COMMUNITY_ANNOUNCEMENT_NO: u8 = 1;
const COMMUNITY_ANNOUNCEMENT_YES: u8 = 2;

fn community_announcement_from_code(code: u8) -> Option<bool> {
    match code {
        COMMUNITY_ANNOUNCEMENT_UNKNOWN => None,
        COMMUNITY_ANNOUNCEMENT_NO => Some(false),
        COMMUNITY_ANNOUNCEMENT_YES => Some(true),
        _ => None,
    }
}

fn community_participant_count_from_abi(value: i64) -> Option<u32> {
    (value >= 0).then(|| u32::try_from(value).ok()).flatten()
}

/// Returns real community roots and linked groups reported by WhatsApp.
pub fn get_communities() -> Result<Vec<CommunityInfo>, CommunitiesError> {
    let result = unsafe { C_GetCommunities() };
    if result.status != 0 {
        unsafe { C_FreeCommunities(result) };
        return Err(CommunitiesError::BridgeUnavailable);
    }
    if result.entries.is_null() || result.size == 0 {
        unsafe { C_FreeCommunities(result) };
        return Ok(Vec::new());
    }
    let entries = unsafe { std::slice::from_raw_parts(result.entries, result.size as usize) };
    let communities = entries
        .iter()
        .map(|entry| {
            let parent = unsafe { CStr::from_ptr(entry.parent_jid) }.to_string_lossy();
            CommunityInfo {
                jid: (&entry.jid).into(),
                name: unsafe { CStr::from_ptr(entry.name) }
                    .to_string_lossy()
                    .into_owned()
                    .into(),
                parent_jid: (!parent.is_empty()).then(|| parent.to_string().into()),
                is_parent: entry.is_parent,
                is_joined: entry.is_joined,
                is_default_subgroup: entry.is_default_subgroup,
                is_announce: community_announcement_from_code(entry.announcement),
                participant_count: community_participant_count_from_abi(entry.participant_count),
            }
        })
        .collect::<Vec<_>>();
    unsafe { C_FreeCommunities(result) };
    Ok(communities)
}

#[cfg(test)]
mod tests {
    use super::{community_announcement_from_code, community_participant_count_from_abi};

    #[test]
    fn maps_community_announcement_tristate() {
        assert_eq!(community_announcement_from_code(0), None);
        assert_eq!(community_announcement_from_code(1), Some(false));
        assert_eq!(community_announcement_from_code(2), Some(true));
        assert_eq!(community_announcement_from_code(255), None);
    }

    #[test]
    fn maps_community_participant_count_without_truncation() {
        assert_eq!(community_participant_count_from_abi(-1), None);
        assert_eq!(community_participant_count_from_abi(0), Some(0));
        assert_eq!(
            community_participant_count_from_abi(i64::from(u32::MAX)),
            Some(u32::MAX)
        );
        assert_eq!(
            community_participant_count_from_abi(i64::from(u32::MAX) + 1),
            None
        );
    }
}
