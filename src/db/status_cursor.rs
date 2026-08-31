use std::path::{Path, PathBuf};

use crate::app::status_cursor::{StatusCursorError, StatusCursorPort, StoreStatusCursor};

use super::{cursor_repository, try_open_database};

pub struct SqliteStatusCursor {
    db_path: PathBuf,
}

impl SqliteStatusCursor {
    pub fn new(db_path: &Path) -> Self {
        Self {
            db_path: db_path.to_owned(),
        }
    }
}

impl StatusCursorPort for SqliteStatusCursor {
    fn load(&self) -> Result<Vec<(whatsrust::JID, i64)>, StatusCursorError> {
        let database = try_open_database(&self.db_path).map_err(map_error)?;
        cursor_repository::status_last_seen(&database).map_err(map_error)
    }

    fn store(&self, command: StoreStatusCursor) -> Result<(), StatusCursorError> {
        let database = try_open_database(&self.db_path).map_err(map_error)?;
        cursor_repository::set_status_last_seen(&database, &command.contact, command.timestamp)
            .map(|_| ())
            .map_err(map_error)
    }
}

fn map_error(error: rusqlite::Error) -> StatusCursorError {
    StatusCursorError(error.to_string().into())
}
