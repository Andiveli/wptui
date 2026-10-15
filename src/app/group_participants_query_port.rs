use whatsrust as wr;

pub trait GroupParticipantsQueryPort {
    fn get_group_participants(&self, jid: &wr::JID) -> Vec<wr::GroupParticipant>;
}
