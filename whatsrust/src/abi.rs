use std::ffi::c_char;

pub(crate) type CJID = *const c_char;

#[repr(C)]
pub(crate) struct CContact {
    pub(crate) found: bool,
    pub(crate) first_name: *const c_char,
    pub(crate) full_name: *const c_char,
    pub(crate) push_name: *const c_char,
    pub(crate) business_name: *const c_char,
}

#[repr(C)]
pub(crate) struct CContactEntry {
    pub(crate) jid: CJID,
    pub(crate) name: *const c_char,
}

#[repr(C)]
pub(crate) struct CCommunityEntry {
    pub(crate) jid: CJID,
    pub(crate) name: *const c_char,
    pub(crate) parent_jid: CJID,
    pub(crate) is_parent: bool,
    pub(crate) is_joined: bool,
    pub(crate) is_default_subgroup: bool,
    // Stable C encoding: 0 unknown, 1 no, 2 yes.
    pub(crate) announcement: u8,
    // Stable C encoding: -1 unknown, otherwise a known signed count.
    pub(crate) participant_count: i64,
}

#[repr(C)]
pub(crate) struct CGetContactsResult {
    pub(crate) entries: *const CContactEntry,
    pub(crate) size: u32,
}

#[repr(C)]
pub(crate) struct CGetCommunitiesResult {
    pub(crate) entries: *const CCommunityEntry,
    pub(crate) size: u32,
    pub(crate) status: u8,
}

#[repr(C)]
pub(crate) struct CProfilePictureResult {
    pub(crate) status: u8,
    pub(crate) picture_id: *mut c_char,
    pub(crate) picture_type: *mut c_char,
    pub(crate) data: *mut u8,
    pub(crate) size: u32,
}

#[repr(C)]
pub(crate) struct CChatSettings {
    pub(crate) found: bool,
    pub(crate) muted_until: i64,
    pub(crate) pinned: bool,
    pub(crate) archived: bool,
}

#[repr(C)]
pub(crate) struct CGroupInfoResult {
    pub(crate) status: u8,
    pub(crate) is_announce: bool,
    pub(crate) is_admin: bool,
}

#[repr(C)]
pub(crate) struct CGroupParticipantEntry {
    pub(crate) jid: CJID,
    pub(crate) phone_number: CJID,
    pub(crate) name: *const c_char,
}

#[repr(C)]
pub(crate) struct CGroupParticipantsResult {
    pub(crate) entries: *const CGroupParticipantEntry,
    pub(crate) size: u32,
}
