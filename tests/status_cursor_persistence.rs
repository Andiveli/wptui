use tempfile::tempdir;
use wp_tui::{
    app::{StatusCursorError, StatusCursorPort, StoreStatusCursor},
    db::{DatabaseHandler, SqliteStatusCursor},
};

#[test]
fn sqlite_status_cursor_keeps_the_newest_timestamp_after_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("status-cursors.db");
    let contact = whatsrust::JID("alice:7@example.test".into());
    let mut handler = DatabaseHandler::new(&path);
    handler.init();

    let cursor = SqliteStatusCursor::new(&path);
    let port: &dyn StatusCursorPort = &cursor;
    for timestamp in [42, 21, 84] {
        port.store(StoreStatusCursor {
            contact: contact.clone(),
            timestamp,
        })
        .unwrap();
    }
    assert_eq!(port.load().unwrap(), vec![(contact.clone(), 84)]);
    handler.stop();

    let mut reopened_handler = DatabaseHandler::new(&path);
    let reopened = SqliteStatusCursor::new(&path);
    assert_eq!(reopened.load().unwrap(), vec![(contact, 84)]);
    reopened_handler.stop();
}

#[test]
fn status_cursor_error_display_preserves_its_cause() {
    assert_eq!(
        StatusCursorError("database unavailable".into()).to_string(),
        "database unavailable"
    );
}
