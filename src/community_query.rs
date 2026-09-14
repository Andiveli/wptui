use crate::app::CommunityQueryPort;
use whatsrust as wr;

pub struct WhatsRustCommunityQuery;

impl CommunityQueryPort for WhatsRustCommunityQuery {
    fn get_communities(&self) -> Result<Vec<wr::CommunityInfo>, wr::CommunitiesError> {
        wr::get_communities()
    }
}
