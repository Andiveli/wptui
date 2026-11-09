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
    fn load_native_link_previews(
        &self,
    ) -> Vec<crate::app::native_link_previews::NativeLinkPreviewRecord> {
        Vec::new()
    }
}
