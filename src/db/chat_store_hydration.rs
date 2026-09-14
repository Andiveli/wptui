use std::path::{Path, PathBuf};

use crate::app::chat_store::hydration_port::{ChatStoreHydration, ChatStoreHydrationPort};

pub struct SqliteChatStoreHydration {
    db_path: PathBuf,
}

impl SqliteChatStoreHydration {
    pub fn new(db_path: impl AsRef<Path>) -> Self {
        Self {
            db_path: db_path.as_ref().to_path_buf(),
        }
    }
}

impl ChatStoreHydrationPort for SqliteChatStoreHydration {
    fn load(&self) -> ChatStoreHydration {
        let db = super::open_database(&self.db_path);
        ChatStoreHydration {
            chats: super::chat_store::get_chats(&db),
            contacts: super::chat_store::get_contacts(&db),
            messages: super::message_store::get_messages(&db),
            reactions: super::reaction_repository::get(&db),
        }
    }
}
