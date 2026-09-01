use std::path::{Path, PathBuf};

use crate::app::chat_store::read_cursor_port::{ChatReadCursorPort, StoreChatReadCursor};

use super::{cursor_repository, open_database};

pub struct SqliteChatReadCursor {
    db_path: PathBuf,
}

impl SqliteChatReadCursor {
    pub fn new(db_path: &Path) -> Self {
        Self {
            db_path: db_path.to_owned(),
        }
    }
}

impl ChatReadCursorPort for SqliteChatReadCursor {
    fn load(&self) -> Vec<(whatsrust::JID, whatsrust::MessageId, i64)> {
        let database = open_database(&self.db_path);
        cursor_repository::read_cursors(&database)
    }

    fn store(&self, command: StoreChatReadCursor) {
        let database = open_database(&self.db_path);
        cursor_repository::set_last_read(
            &database,
            &command.chat,
            command.message_id,
            command.timestamp,
        );
    }
}
