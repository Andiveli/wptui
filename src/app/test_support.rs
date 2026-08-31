use super::{App, Clock, NotificationProjection, Notifier};
use crate::db::{DatabaseHandler, SqliteChatStoreHydration};
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
        std::mem::replace(&mut app.db_handler, DatabaseHandler::new(&db_path)).stop();
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
