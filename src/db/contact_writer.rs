use std::path::{Path, PathBuf};

use crate::app::chat_store::contact_write_port::{ContactWritePort, PersistContact};

pub struct SqliteContactWriter {
    db_path: PathBuf,
}

impl SqliteContactWriter {
    pub fn new(db_path: impl AsRef<Path>) -> Self {
        Self {
            db_path: db_path.as_ref().to_path_buf(),
        }
    }
}

impl ContactWritePort for SqliteContactWriter {
    fn persist(&self, command: PersistContact) {
        let db = super::open_database(&self.db_path);
        super::chat_store::add_contact(&db, &command.jid, command.name.as_ref());
    }
}
