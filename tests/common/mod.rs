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
use wp_tui::db::{
    DatabaseHandler, SqliteChatReadCursor, SqliteChatStoreHydration, SqliteContactWriter,
    SqliteMessageReactionWriter, SqliteStatusCursor, SqliteStatusRetention,
};

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

    /// Loads real settings from an isolated test directory before bootstrap.
    pub fn with_settings(settings: &str) -> Self {
        Self::with_config_files(settings, None)
    }

    /// Writes an optional sibling theme before the ordinary settings bootstrap.
    pub fn with_config_files(settings: &str, theme_json: Option<&str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.conf"), settings).unwrap();
        if let Some(contents) = theme_json {
            std::fs::write(dir.path().join("theme.json"), contents).unwrap();
        }
        let app: App<'static> = App::with_data_dir(dir.path(), dir.path());
        app.db_handler.init();
        Self { app, _dir: dir }
    }

    /// Exposes only this fixture's settings path, never the user's config.
    pub fn settings_path(&self) -> std::path::PathBuf {
        self._dir.path().join("settings.conf")
    }

    /// A directory at the settings path rejects writes even when run as root.
    pub fn with_unwritable_settings() -> Self {
        let app = Self::new();
        std::fs::create_dir(app.settings_path()).unwrap();
        std::fs::write(app.settings_path().join("sentinel"), "keep\n").unwrap();
        app
    }

    /// Builds an app whose `db_handler` targets the already-initialized
    /// database at `path`. The base app still lives in a private tempdir.
    pub fn with_database(path: &Path) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut app: App<'static> = App::with_data_dir(dir.path(), dir.path());
        app.db_handler.init();
        let db_handler = DatabaseHandler::new(path);
        app.set_chat_store_write(Box::new(db_handler.chat_store_writer()));
        app.set_contact_write(Box::new(SqliteContactWriter::new(path)));
        app.set_message_reaction_write(Box::new(SqliteMessageReactionWriter::new(path)));
        app.set_chat_read_cursor(Box::new(SqliteChatReadCursor::new(path)));
        app.set_status_cursor(Box::new(SqliteStatusCursor::new(path)));
        app.set_status_retention(Box::new(SqliteStatusRetention::new(path)));
        std::mem::replace(&mut app.db_handler, db_handler).stop();
        app.set_chat_store_hydration(Box::new(SqliteChatStoreHydration::new(path)));
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
