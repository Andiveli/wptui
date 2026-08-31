use std::{
    fs,
    path::{Path, PathBuf},
};

const EXECUTOR: &str = "CommandLaunchExecutor";
const APPROVED_BOOTSTRAP_CONSTRUCTION: &str = "Box::new(crate::media::CommandLaunchExecutor)";

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
