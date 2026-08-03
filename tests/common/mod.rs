//! Shared test support for integration tests.
//!
//! `TestApp` is the only supported way to build an `App` in tests: it points
//! every storage path at a fresh tempdir (so no test ever opens or writes the
//! real user database under `~/.local/share/wptui/whatsapp.db`) and stops and
//! joins the `DatabaseHandler` background writer thread on drop (so a leaked
//! thread can never panic while holding the process-global write lock and
//! poison later tests).
#![allow(dead_code)]

use std::ops::{Deref, DerefMut};
use std::path::Path;

use tempfile::TempDir;
use wp_tui::app::App;
use wp_tui::db::DatabaseHandler;

pub struct TestApp {
    app: App<'static>,
    _dir: TempDir,
}

impl TestApp {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let app: App<'static> = App::with_data_dir(dir.path(), dir.path());
        // Full schema up front: the background writer thread drains the queues
        // asynchronously and must never hit a missing table.
        app.db_handler.init();
        Self { app, _dir: dir }
    }

    /// Builds an app whose `db_handler` targets the already-initialized
    /// database at `path`. The base app still lives in a private tempdir.
    pub fn with_database(path: &Path) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut app: App<'static> = App::with_data_dir(dir.path(), dir.path());
        app.db_handler.init();
        std::mem::replace(&mut app.db_handler, DatabaseHandler::new(path)).stop();
        app.db_handler.init();
        Self { app, _dir: dir }
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        self.app.db_handler.stop();
    }
}

impl Deref for TestApp {
    type Target = App<'static>;
    fn deref(&self) -> &Self::Target {
        &self.app
    }
}

impl DerefMut for TestApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.app
    }
}
