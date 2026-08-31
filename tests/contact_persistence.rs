use std::sync::Arc;

use tempfile::tempdir;
use wp_tui::{
    app::chat_store::{ContactWritePort, PersistContact},
    db::{DatabaseHandler, SqliteContactWriter},
};

#[test]
fn sqlite_contact_writer_persists_raw_contacts_and_replaces_by_jid() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("contacts.db");
    let jid = whatsrust::JID("alice:7@example.test".into());
    let mut handler = DatabaseHandler::new(&path);
    handler.init();

    let writer = SqliteContactWriter::new(&path);
    let port: &dyn ContactWritePort = &writer;
    port.persist(PersistContact {
        jid: jid.clone(),
        name: Arc::from("~ Raw Name"),
    });
    port.persist(PersistContact {
        jid: jid.clone(),
        name: Arc::from("+ Replacement Name"),
    });
    handler.stop();

    let mut reopened = DatabaseHandler::new(&path);
    assert_eq!(
        reopened.get_contacts(),
        vec![(jid, Arc::from("+ Replacement Name"))]
    );
    reopened.stop();
}
