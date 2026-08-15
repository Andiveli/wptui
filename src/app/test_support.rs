use super::App;

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
