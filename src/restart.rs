//! Relaunch only after the caller has restored the terminal and dropped App/storage.
use std::{ffi::OsString, path::Path};

pub trait RelaunchPort {
    fn relaunch(&mut self, executable: &Path, args: &[OsString]) -> Result<(), String>;
}

pub struct SystemRelaunch;

impl RelaunchPort for SystemRelaunch {
    fn relaunch(&mut self, executable: &Path, args: &[OsString]) -> Result<(), String> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // No shell, no environment clearing, no current_exe lookup after replacement.
            let error = std::process::Command::new(executable).args(args).exec();
            Err(error.to_string())
        }
        #[cfg(not(unix))]
        {
            let _ = (executable, args);
            Err("restart is only supported on Unix".into())
        }
    }
}

pub fn relaunch(
    executable: &Path,
    args: &[OsString],
    port: &mut impl RelaunchPort,
) -> Result<(), String> {
    port.relaunch(executable, args).map_err(|error| format!(
        "Could not restart {}: {error}. Run this installed binary manually with your previous arguments. Your login/session data has not been removed.",
        executable.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_relauncher_uses_installed_path_and_preserves_all_arguments() {
        struct Recording(Vec<(std::path::PathBuf, Vec<OsString>)>);
        impl RelaunchPort for Recording {
            fn relaunch(&mut self, path: &Path, args: &[OsString]) -> Result<(), String> {
                self.0.push((path.to_path_buf(), args.to_vec()));
                Err("permission denied".into())
            }
        }
        let path = Path::new("/installed/new/wp-tui");
        let args = [
            "--phone",
            "+1234567890",
            "--no-view",
            "--crash-log",
            "log with spaces",
        ]
        .map(OsString::from);
        let mut port = Recording(vec![]);
        let error = relaunch(path, &args, &mut port).unwrap_err();
        assert_eq!(port.0, [(path.to_path_buf(), args.to_vec())]);
        assert!(error.contains("permission denied"));
        assert!(error.contains("Run this installed binary manually"));
        assert!(error.contains("login/session data has not been removed"));
    }
}
