use std::collections::HashMap;
use std::path::Path;

use tempfile::tempdir;
use wp_tui::db::DatabaseHandler;
use wp_tui::ui::message_list::reaction_chips;
mod common;
use common::TestApp;

#[test]
fn reactions_survive_restart_replace_per_participant_and_delete_individually() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("reactions.db");

    let mut database = DatabaseHandler::new(&path);
    database.init();
    database.record_reaction(
        &"message-1".into(),
        String::from("alice@example.test").into(),
        "👍".into(),
    );
    database.record_reaction(
        &"message-1".into(),
        String::from("alice@example.test").into(),
        "❤️".into(),
    );
    database.record_reaction(
        &"message-1".into(),
        String::from("bob@example.test").into(),
        "😂".into(),
    );
    database.stop();

    let mut restarted = DatabaseHandler::new(&path);
    restarted.init();
    assert_eq!(
        restarted.get_reactions(),
        vec![
            (
                "message-1".into(),
                String::from("alice@example.test").into(),
                "❤️".into()
            ),
            (
                "message-1".into(),
                String::from("bob@example.test").into(),
                "😂".into()
            ),
        ]
    );

    restarted.record_reaction(
        &"message-1".into(),
        String::from("alice@example.test").into(),
        "".into(),
    );
    assert_eq!(
        restarted.get_reactions(),
        vec![(
            "message-1".into(),
            String::from("bob@example.test").into(),
            "😂".into()
        )]
    );
    restarted.stop();
}

#[test]
fn reactions_are_retained_when_the_target_message_is_not_stored_yet() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("out-of-order.db");
    let mut database = DatabaseHandler::new(&path);
    database.init();

    database.record_reaction(
        &"message-arrives-later".into(),
        String::from("alice@example.test").into(),
        "🙏".into(),
    );

    assert_eq!(
        database.get_reactions(),
        vec![(
            "message-arrives-later".into(),
            String::from("alice@example.test").into(),
            "🙏".into(),
        )]
    );
    database.stop();
}

#[test]
fn startup_hydrates_reactions_without_rewriting_rows() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("startup.db");
    let mut database = DatabaseHandler::new(&path);
    database.init();
    database.record_reaction(
        &"stored-message".into(),
        String::from("alice@example.test").into(),
        "❤️".into(),
    );
    database.stop();

    let mut app = app_with_database(&path);
    app.load_data_from_db();
    app.load_data_from_db();

    assert_eq!(
        app.reactions
            .get("stored-message")
            .and_then(|reactions| reactions.get(&String::from("alice@example.test").into()))
            .map(|emoji| emoji.as_ref()),
        Some("❤️")
    );
    assert_eq!(app.db_handler.get_reactions().len(), 1);
    app.db_handler.stop();
}

#[test]
fn duplicate_reaction_events_are_idempotent_in_memory_and_storage() {
    let dir = tempdir().unwrap();
    let mut app = app_with_database(&dir.path().join("echo.db"));
    for _ in 0..2 {
        app.apply_reaction(
            &"message-echo".into(),
            String::from("alice@example.test").into(),
            "👍".into(),
        );
    }

    assert_eq!(app.reactions.get("message-echo").unwrap().len(), 1);
    assert_eq!(app.db_handler.get_reactions().len(), 1);
    app.db_handler.stop();
}

#[test]
fn reaction_chips_are_deterministic_and_count_participants() {
    let mut reactions = HashMap::new();
    reactions.insert(String::from("bob@example.test").into(), "😂".into());
    reactions.insert(String::from("alice@example.test").into(), "👍".into());
    reactions.insert(String::from("carol@example.test").into(), "👍".into());

    assert_eq!(
        reaction_chips(Some(&reactions)),
        vec!["[👍 2]".to_owned(), "[😂 1]".to_owned()]
    );
    assert!(reaction_chips(None).is_empty());
}

fn app_with_database(path: &Path) -> TestApp {
    TestApp::with_database(path)
}
