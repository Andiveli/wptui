use std::path::{Path, PathBuf};

use crate::app::message_reactions::{MessageReactionWritePort, RecordMessageReaction};

use super::{open_database, reaction_repository};

pub struct SqliteMessageReactionWriter {
    db_path: PathBuf,
}

impl SqliteMessageReactionWriter {
    pub fn new(db_path: &Path) -> Self {
        Self {
            db_path: db_path.to_owned(),
        }
    }
}

impl MessageReactionWritePort for SqliteMessageReactionWriter {
    fn record(&self, command: RecordMessageReaction) {
        let db = open_database(&self.db_path);
        reaction_repository::record(&db, &command.message_id, command.participant, command.emoji);
    }
}
