use std::{
    fs,
    path::{Path, PathBuf},
};

const EXECUTOR: &str = "CommandLaunchExecutor";
const APPROVED_BOOTSTRAP_CONSTRUCTION: &str = "Box::new(crate::media::CommandLaunchExecutor)";
const SQLITE_CHAT_STORE_HYDRATION: &str = "SqliteChatStoreHydration";
const APPROVED_HYDRATION_CONSTRUCTION: &str =
    "Box::new(crate::db::SqliteChatStoreHydration::new(&db_path))";
// This explicit allowlist covers every known test factory that replaces
// `DatabaseHandler`; update it when adding another such factory.
const TEST_DATABASE_FACTORY_ALLOWLIST: [(&str, &str, &str); 4] = [
    (
        "src/app/test_support.rs",
        "DatabaseHandler::new(&db_path)",
        "SqliteChatStoreHydration::new(&db_path)",
    ),
    (
        "tests/common/mod.rs",
        "DatabaseHandler::new(path)",
        "SqliteChatStoreHydration::new(path)",
    ),
    (
        "tests/message_actions.rs",
        "DatabaseHandler::new(&db_path)",
        "SqliteChatStoreHydration::new(&db_path)",
    ),
    (
        "tests/media_cleanup.rs",
        "DatabaseHandler::new(&db_path)",
        "SqliteChatStoreHydration::new(&db_path)",
    ),
];
const HYDRATION_DIRECT_READS: [&str; 4] = [
    "db_handler.get_chats",
    "db_handler.get_contacts",
    "db_handler.get_messages",
    "db_handler.get_reactions",
];

#[test]
fn command_launch_executor_is_constructed_only_by_app_bootstrap() {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let media = project_root.join("src/media.rs");
    let bootstrap = project_root.join("src/app/bootstrap.rs");
    let bootstrap_source = fs::read_to_string(&bootstrap).unwrap();

    assert_eq!(
        bootstrap_source.match_indices(EXECUTOR).count(),
        1,
        "src/app/bootstrap.rs must not import or alias CommandLaunchExecutor"
    );
    assert_eq!(
        bootstrap_source
            .match_indices(APPROVED_BOOTSTRAP_CONSTRUCTION)
            .count(),
        1,
        "src/app/bootstrap.rs must contain the one approved CommandLaunchExecutor construction"
    );

    for source_path in rust_sources(&project_root.join("src")) {
        if source_path == media || source_path == bootstrap {
            continue;
        }

        let source = fs::read_to_string(&source_path).unwrap();
        assert!(
            !source.contains(EXECUTOR),
            "{} must depend on LaunchExecutor, not import or construct CommandLaunchExecutor",
            source_path.strip_prefix(project_root).unwrap().display(),
        );
    }
}

#[test]
fn hydration_uses_a_port_for_chat_store_reads() {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let hydration = project_root.join("src/app/chat_store/hydration.rs");
    let source = fs::read_to_string(&hydration).unwrap();

    for direct_read in HYDRATION_DIRECT_READS {
        assert!(
            !source.contains(direct_read),
            "src/app/chat_store/hydration.rs must read chat-store data through ChatStoreHydrationPort, not {direct_read}"
        );
    }
}

#[test]
fn allowlisted_database_handler_replacement_factories_use_matching_hydration_paths() {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));

    for (relative_path, handler_construction, hydration_construction) in
        TEST_DATABASE_FACTORY_ALLOWLIST
    {
        let source_path = project_root.join(relative_path);
        let source = fs::read_to_string(&source_path).unwrap();

        assert!(
            source.contains(handler_construction),
            "allowlisted factory {relative_path} must construct DatabaseHandler with its requested database path"
        );
        assert!(
            source.contains(hydration_construction),
            "allowlisted factory {relative_path} must construct SqliteChatStoreHydration with the same requested database path"
        );
    }
}

#[test]
fn sqlite_chat_store_hydration_is_constructed_only_by_bootstrap_or_test_factories() {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bootstrap = project_root.join("src/app/bootstrap.rs");
    let test_support = project_root.join("src/app/test_support.rs");
    let adapter = project_root.join("src/db/chat_store_hydration.rs");
    let bootstrap_source = fs::read_to_string(&bootstrap).unwrap();

    assert_eq!(
        bootstrap_source
            .match_indices(SQLITE_CHAT_STORE_HYDRATION)
            .count(),
        1,
        "src/app/bootstrap.rs must contain the one approved SqliteChatStoreHydration construction"
    );
    assert_eq!(
        bootstrap_source
            .match_indices(APPROVED_HYDRATION_CONSTRUCTION)
            .count(),
        1,
        "src/app/bootstrap.rs must contain the exact approved SqliteChatStoreHydration construction"
    );

    for source_path in rust_sources(&project_root.join("src")) {
        if source_path == bootstrap || source_path == test_support || source_path == adapter {
            continue;
        }

        let source = fs::read_to_string(&source_path).unwrap();
        assert!(
            !source.contains("SqliteChatStoreHydration::new")
                && !source.contains("SqliteChatStoreHydration {"),
            "{} must receive ChatStoreHydrationPort rather than construct SqliteChatStoreHydration",
            source_path.strip_prefix(project_root).unwrap().display(),
        );
    }
}

fn rust_sources(directory: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    collect_rust_sources(directory, &mut sources);
    sources
}

fn collect_rust_sources(directory: &Path, sources: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rust_sources(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}
