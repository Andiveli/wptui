use whatsrust as wr;

pub trait GroupInfoQueryPort {
    fn get_group_info(&self, jid: &wr::JID) -> Result<wr::GroupInfo, wr::GroupInfoError>;
}
