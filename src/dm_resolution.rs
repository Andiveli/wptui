use crate::app::DmResolverPort;
use whatsrust as wr;

pub struct WhatsRustDmResolver;

impl DmResolverPort for WhatsRustDmResolver {
    fn resolve_dm_chat(&self, sender: &wr::JID) -> Option<wr::JID> {
        wr::resolve_dm_chat(sender)
    }
}
