use std::path::{Path, PathBuf};

use crate::app::chat_store::hydration_port::{
    ChatStoreHydration, ChatStoreHydrationPort, CompleteChatStoreHydration,
};

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
    fn try_load_complete(&self) -> rusqlite::Result<CompleteChatStoreHydration> {
        let mut db = super::try_open_database(&self.db_path)?;
        // Preserve legacy preparation, but never attempt schema writes inside
        // the read snapshot. No lock/timeout policy is changed here.
        super::schema::try_prepare_legacy_message_schema(&db)?;
        let tx = db.transaction()?;
        let complete = CompleteChatStoreHydration {
            snapshot: ChatStoreHydration {
                chats: super::chat_store::try_get_chats(&tx)?,
                contacts: super::chat_store::try_get_contacts(&tx)?,
                messages: super::message_store::try_get_messages(&tx)?,
                reactions: super::reaction_repository::try_get(&tx)?,
            },
            native_link_previews: super::message_store::try_get_native_link_previews(&tx)?,
        };
        tx.commit()?;
        Ok(complete)
    }

    fn load_native_link_previews(
        &self,
    ) -> Vec<crate::app::native_link_previews::NativeLinkPreviewRecord> {
        let db = super::open_database(&self.db_path);
        super::message_store::get_native_link_previews(&db)
    }

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
