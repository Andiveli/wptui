use std::{ffi::CStr, path::Path};

use super::*;

fn profile_picture_from_parts(
    status: u8,
    picture_id: &str,
    picture_type: &str,
    bytes: Vec<u8>,
) -> Result<ProfilePictureAvailability, ProfilePictureError> {
    match status {
        0 if !bytes.is_empty() => Ok(ProfilePictureAvailability::Available(ProfilePicture {
            id: picture_id.into(),
            picture_type: picture_type.into(),
            bytes,
        })),
        0 => Err(ProfilePictureError::InvalidBridgeResult),
        1 => Ok(ProfilePictureAvailability::Unavailable),
        2 => Err(ProfilePictureError::InvalidJid),
        3 => Err(ProfilePictureError::ClientUnavailable),
        4 => Err(ProfilePictureError::RequestCancelled),
        5 => Err(ProfilePictureError::Metadata),
        6 => Err(ProfilePictureError::EmptyUrl),
        7 => Err(ProfilePictureError::Download),
        8 => Err(ProfilePictureError::Oversized),
        9 => Err(ProfilePictureError::InvalidImage),
        _ => Err(ProfilePictureError::InvalidBridgeResult),
    }
}

fn get_profile_picture_from_bridge(
    jid: &JID,
    lookup: unsafe extern "C" fn(CJID) -> CProfilePictureResult,
) -> Result<ProfilePictureAvailability, ProfilePictureError> {
    let jid = CString::new(jid.0.as_ref()).map_err(|_| ProfilePictureError::InvalidJid)?;
    let result = unsafe { lookup(jid.as_ptr()) };
    let picture_id = if result.picture_id.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(result.picture_id) }
            .to_string_lossy()
            .into_owned()
    };
    let picture_type = if result.picture_type.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(result.picture_type) }
            .to_string_lossy()
            .into_owned()
    };
    let bytes = if result.data.is_null() || result.size == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(result.data, result.size as usize) }.to_vec()
    };
    let converted = profile_picture_from_parts(result.status, &picture_id, &picture_type, bytes);
    unsafe { C_FreeProfilePicture(result) };
    converted
}

pub fn get_profile_picture(jid: &JID) -> Result<ProfilePictureAvailability, ProfilePictureError> {
    get_profile_picture_from_bridge(jid, C_GetProfilePicture)
}

pub fn get_community_profile_picture(
    jid: &JID,
) -> Result<ProfilePictureAvailability, ProfilePictureError> {
    get_profile_picture_from_bridge(jid, C_GetCommunityProfilePicture)
}

pub fn download_file(file_id: &FileId, base_path: &Path) -> Result<(), DownloadFailed> {
    let file_id_c = CString::new(file_id.as_ref()).unwrap();
    let base_path_c = CString::new(base_path.to_str().unwrap()).unwrap();
    let code = unsafe { C_DownloadFile(file_id_c.as_ptr(), base_path_c.as_ptr()) };
    if code == 0 {
        Ok(())
    } else {
        Err(DownloadFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::{ProfilePictureAvailability, ProfilePictureError, profile_picture_from_parts};

    #[test]
    fn maps_available_payload_without_exposing_the_temporary_url() {
        let result = profile_picture_from_parts(0, "picture-42", "preview", vec![1, 2, 3]);
        let ProfilePictureAvailability::Available(picture) = result.unwrap() else {
            panic!("expected available profile picture");
        };
        assert_eq!(picture.id.as_ref(), "picture-42");
        assert_eq!(picture.picture_type.as_ref(), "preview");
        assert_eq!(picture.bytes, vec![1, 2, 3]);
    }

    #[test]
    fn maps_unavailable_and_invalid_bridge_results() {
        assert_eq!(
            profile_picture_from_parts(1, "", "", Vec::new()),
            Ok(ProfilePictureAvailability::Unavailable)
        );
        for (status, expected) in [
            (2, ProfilePictureError::InvalidJid),
            (3, ProfilePictureError::ClientUnavailable),
            (4, ProfilePictureError::RequestCancelled),
            (5, ProfilePictureError::Metadata),
            (6, ProfilePictureError::EmptyUrl),
            (7, ProfilePictureError::Download),
            (8, ProfilePictureError::Oversized),
            (9, ProfilePictureError::InvalidImage),
            (255, ProfilePictureError::InvalidBridgeResult),
        ] {
            assert_eq!(
                profile_picture_from_parts(status, "id", "preview", Vec::new()),
                Err(expected)
            );
        }
    }
}
