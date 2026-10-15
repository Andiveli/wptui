use whatsrust as wr;

pub trait DmResolverPort {
    fn resolve_dm_chat(&self, sender: &wr::JID) -> Option<wr::JID>;
}
