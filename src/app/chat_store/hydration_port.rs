use std::sync::Arc;

use whatsrust as wr;

use crate::app::Chat;

pub struct ChatStoreHydration {
    pub chats: Vec<Chat>,
    pub contacts: Vec<(wr::JID, Arc<str>)>,
    pub messages: Vec<wr::Message>,
    pub reactions: Vec<(wr::MessageId, wr::JID, Arc<str>)>,
}

pub trait ChatStoreHydrationPort {
    fn load(&self) -> ChatStoreHydration;
}
