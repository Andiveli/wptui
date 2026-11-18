use std::sync::Arc;

use whatsrust as wr;

use crate::app::Chat;

pub struct ChatStoreHydration {
    pub chats: Vec<Chat>,
    pub contacts: Vec<(wr::JID, Arc<str>)>,
    pub messages: Vec<wr::Message>,
    pub reactions: Vec<(wr::MessageId, wr::JID, Arc<str>)>,
}

pub struct CompleteChatStoreHydration {
    pub snapshot: ChatStoreHydration,
    pub native_link_previews: Vec<crate::app::native_link_previews::NativeLinkPreviewRecord>,
}

pub trait ChatStoreHydrationPort {
    /// Legacy implementors remain compatible; fallible adapters override this
    /// to read every projection before publishing any application state.
    fn try_load_complete(&self) -> rusqlite::Result<CompleteChatStoreHydration> {
        Ok(CompleteChatStoreHydration {
            snapshot: self.load(),
            native_link_previews: self.load_native_link_previews(),
        })
    }

    fn load(&self) -> ChatStoreHydration;
    fn load_native_link_previews(
        &self,
    ) -> Vec<crate::app::native_link_previews::NativeLinkPreviewRecord> {
        Vec::new()
    }
}
