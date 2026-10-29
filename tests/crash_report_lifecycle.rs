use std::fs;
use std::panic;
use std::process::Command;

const HELPER_PATH: &str = "WPTUI_CRASH_REPORT_TEST_PATH";
const HELPER_EXISTING: &str = "WPTUI_CRASH_REPORT_TEST_EXISTING";

#[test]
fn installing_reporter_preserves_or_omits_file_until_a_panic() {
    let directory = tempfile::tempdir().unwrap();
    for existing in [false, true] {
        let path = directory.path().join(if existing { "existing.log" } else { "new.log" });
        if existing {
            fs::write(&path, "previous crash report").unwrap();
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crash_report_helper", "--nocapture"])
            .env(HELPER_PATH, &path)
            .env(HELPER_EXISTING, if existing { "1" } else { "0" })
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "helper failed: stdout={:?}, stderr={:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("intentional test panic"));
        let report = fs::read_to_string(&path).unwrap();
        assert!(report.starts_with("wptui crash report\n"));
        assert!(report.contains("panic: intentional test panic"));
    }
}

#[test]
fn crash_report_helper() {
    let Some(path) = std::env::var_os(HELPER_PATH) else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let existing = std::env::var(HELPER_EXISTING).unwrap() == "1";
    wp_tui::crash_diagnostics::install(path.clone());
    if existing {
        assert_eq!(fs::read_to_string(&path).unwrap(), "previous crash report");
    } else {
        assert!(!path.exists(), "install created an empty crash report");
    }
    assert!(panic::catch_unwind(|| panic!("intentional test panic")).is_err());
}
