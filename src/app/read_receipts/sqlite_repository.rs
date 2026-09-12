#[cfg(test)]
mod tests {
    use crate::app::read_receipts::{
        MAX_PENDING, PendingReceiptRepository, ReceiptCandidate, ReceiptKind,
    };
    use crate::db::SqlitePendingReceiptRepository as SqliteRepository;
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

    #[test]
    fn sqlite_pending_receipt_adapter_excludes_terminal_receipts_after_reopen() {
        let (directory, repository) = repository_with_pending(0);
        let completed = ReceiptCandidate {
            chat: "completed@s.whatsapp.net".into(),
            sender: "sender@s.whatsapp.net".into(),
            message_id: "completed".into(),
            timestamp: 1,
            kind: ReceiptKind::Chat,
            from_me: false,
            unsupported: false,
            visible: true,
            active: true,
        };
        let rejected = ReceiptCandidate {
            chat: "rejected@s.whatsapp.net".into(),
            sender: "sender@s.whatsapp.net".into(),
            message_id: "rejected".into(),
            timestamp: 2,
            kind: ReceiptKind::Chat,
            from_me: false,
            unsupported: false,
            visible: true,
            active: true,
        };
        let control = ReceiptCandidate {
            chat: "control@s.whatsapp.net".into(),
            sender: "sender@s.whatsapp.net".into(),
            message_id: "control".into(),
            timestamp: 3,
            kind: ReceiptKind::Chat,
            from_me: false,
            unsupported: false,
            visible: true,
            active: true,
        };
        let path = directory.path().join("read-receipts.sqlite");

        repository.save(&completed).unwrap();
        repository.save(&rejected).unwrap();
        repository.save(&control).unwrap();
        repository.complete_success(&completed.key()).unwrap();
        repository.reject(&rejected.key()).unwrap();
        drop(repository);

        let reopened = crate::db::SqlitePendingReceiptRepository::new(path);
        let loaded = reopened.load().unwrap();

        assert!(
            !loaded
                .iter()
                .any(|candidate| candidate.key() == completed.key())
        );
        assert!(
            !loaded
                .iter()
                .any(|candidate| candidate.key() == rejected.key())
        );
        assert!(
            loaded
                .iter()
                .any(|candidate| candidate.key() == control.key())
        );
        assert!(reopened.was_sent(&completed.key()).unwrap());
        assert!(reopened.was_sent(&rejected.key()).unwrap());
        assert!(!reopened.was_sent(&control.key()).unwrap());
    }
}
