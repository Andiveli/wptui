use super::{
    App, ChatReadCursorPort, Clock, NotificationProjection, Notifier, StatusCursorError,
    StatusCursorPort, StoreChatReadCursor, StoreStatusCursor,
};
use crate::db::{
    DatabaseHandler, SqliteChatReadCursor, SqliteChatStoreHydration, SqliteContactWriter,
    SqliteMessageReactionWriter, SqliteStatusCursor,
};
use std::path::Path;
use std::sync::{Arc, Mutex};
use whatsrust as wr;
pub(crate) struct TestApp {
    pub(crate) app: App<'static>,
    _dir: tempfile::TempDir,
}

#[derive(Debug)]
pub(crate) struct FixedClock(pub(crate) Option<i64>);

impl FixedClock {
    pub(crate) fn new(value: i64) -> Self {
        Self(Some(value))
    }
}

impl Clock for FixedClock {
    fn unix_seconds(&self) -> Option<i64> {
        self.0
    }
}

#[derive(Clone)]
pub(crate) struct MutableClock(Arc<Mutex<Option<i64>>>);

impl MutableClock {
    pub(crate) fn new(value: Option<i64>) -> Self {
        Self(Arc::new(Mutex::new(value)))
    }

    pub(crate) fn set(&self, value: Option<i64>) {
        *self.0.lock().unwrap() = value;
    }
}

impl Clock for MutableClock {
    fn unix_seconds(&self) -> Option<i64> {
        *self.0.lock().unwrap()
    }
}

#[derive(Clone, Default)]
pub(crate) struct RecordingNotifier {
    pub(crate) notifications: Arc<Mutex<Vec<(String, String)>>>,
}

#[derive(Clone, Default)]
pub(crate) struct FakeStatusCursorPort {
    pub(crate) loaded: Arc<Mutex<Vec<(wr::JID, i64)>>>,
    pub(crate) stored: Arc<Mutex<Vec<StoreStatusCursor>>>,
    pub(crate) fails: Arc<Mutex<bool>>,
}

impl StatusCursorPort for FakeStatusCursorPort {
    fn load(&self) -> Result<Vec<(wr::JID, i64)>, StatusCursorError> {
        Ok(self.loaded.lock().unwrap().clone())
    }

    fn store(&self, command: StoreStatusCursor) -> Result<(), StatusCursorError> {
        self.stored.lock().unwrap().push(command);
        if *self.fails.lock().unwrap() {
            Err(StatusCursorError("store failed".into()))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct FakeChatReadCursorPort {
    pub(crate) loaded: Arc<Mutex<Vec<(wr::JID, wr::MessageId, i64)>>>,
    pub(crate) stored: Arc<Mutex<Vec<StoreChatReadCursor>>>,
    pub(crate) panic_on_store: Arc<Mutex<bool>>,
}

impl ChatReadCursorPort for FakeChatReadCursorPort {
    fn load(&self) -> Vec<(wr::JID, wr::MessageId, i64)> {
        self.loaded.lock().unwrap().clone()
    }

    fn store(&self, command: StoreChatReadCursor) {
        self.stored.lock().unwrap().push(command);
        assert!(
            !*self.panic_on_store.lock().unwrap(),
            "cursor storage failed"
        );
    }
}

impl Notifier for RecordingNotifier {
    fn show(&self, notification: &NotificationProjection) -> Result<(), String> {
        self.notifications
            .lock()
            .unwrap()
            .push((notification.summary.to_string(), notification.body.clone()));
        Ok(())
    }
}
impl TestApp {
    pub(crate) fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let app = App::with_data_dir(dir.path(), dir.path());
        app.db_handler.init();
        Self { app, _dir: dir }
    }
    pub(crate) fn with_database(path: &Path) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_data_dir(dir.path(), dir.path());
        app.db_handler.init();
        let db_path = path.join("app.db");
        let db_handler = DatabaseHandler::new(&db_path);
        app.chat_store_write = Box::new(db_handler.chat_store_writer());
        app.set_contact_write(Box::new(SqliteContactWriter::new(&db_path)));
        app.set_message_reaction_write(Box::new(SqliteMessageReactionWriter::new(&db_path)));
        app.set_chat_read_cursor(Box::new(SqliteChatReadCursor::new(&db_path)));
        app.set_status_cursor(Box::new(SqliteStatusCursor::new(&db_path)));
        std::mem::replace(&mut app.db_handler, db_handler).stop();
        app.chat_store_hydration = Box::new(SqliteChatStoreHydration::new(&db_path));
        app.db_handler.init();
        Self { app, _dir: dir }
    }

    pub(crate) fn with_ports<C, N>(clock: C, notifier: N) -> Self
    where
        C: Clock + 'static,
        N: Notifier + 'static,
    {
        let dir = tempfile::tempdir().unwrap();
        let app = App::with_data_dir_and_ports(
            dir.path(),
            dir.path(),
            Box::new(clock),
            Box::new(notifier),
        );
        app.db_handler.init();
        Self { app, _dir: dir }
    }
}
impl Drop for TestApp {
    fn drop(&mut self) {
        self.app.db_handler.stop();
    }
}
impl std::ops::Deref for TestApp {
    type Target = App<'static>;

    fn deref(&self) -> &Self::Target {
        &self.app
    }
}
impl std::ops::DerefMut for TestApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.app
    }
}
pub(crate) fn message(chat: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: chat.clone(),
            mentions_self: false,
            timestamp,
            forwarding: Default::default(),
            is_from_me: false,
            quote_id: None,
            read_by: 0,
        },
        message: wr::MessageContent::Text(id.into()),
    }
}
