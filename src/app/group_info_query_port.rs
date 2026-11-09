use std::sync::Arc;

use whatsrust as wr;

pub trait GroupInfoQueryPort {
    fn get_group_info(&self, jid: &wr::JID) -> Result<wr::GroupInfo, wr::GroupInfoError>;
}

impl<T: GroupInfoQueryPort + ?Sized> GroupInfoQueryPort for Arc<T> {
    fn get_group_info(&self, jid: &wr::JID) -> Result<wr::GroupInfo, wr::GroupInfoError> {
        (**self).get_group_info(jid)
    }
}
