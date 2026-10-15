use whatsrust as wr;

use crate::app::avatar_query_port::AvatarQueryPort;

pub struct WhatsRustAvatarQuery;

impl AvatarQueryPort for WhatsRustAvatarQuery {
    fn get_profile_picture(
        &self,
        jid: &wr::JID,
    ) -> Result<wr::ProfilePictureAvailability, wr::ProfilePictureError> {
        wr::get_profile_picture(jid)
    }

    fn get_community_profile_picture(
        &self,
        jid: &wr::JID,
    ) -> Result<wr::ProfilePictureAvailability, wr::ProfilePictureError> {
        wr::get_community_profile_picture(jid)
    }
}
