use whatsrust as wr;

pub trait CommunityQueryPort {
    fn get_communities(&self) -> Result<Vec<wr::CommunityInfo>, wr::CommunitiesError>;
}
