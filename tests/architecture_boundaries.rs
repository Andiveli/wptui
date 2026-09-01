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
const TEST_DATABASE_FACTORY_ALLOWLIST: [(&str, &str, &str, &str); 4] = [
    (
        "src/app/test_support.rs",
        "DatabaseHandler::new(&db_path)",
        "SqliteChatStoreHydration::new(&db_path)",
        "app.chat_store_write = Box::new(db_handler.chat_store_writer())",
    ),
    (
        "tests/common/mod.rs",
        "DatabaseHandler::new(path)",
        "SqliteChatStoreHydration::new(path)",
        "app.set_chat_store_write(Box::new(db_handler.chat_store_writer()))",
    ),
    (
        "tests/message_actions.rs",
        "DatabaseHandler::new(&db_path)",
        "SqliteChatStoreHydration::new(&db_path)",
        "app.set_chat_store_write(Box::new(db_handler.chat_store_writer()))",
    ),
    (
        "tests/media_cleanup.rs",
        "DatabaseHandler::new(&db_path)",
        "SqliteChatStoreHydration::new(&db_path)",
        "app.set_chat_store_write(Box::new(db_handler.chat_store_writer()))",
    ),
];
const HYDRATION_MODULES: [&str; 3] = [
    "src/app/chat_store/hydration.rs",
    "src/app/chat_store/hydration_port.rs",
    "src/db/chat_store_hydration.rs",
];
const CHAT_STORE_WRITER_SOURCES: [&str; 7] = [
    "src/db.rs",
    "src/db/chat_store_writer.rs",
    "src/app/bootstrap.rs",
    "src/app/test_support.rs",
    "tests/common/mod.rs",
    "tests/message_actions.rs",
    "tests/media_cleanup.rs",
];
const WRITER_TOKENS: [&str; 2] = ["SqliteChatStoreWriter", "chat_store_writer()"];
const REACTION_WRITER_FACTORIES: [(&str, &str); 5] = [
    (
        "src/app/bootstrap.rs",
        "SqliteMessageReactionWriter::new(&db_path)",
    ),
    (
        "src/app/test_support.rs",
        "SqliteMessageReactionWriter::new(&db_path)",
    ),
    (
        "tests/common/mod.rs",
        "SqliteMessageReactionWriter::new(path)",
    ),
    (
        "tests/message_actions.rs",
        "SqliteMessageReactionWriter::new(&db_path)",
    ),
    (
        "tests/media_cleanup.rs",
        "SqliteMessageReactionWriter::new(&db_path)",
    ),
];
const HYDRATION_DIRECT_READS: [&str; 4] = [
    "db_handler.get_chats",
    "db_handler.get_contacts",
    "db_handler.get_messages",
    "db_handler.get_reactions",
];
const CONTACT_WRITER_FACTORIES: [(&str, &str); 5] = [
    ("src/app/bootstrap.rs", "SqliteContactWriter::new(&db_path)"),
    (
        "src/app/test_support.rs",
        "SqliteContactWriter::new(&db_path)",
    ),
    ("tests/common/mod.rs", "SqliteContactWriter::new(path)"),
    (
        "tests/message_actions.rs",
        "SqliteContactWriter::new(&db_path)",
    ),
    (
        "tests/media_cleanup.rs",
        "SqliteContactWriter::new(&db_path)",
    ),
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

    for (relative_path, handler_construction, hydration_construction, writer_rebinding) in
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
        assert!(
            source.contains(writer_rebinding),
            "allowlisted factory {relative_path} must rebind chat_store_write from its replacement DatabaseHandler"
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

#[test]
fn message_and_chat_writes_use_the_port_while_legacy_actions_remain_compatible() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ingestion = fs::read_to_string(root.join("src/app/message_ingestion.rs")).unwrap();
    let events = fs::read_to_string(root.join("src/app/whatsapp_events.rs")).unwrap();
    assert!(!ingestion.contains("db_handler.add_message"));
    assert!(ingestion.contains("app.chat_store_write") && ingestion.contains("PersistChatMessage"));
    assert!(!events.contains("db_handler.add_chat"));
    assert!(events.contains("chat_store_write.persist_chat("));
    assert!(
        fs::read_to_string(root.join("src/db.rs"))
            .unwrap()
            .contains("pub fn add_chat")
    );
}

#[test]
fn receipt_message_writeback_uses_the_chat_store_port_only() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let receipts: String = fs::read_to_string(root.join("src/app/chat_store/receipts.rs"))
        .unwrap()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    assert!(!receipts.contains("db_handler.add_message"));
    assert!(receipts.contains("chat_store_write.persist_message("));
    assert!(
        fs::read_to_string(root.join("src/app/message_actions.rs"))
            .unwrap()
            .contains("db_handler.add_message")
    );
    assert!(
        fs::read_to_string(root.join("src/app/whatsapp_events.rs"))
            .unwrap()
            .contains("Event::Chat")
    );
}

#[test]
fn chat_store_writer_stays_at_its_adapter_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let allowed: Vec<_> = CHAT_STORE_WRITER_SOURCES
        .iter()
        .map(|path| root.join(path))
        .collect();
    for path in rust_sources(&root.join("src"))
        .into_iter()
        .chain(rust_sources(&root.join("tests")))
    {
        if !allowed.contains(&path)
            && path != root.join("tests/architecture_boundaries.rs")
            && path != root.join("tests/receipt_message_persistence.rs")
            && path != root.join("tests/chat_event_persistence.rs")
        {
            assert!(
                WRITER_TOKENS
                    .iter()
                    .all(|token| !fs::read_to_string(&path).unwrap().contains(token)),
                "{} must not construct or request SqliteChatStoreWriter",
                path.strip_prefix(root).unwrap().display()
            );
        }
    }
    let adapter = fs::read_to_string(root.join("src/db/chat_store_writer.rs")).unwrap();
    assert!(
        ["Connection", "Worker::new", "DatabaseHandler::new"]
            .iter()
            .all(|token| !adapter.contains(token))
    );
}

#[test]
fn reaction_persistence_stays_at_its_port_and_path_adapter_boundaries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ingestion = fs::read_to_string(root.join("src/app/message_ingestion.rs")).unwrap();
    let ingestion_compact: String = ingestion
        .chars()
        .filter(|char| !char.is_whitespace())
        .collect();
    assert!(!ingestion_compact.contains("db_handler.record_reaction"));
    assert!(
        ingestion_compact.contains("message_reaction_write.record(")
            && ingestion_compact.contains("RecordMessageReaction")
    );

    let allowed: Vec<_> = REACTION_WRITER_FACTORIES
        .iter()
        .map(|(path, _)| root.join(path))
        .collect();
    for path in rust_sources(&root.join("src"))
        .into_iter()
        .chain(rust_sources(&root.join("tests")))
    {
        if path != root.join("tests/architecture_boundaries.rs") && !allowed.contains(&path) {
            assert!(
                !fs::read_to_string(&path)
                    .unwrap()
                    .contains("SqliteMessageReactionWriter::new")
            );
        }
    }
    for (path, construction) in REACTION_WRITER_FACTORIES {
        let source = fs::read_to_string(root.join(path)).unwrap();
        let compact: String = source
            .chars()
            .filter(|char| !char.is_whitespace())
            .collect();
        assert!(
            compact.contains(construction),
            "{path} must construct SqliteMessageReactionWriter with its factory database path"
        );
    }

    let adapter = fs::read_to_string(root.join("src/db/reaction_writer.rs")).unwrap();
    assert!(
        adapter.contains("db_path: PathBuf")
            && adapter.contains("open_database(&self.db_path)")
            && adapter.contains("reaction_repository::record")
    );
    assert!(
        [
            "Worker",
            "QueueHandle",
            "ChatStoreWritePort",
            "thread::spawn",
            "Connection"
        ]
        .iter()
        .all(|token| !adapter.contains(token))
    );
}

#[test]
fn contact_persistence_routes_through_the_port_and_keeps_a_path_only_adapter() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let hydration: String = fs::read_to_string(root.join("src/app/chat_store/hydration.rs"))
        .unwrap()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let adapter = fs::read_to_string(root.join("src/db/contact_writer.rs")).unwrap();
    assert!(!hydration.contains("db_handler.add_contact"));
    assert!(
        hydration.contains("contact_write")
            && hydration.contains(".persist(")
            && hydration.contains("PersistContact{jid,name}")
    );
    assert!(hydration.contains("wr::get_contacts()"));
    assert!(
        adapter.contains("db_path: PathBuf") && adapter.contains("open_database(&self.db_path)")
    );
    assert!(adapter.contains("chat_store::add_contact"));
    assert!(!adapter.contains("whatsrust") && !adapter.contains("get_contacts"));
    assert!(
        [
            "Worker",
            "QueueHandle",
            "ChatStoreWritePort",
            "thread",
            "Connection"
        ]
        .iter()
        .all(|token| !adapter.contains(token))
    );
}

#[test]
fn contact_writer_construction_stays_at_matching_database_factories() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let allowed: Vec<_> = CONTACT_WRITER_FACTORIES
        .iter()
        .map(|(path, _)| root.join(path))
        .collect();
    for path in rust_sources(&root.join("src"))
        .into_iter()
        .chain(rust_sources(&root.join("tests")))
    {
        assert!(
            allowed.contains(&path)
                || path == root.join("tests/architecture_boundaries.rs")
                || path == root.join("tests/contact_persistence.rs")
                || !fs::read_to_string(&path)
                    .unwrap()
                    .contains("SqliteContactWriter::new")
        );
    }
    for (path, construction) in CONTACT_WRITER_FACTORIES {
        let compact: String = fs::read_to_string(root.join(path))
            .unwrap()
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        assert!(
            compact.contains(construction),
            "{path} must use its database path"
        );
    }
}

#[test]
fn hydration_modules_do_not_write_chat_store_data() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for module in HYDRATION_MODULES {
        let source = fs::read_to_string(root.join(module)).unwrap();
        assert!(
            !source.contains("chat_store_write")
                && !source.contains("PersistChatMessage")
                && !source.contains("message_reaction_write")
                && !source.contains("RecordMessageReaction"),
            "{module} must not write chat-store or reaction data"
        );
    }
}

#[test]
fn status_cursor_stays_behind_its_port_and_path_adapter_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (path, call) in [
        ("src/app/chat_store/hydration.rs", "status_cursor.load()"),
        ("src/app/status_projection.rs", "status_cursor.store("),
        ("src/app/chat_store/receipts.rs", "status_cursor.store("),
    ] {
        let source = fs::read_to_string(root.join(path)).unwrap();
        assert!(source.contains(call), "{path} must use StatusCursorPort");
    }
    for path in rust_sources(&root.join("src/app")) {
        let source = fs::read_to_string(&path).unwrap();
        assert!(
            !source.contains("db_handler.status_last_seen")
                && !source.contains("db_handler.set_status_last_seen"),
            "{} must not bypass StatusCursorPort",
            path.display()
        );
    }
    let allowed = [
        ("src/app/bootstrap.rs", "SqliteStatusCursor::new(&db_path)"),
        (
            "src/app/test_support.rs",
            "SqliteStatusCursor::new(&db_path)",
        ),
        ("tests/common/mod.rs", "SqliteStatusCursor::new(path)"),
        (
            "tests/message_actions.rs",
            "SqliteStatusCursor::new(&db_path)",
        ),
        (
            "tests/media_cleanup.rs",
            "SqliteStatusCursor::new(&db_path)",
        ),
        (
            "tests/status_cursor_persistence.rs",
            "SqliteStatusCursor::new(&path)",
        ),
    ];
    for path in rust_sources(&root.join("src"))
        .into_iter()
        .chain(rust_sources(&root.join("tests")))
    {
        assert!(
            allowed
                .iter()
                .any(|(allowed, _)| path == root.join(allowed))
                || path == root.join("src/db/status_cursor.rs")
                || path == root.join("tests/architecture_boundaries.rs")
                || !fs::read_to_string(&path)
                    .unwrap()
                    .contains("SqliteStatusCursor::new")
        );
    }
    for (path, construction) in allowed {
        let compact: String = fs::read_to_string(root.join(path))
            .unwrap()
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        assert!(
            compact.contains(construction),
            "{path} must use its database path"
        );
    }
    let adapter = fs::read_to_string(root.join("src/db/status_cursor.rs")).unwrap();
    assert!(
        adapter.contains("db_path: PathBuf")
            && adapter.contains("try_open_database(&self.db_path)")
            && adapter.contains("cursor_repository::")
    );
    assert!(
        [
            "Worker",
            "QueueHandle",
            "thread",
            "PendingReceiptRepository",
            "ChatStoreWritePort",
            "Connection",
            "DATABASE_WRITE_LOCK"
        ]
        .iter()
        .all(|token| !adapter.contains(token))
    );
    assert!(
        !fs::read_to_string(root.join("src/app/status_cursor.rs"))
            .unwrap()
            .contains("rusqlite")
    );
}

#[test]
fn chat_read_cursor_stays_behind_its_port_and_path_adapter_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in rust_sources(&root.join("src/app")) {
        let source = fs::read_to_string(&path).unwrap();
        assert!(
            !source.contains("db_handler.read_cursors")
                && !source.contains("db_handler.set_last_read_cursor"),
            "{} must not bypass ChatReadCursorPort",
            path.display()
        );
    }
    for (path, calls) in [
        (
            "src/app/chat_store/read_state.rs",
            ["chat_read_cursor.load()", "chat_read_cursor.store("],
        ),
        (
            "src/app/chat_store/receipts.rs",
            ["chat_read_cursor.store(", "StoreChatReadCursor"],
        ),
    ] {
        let source = fs::read_to_string(root.join(path)).unwrap();
        assert!(
            calls.iter().all(|call| source.contains(call)),
            "{path} must use ChatReadCursorPort"
        );
    }
    let allowed = [
        (
            "src/app/bootstrap.rs",
            "SqliteChatReadCursor::new(&db_path)",
        ),
        (
            "src/app/test_support.rs",
            "SqliteChatReadCursor::new(&db_path)",
        ),
        ("tests/common/mod.rs", "SqliteChatReadCursor::new(path)"),
        (
            "tests/message_actions.rs",
            "SqliteChatReadCursor::new(&db_path)",
        ),
        (
            "tests/media_cleanup.rs",
            "SqliteChatReadCursor::new(&db_path)",
        ),
        (
            "tests/chat_cursor_persistence.rs",
            "SqliteChatReadCursor::new(&path)",
        ),
    ];
    for path in rust_sources(&root.join("src"))
        .into_iter()
        .chain(rust_sources(&root.join("tests")))
    {
        assert!(
            allowed
                .iter()
                .any(|(allowed, _)| path == root.join(allowed))
                || path == root.join("src/db/chat_read_cursor.rs")
                || path == root.join("tests/architecture_boundaries.rs")
                || !fs::read_to_string(&path)
                    .unwrap()
                    .contains("SqliteChatReadCursor::new")
        );
    }
    for (path, construction) in allowed {
        let compact: String = fs::read_to_string(root.join(path))
            .unwrap()
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        assert!(
            compact.contains(construction),
            "{path} must use its database path"
        );
    }
    let adapter = fs::read_to_string(root.join("src/db/chat_read_cursor.rs")).unwrap();
    let adapter_compact: String = adapter
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    assert!(
        adapter_compact.contains("pubstructSqliteChatReadCursor{db_path:PathBuf,}")
            && adapter
                .match_indices("open_database(&self.db_path)")
                .count()
                == 2
            && adapter.contains("cursor_repository::")
    );
    assert!(
        [
            "Connection",
            "Worker",
            "QueueHandle",
            "thread",
            "ReadSyncWorker",
            "PendingReceiptRepository",
            "DATABASE_WRITE_LOCK"
        ]
        .iter()
        .all(|token| !adapter.contains(token))
    );
    let port = fs::read_to_string(root.join("src/app/chat_store/read_cursor_port.rs")).unwrap();
    assert!(!port.contains("rusqlite") && !port.contains("db"));
    let handler: String = fs::read_to_string(root.join("src/db.rs"))
        .unwrap()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    assert!(handler.contains("pubfnset_last_read_cursor(&self,chat:&wr::JID,message_id:Option<wr::MessageId>,timestamp:i64,)") && handler.contains("pubfnread_cursors(&self)->Vec<(wr::JID,wr::MessageId,i64)>"));
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
