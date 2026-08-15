use super::{App, Clock, Notifier};
use crate::db::DatabaseHandler;
use std::path::Path;
use whatsrust as wr;
pub(crate) struct TestApp {
    pub(crate) app: App<'static>,
    _dir: tempfile::TempDir,
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
        std::mem::replace(
            &mut app.db_handler,
            DatabaseHandler::new(&path.join("app.db")),
        )
        .stop();
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
            timestamp,
            forwarding: Default::default(),
            is_from_me: false,
            quote_id: None,
            read_by: 0,
        },
        message: wr::MessageContent::Text(id.into()),
    }
}
