use std::sync::Arc;

use whatsrust as wr;

pub trait GroupParticipantsQueryPort {
    fn get_group_participants(&self, jid: &wr::JID) -> Vec<wr::GroupParticipant>;
}

impl<T: GroupParticipantsQueryPort + ?Sized> GroupParticipantsQueryPort for Arc<T> {
    fn get_group_participants(&self, jid: &wr::JID) -> Vec<wr::GroupParticipant> {
        (**self).get_group_participants(jid)
    }
}
