use whatsrust as wr;

pub trait PresenceSubscriptionPort {
    fn subscribe(&self, jid: &wr::JID) -> wr::SubscribePresenceResult;
}
