use crate::app::GroupInfoQueryPort;
use whatsrust as wr;

pub struct WhatsRustGroupInfoQuery;

impl GroupInfoQueryPort for WhatsRustGroupInfoQuery {
    fn get_group_info(&self, jid: &wr::JID) -> Result<wr::GroupInfo, wr::GroupInfoError> {
        wr::get_group_info(jid)
    }
}
