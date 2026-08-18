use std::path::PathBuf;

use rusqlite::{Error, ErrorCode, params};

use super::{
    MAX_PENDING, PendingReceiptRepository, ReceiptCandidate, ReceiptKey, ReceiptKind,
    RepositoryError,
};

/// Every eligible candidate is durable before it enters the bounded working set.
/// Therefore memory pressure cannot silently lose a receipt that may leave the
/// viewport unread; successful completion removes exactly one pending key and
/// records its full identity so restart cannot replay loaded history.
pub struct SqliteRepository {
    path: PathBuf,
}

impl SqliteRepository {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    fn connection(&self) -> Result<rusqlite::Connection, RepositoryError> {
        crate::db::try_open_database(&self.path).map_err(classify)
    }
}

fn classify(error: Error) -> RepositoryError {
    match error {
        Error::SqliteFailure(ref failure, _)
            if matches!(
                failure.code,
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
            ) =>
        {
            RepositoryError::Busy
        }
        Error::InvalidQuery
        | Error::InvalidParameterName(_)
        | Error::InvalidParameterCount(_, _) => RepositoryError::Schema,
        _ => RepositoryError::Unavailable,
    }
}

impl PendingReceiptRepository for SqliteRepository {
    fn load(&self) -> Result<Vec<ReceiptCandidate>, RepositoryError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT p.chat, p.sender, p.message_id, p.timestamp, p.kind FROM read_receipt_pending p LEFT JOIN read_receipt_sent s ON s.chat = p.chat AND s.sender = p.sender AND s.message_id = p.message_id LEFT JOIN read_receipt_rejected r ON r.chat = p.chat AND r.sender = p.sender AND r.message_id = p.message_id WHERE s.message_id IS NULL AND r.message_id IS NULL ORDER BY p.timestamp LIMIT ?1").map_err(classify)?;
        statement
            .query_map([MAX_PENDING as i64], |row| {
                Ok(ReceiptCandidate {
                    chat: row.get(0)?,
                    sender: row.get(1)?,
                    message_id: row.get(2)?,
                    timestamp: row.get(3)?,
                    kind: if row.get::<_, i64>(4)? == 1 {
                        ReceiptKind::Status
                    } else {
                        ReceiptKind::Chat
                    },
                    from_me: false,
                    unsupported: false,
                    visible: true,
                    active: true,
                })
            })
            .map_err(classify)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(classify)
    }
    fn save(&self, candidate: &ReceiptCandidate) -> Result<(), RepositoryError> {
        crate::db::with_database_write_lock(|| {
            let connection = self.connection()?;
            connection.execute("INSERT OR IGNORE INTO read_receipt_pending (chat, sender, message_id, timestamp, kind) VALUES (?1, ?2, ?3, ?4, ?5)", params![&candidate.chat, &candidate.sender, &candidate.message_id, candidate.timestamp, matches!(candidate.kind, ReceiptKind::Status) as i64]).map(|_| ()).map_err(classify)
        })
    }
    fn was_sent(&self, key: &ReceiptKey) -> Result<bool, RepositoryError> {
        let connection = self.connection()?;
        connection.query_row("SELECT 1 FROM read_receipt_sent WHERE chat = ?1 AND sender = ?2 AND message_id = ?3 UNION ALL SELECT 1 FROM read_receipt_rejected WHERE chat = ?1 AND sender = ?2 AND message_id = ?3 LIMIT 1", params![&key.chat, &key.sender, &key.message_id], |_| Ok(())).map(|_| true).or_else(|error| match error { rusqlite::Error::QueryReturnedNoRows => Ok(false), other => Err(classify(other)) })
    }
    fn complete_success(&self, key: &ReceiptKey) -> Result<(), RepositoryError> {
        crate::db::with_database_write_lock(|| {
            let mut connection = self.connection()?;
            let transaction = connection.transaction().map_err(classify)?;
            transaction.execute("INSERT OR IGNORE INTO read_receipt_sent (chat, sender, message_id) VALUES (?1, ?2, ?3)", params![&key.chat, &key.sender, &key.message_id]).map_err(classify)?;
            transaction.execute("DELETE FROM read_receipt_pending WHERE chat = ?1 AND sender = ?2 AND message_id = ?3", params![&key.chat, &key.sender, &key.message_id]).map_err(classify)?;
            transaction.commit().map_err(classify)
        })
    }
    fn reject(&self, key: &ReceiptKey) -> Result<(), RepositoryError> {
        crate::db::with_database_write_lock(|| {
            let mut connection = self.connection()?;
            let transaction = connection.transaction().map_err(classify)?;
            transaction.execute("INSERT OR IGNORE INTO read_receipt_rejected (chat, sender, message_id) VALUES (?1, ?2, ?3)", params![&key.chat, &key.sender, &key.message_id]).map_err(classify)?;
            transaction.execute("DELETE FROM read_receipt_pending WHERE chat = ?1 AND sender = ?2 AND message_id = ?3", params![&key.chat, &key.sender, &key.message_id]).map_err(classify)?;
            transaction.commit().map_err(classify)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::read_receipts::{MAX_PENDING, PendingReceiptRepository};
    use tempfile::tempdir;

    fn repository_with_pending(count: usize) -> (tempfile::TempDir, SqliteRepository) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("read-receipts.sqlite");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE read_receipt_pending (
                    chat TEXT NOT NULL,
                    sender TEXT NOT NULL,
                    message_id TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    kind INTEGER NOT NULL,
                    PRIMARY KEY (chat, sender, message_id)
                );
                CREATE TABLE read_receipt_sent (
                    chat TEXT NOT NULL,
                    sender TEXT NOT NULL,
                    message_id TEXT NOT NULL,
                    PRIMARY KEY (chat, sender, message_id)
                );
                CREATE TABLE read_receipt_rejected (
                    chat TEXT NOT NULL,
                    sender TEXT NOT NULL,
                    message_id TEXT NOT NULL,
                    PRIMARY KEY (chat, sender, message_id)
                );",
            )
            .unwrap();
        for timestamp in 0..count {
            connection
                .execute(
                    "INSERT INTO read_receipt_pending
                        (chat, sender, message_id, timestamp, kind)
                     VALUES (?1, ?2, ?3, ?4, 0)",
                    rusqlite::params![
                        format!("chat-{timestamp}"),
                        "sender@example.test",
                        format!("message-{timestamp}"),
                        timestamp as i64,
                    ],
                )
                .unwrap();
        }
        drop(connection);
        (directory, SqliteRepository::new(path))
    }

    #[test]
    fn load_is_bounded_to_the_working_set() {
        let (_directory, repository) = repository_with_pending(MAX_PENDING + 1);

        let candidates = repository.load().unwrap();

        assert_eq!(candidates.len(), MAX_PENDING);
        assert_eq!(candidates.first().unwrap().timestamp, 0);
        assert_eq!(
            candidates.last().unwrap().timestamp,
            (MAX_PENDING - 1) as i64
        );
    }

    #[test]
    fn load_fills_the_bound_after_excluding_completed_rows() {
        let (directory, repository) = repository_with_pending(MAX_PENDING + 2);
        let connection =
            rusqlite::Connection::open(directory.path().join("read-receipts.sqlite")).unwrap();
        connection
            .execute(
                "INSERT INTO read_receipt_sent (chat, sender, message_id)
                 VALUES (?1, ?2, ?3)",
                rusqlite::params!["chat-0", "sender@example.test", "message-0"],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO read_receipt_rejected (chat, sender, message_id)
                 VALUES (?1, ?2, ?3)",
                rusqlite::params!["chat-1", "sender@example.test", "message-1"],
            )
            .unwrap();
        drop(connection);

        let candidates = repository.load().unwrap();

        assert_eq!(candidates.len(), MAX_PENDING);
        assert_eq!(candidates.first().unwrap().timestamp, 2);
        assert_eq!(
            candidates.last().unwrap().timestamp,
            (MAX_PENDING + 1) as i64
        );
    }
}
