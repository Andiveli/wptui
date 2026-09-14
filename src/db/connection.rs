use std::{
    path::Path,
    sync::{LazyLock, Mutex},
    time::Duration,
};

use rusqlite::Connection;

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(2);

// SQLite allows only one writer. All handlers in this process share this lock,
// including the asynchronous queue writer started by each handler.
pub(crate) static DATABASE_WRITE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(crate) fn open_database(path: &Path) -> Connection {
    let db = Connection::open(path).unwrap();
    db.busy_timeout(SQLITE_BUSY_TIMEOUT).unwrap();
    db
}

pub(crate) fn try_open_database(path: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open(path)?;
    db.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
    Ok(db)
}

pub(crate) fn with_database_write_lock<T>(operation: impl FnOnce() -> T) -> T {
    let _lock = DATABASE_WRITE_LOCK.lock().unwrap();
    operation()
}
