use crate::app::GroupParticipantsQueryPort;
use whatsrust as wr;

pub struct WhatsRustGroupParticipantsQuery;

impl GroupParticipantsQueryPort for WhatsRustGroupParticipantsQuery {
    fn get_group_participants(&self, jid: &wr::JID) -> Vec<wr::GroupParticipant> {
        wr::get_group_participants(jid)
    }
}
