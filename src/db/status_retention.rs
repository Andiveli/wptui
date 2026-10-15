use std::path::{Path, PathBuf};

use crate::app::status_retention::{
    PurgeExpiredStatuses, PurgedExpiredStatuses, StatusRetentionError, StatusRetentionPort,
};

use super::{retention, try_open_database};

pub struct SqliteStatusRetention {
    db_path: PathBuf,
}

impl SqliteStatusRetention {
    pub fn new(db_path: &Path) -> Self {
        Self {
            db_path: db_path.to_owned(),
        }
    }
}

impl StatusRetentionPort for SqliteStatusRetention {
    fn purge_expired_statuses(
        &self,
        command: PurgeExpiredStatuses,
    ) -> Result<PurgedExpiredStatuses, StatusRetentionError> {
        let mut database = try_open_database(&self.db_path).map_err(map_error)?;
        retention::purge(&mut database, command.now)
            .map(|media_paths| PurgedExpiredStatuses { media_paths })
            .map_err(map_error)
    }
}

fn map_error(error: rusqlite::Error) -> StatusRetentionError {
    StatusRetentionError(error.to_string().into())
}
