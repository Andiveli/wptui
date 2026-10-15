use crate::app::presence_subscription_port::PresenceSubscriptionPort;
use whatsrust as wr;

pub struct WhatsRustPresenceSubscription;

impl PresenceSubscriptionPort for WhatsRustPresenceSubscription {
    fn subscribe(&self, jid: &wr::JID) -> wr::SubscribePresenceResult {
        wr::subscribe_presence(jid)
    }
}
