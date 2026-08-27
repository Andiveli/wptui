use std::ffi::CStr;
use std::sync::Arc;

use super::{
    C_FreeCommunities, C_FreeContacts, C_FreeGroupParticipants, C_FreeResolveDmChatId,
    C_GetChatSettings, C_GetCommunities, C_GetContacts, C_GetGroupInfo, C_GetGroupParticipants,
    C_ResolveDmChatId, CJID,
};
use super::{
    ChatSettings, CommunitiesError, CommunityInfo, GroupInfo, GroupInfoError, GroupParticipant, JID,
};

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

pub fn get_chat_settings(jid: &JID) -> ChatSettings {
    let jid_c = CJID::from(jid);
    let settings = unsafe { C_GetChatSettings(jid_c) };

    ChatSettings {
        found: settings.found,
        muted_until: settings.muted_until,
        pinned: settings.pinned,
        archived: settings.archived,
    }
}

fn group_info_from_parts(
    jid: &JID,
    status: u8,
    is_announce: bool,
    is_admin: bool,
) -> Result<GroupInfo, GroupInfoError> {
    match status {
        0 => Ok(GroupInfo {
            jid: jid.clone(),
            name: "".into(),
            is_announce,
            is_admin,
        }),
        1 => Err(GroupInfoError::NotGroup),
        2 => Err(GroupInfoError::ClientUnavailable),
        3 => Err(GroupInfoError::RequestFailed),
        _ => Err(GroupInfoError::InvalidBridgeResult),
    }
}

pub fn get_group_info(jid: &JID) -> Result<GroupInfo, GroupInfoError> {
    let jid_c = CJID::from(jid);
    let result = unsafe { C_GetGroupInfo(jid_c) };
    group_info_from_parts(jid, result.status, result.is_announce, result.is_admin)
}

pub fn get_group_participants(jid: &JID) -> Vec<GroupParticipant> {
    let jid_c = CJID::from(jid);
    let result = unsafe { C_GetGroupParticipants(jid_c) };
    if result.entries.is_null() || result.size == 0 {
        return Vec::new();
    }
    let entries = unsafe { std::slice::from_raw_parts(result.entries, result.size as usize) };
    let participants = entries
        .iter()
        .filter_map(|entry| {
            let jid: JID = (!entry.jid.is_null()).then(|| (&entry.jid).into())?;
            let phone_number = if entry.phone_number.is_null() {
                jid.clone()
            } else {
                (&entry.phone_number).into()
            };
            let name = if entry.name.is_null() {
                Arc::<str>::from("")
            } else {
                unsafe { CStr::from_ptr(entry.name) }
                    .to_string_lossy()
                    .into_owned()
                    .into()
            };
            Some(GroupParticipant {
                jid,
                phone_number,
                name,
            })
        })
        .collect();
    unsafe { C_FreeGroupParticipants(result) };
    participants
}

pub fn resolve_dm_chat(jid: &JID) -> Option<JID> {
    unsafe {
        super::presence::take_owned_c_string(
            C_ResolveDmChatId(CJID::from(jid)),
            C_FreeResolveDmChatId,
        )
        .map(JID::from)
    }
}

#[cfg(test)]
mod group_info_tests {
    use super::{GroupInfoError, JID, group_info_from_parts};

    #[test]
    fn maps_announce_and_admin_flags() {
        let jid = JID("123@g.us".into());
        let info = group_info_from_parts(&jid, 0, true, false).unwrap();

        assert_eq!(info.jid, jid);
        assert!(info.is_announce);
        assert!(!info.is_admin);
    }

    #[test]
    fn maps_bridge_failures_without_claiming_send_permission() {
        for (status, expected) in [
            (1, GroupInfoError::NotGroup),
            (2, GroupInfoError::ClientUnavailable),
            (3, GroupInfoError::RequestFailed),
            (255, GroupInfoError::InvalidBridgeResult),
        ] {
            assert_eq!(
                group_info_from_parts(&JID("chat@g.us".into()), status, false, false),
                Err(expected)
            );
        }
    }
}
