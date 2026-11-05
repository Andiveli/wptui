use super::*;

#[test]
fn only_confirmed_http_results_are_known() {
    for (success, response, expected) in [
        (true, "HTTP/2.0 204 No Content\r\n", Some(true)),
        (false, "HTTP/2.0 404 Not Found\r\n", Some(false)),
        (false, "HTTP/2.0 401 Unauthorized\r\n", None),
        (false, "HTTP/2.0 403 Forbidden\r\n", None),
        (false, "HTTP/2.0 500 Server Error\r\n", None),
        (false, "network error: 404", None),
        (true, "HTTP/2.0 200 OK\r\n", None),
        (false, "", None),
    ] {
        assert_eq!(classify(success, response), expected, "{response}");
    }
    assert_eq!(
        Operation::Check.args(),
        [
            "api",
            "--hostname",
            "github.com",
            "--include",
            "user/starred/Andiveli/wptui",
        ]
    );
    assert_eq!(
        Operation::Star.args(),
        [
            "api",
            "--hostname",
            "github.com",
            "--method",
            "PUT",
            "--include",
            "user/starred/Andiveli/wptui",
        ]
    );
}

#[cfg(unix)]
#[test]
fn fake_executable_checks_real_process_adapter_without_environment_mutation() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("fake-gh");
    // Local test executable only; production Command never invokes a shell.
    fs::write(&executable, "#!/bin/sh\n[ \"$*\" = 'api --hostname github.com --include user/starred/Andiveli/wptui' ] || exit 2\nprintf 'HTTP/2.0 404 Not Found\\r\\n'\nexit 1\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let cancel = AtomicBool::new(false);
    assert_eq!(run_gh(&executable, Operation::Check, &cancel), Some(false));
    assert_eq!(
        run_gh(&directory.path().join("missing"), Operation::Check, &cancel),
        None
    );
    fs::write(&executable, "#!/bin/sh\n[ \"$*\" = 'api --hostname github.com --method PUT --include user/starred/Andiveli/wptui' ] || exit 2\nprintf 'HTTP/2.0 204 No Content\\r\\n'\n").unwrap();
    assert_eq!(run_gh(&executable, Operation::Star, &cancel), Some(true));
    fs::write(&executable, "#!/bin/sh\nwhile :; do :; done\n").unwrap();
    let started = Instant::now();
    assert_eq!(
        run_gh_with_timeout(
            &executable,
            Operation::Check,
            &cancel,
            Duration::from_millis(30)
        ),
        None
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(run_gh(&executable, Operation::Star, &cancel), None);
}
