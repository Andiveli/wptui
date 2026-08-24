use flate2::read::GzDecoder;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use tar::Archive;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallClassification {
    Standalone,
    PackageManaged,
    Unsafe,
}

pub fn classify_installation(path: &Path) -> io::Result<InstallClassification> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Ok(InstallClassification::Unsafe);
    }
    if is_managed_layout(path) {
        return Ok(InstallClassification::PackageManaged);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let parent_writable = path
            .parent()
            .and_then(|parent| fs::metadata(parent).ok())
            .is_some_and(|parent| {
                parent.uid() == unsafe { libc::geteuid() }
                    && parent.permissions().mode() & 0o222 != 0
            });
        return Ok(
            if metadata.uid() == unsafe { libc::geteuid() }
                && metadata.permissions().mode() & 0o111 != 0
                && parent_writable
            {
                InstallClassification::Standalone
            } else {
                InstallClassification::Unsafe
            },
        );
    }
    #[cfg(not(unix))]
    {
        Ok(InstallClassification::Unsafe)
    }
}

/// Returns true for paths whose ownership is controlled by a package manager.
///
/// This deliberately does not classify every path below `$HOME/.local`: the
/// official installer places standalone binaries in `$HOME/.local/bin`.
fn is_managed_layout(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let home_prefix = |suffix: &str| {
        home.as_ref()
            .map(|home| home.join(suffix))
            .is_some_and(|prefix| path.starts_with(prefix))
    };

    // Nix profiles and store paths can be user-owned and writable, but are
    // still replaced by Nix rather than by this updater.
    if path.starts_with("/nix/store")
        || path.starts_with("/nix/var/nix/profiles")
        || path.starts_with("/etc/profiles")
        || home_prefix(".nix-profile")
        || home_prefix(".local/state/nix/profiles")
    {
        return true;
    }

    // Homebrew/Linuxbrew prefixes, including per-user installations.
    if path.starts_with("/opt/homebrew")
        || path.starts_with("/home/linuxbrew/.linuxbrew")
        || path.starts_with("/usr/local/Cellar")
        || path.starts_with("/usr/local/Homebrew")
        || home_prefix(".linuxbrew")
        || home_prefix(".homebrew")
    {
        return true;
    }

    // Other recognizable package/version-manager destinations. The generic
    // system prefixes below remain conservative; `$HOME/.local/bin` does not.
    if home_prefix(".cargo/bin")
        || home_prefix(".asdf/installs")
        || home_prefix(".local/share/mise/installs")
        || path.starts_with("/snap")
        || path.starts_with("/var/lib/flatpak")
        || path.starts_with("/app")
        || ["/usr/", "/bin/", "/sbin/", "/opt/"]
            .iter()
            .any(|prefix| text.starts_with(prefix))
    {
        return true;
    }

    false
}

pub struct StandaloneInstaller;

impl super::InstallerPort for StandaloneInstaller {
    fn install(&mut self, current_exe: &Path, archive: &[u8], digest: &str) -> Result<(), String> {
        super::verify_digest(archive, digest)?;
        let parent = current_exe
            .parent()
            .ok_or("current executable has no parent")?;
        let mut archive = Archive::new(GzDecoder::new(archive));
        let mut entries = archive.entries().map_err(|error| error.to_string())?;
        let mut entry = entries
            .next()
            .ok_or("update archive is empty")?
            .map_err(|error| error.to_string())?;
        if entry.path().map_err(|error| error.to_string())? != PathBuf::from("wp-tui")
            || !entry.header().entry_type().is_file()
        {
            return Err("update archive is unsafe".into());
        }
        let mode = entry.header().mode().map_err(|error| error.to_string())?;
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        io::copy(&mut entry, &mut temp).map_err(|error| error.to_string())?;
        if entries.next().is_some() {
            return Err("update archive is unsafe".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            temp.as_file()
                .set_permissions(fs::Permissions::from_mode(mode | 0o100))
                .map_err(|error| error.to_string())?;
        }
        temp.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        temp.persist(current_exe)
            .map_err(|error| error.error.to_string())?;
        #[cfg(unix)]
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::InstallerPort;
    use sha2::{Digest, Sha256};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut compressed =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut builder = tar::Builder::new(&mut compressed);
            for (name, bytes) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_path(name).unwrap();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                builder.append(&header, *bytes).unwrap();
            }
            builder.finish().unwrap();
        }
        compressed.finish().unwrap()
    }

    #[test]
    fn rejects_invalid_archive_without_replacing_binary() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("wp-tui");
        fs::write(&path, b"old").unwrap();
        let bytes = archive(&[("wp-tui", b"new"), ("extra", b"bad")]);
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let mut installer = StandaloneInstaller;
        assert_eq!(
            installer.install(&path, &bytes, &digest).unwrap_err(),
            "update archive is unsafe"
        );
        assert_eq!(fs::read(path).unwrap(), b"old");
    }

    #[test]
    fn atomically_replaces_binary_and_keeps_it_executable() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("wp-tui");
        fs::write(&path, b"old").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let bytes = archive(&[("wp-tui", b"#!/bin/sh\nprintf 'wp-tui-test-ok\\n'\n")]);
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let mut installer = StandaloneInstaller;
        installer.install(&path, &bytes, &digest).unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            b"#!/bin/sh\nprintf 'wp-tui-test-ok\\n'\n"
        );
        #[cfg(unix)]
        {
            assert_ne!(fs::metadata(&path).unwrap().permissions().mode() & 0o111, 0);
            let output = std::process::Command::new(&path).output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"wp-tui-test-ok\n");
        }
    }

    #[test]
    fn rejects_checksum_mismatch_before_archive_processing() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("wp-tui");
        fs::write(&path, b"old").unwrap();
        let mut installer = StandaloneInstaller;
        assert_eq!(
            installer
                .install(&path, b"not-an-archive", &"00".repeat(32))
                .unwrap_err(),
            "download checksum mismatch"
        );
        assert_eq!(fs::read(path).unwrap(), b"old");
    }

    #[test]
    fn rejects_missing_wrong_and_traversal_archive_members() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("wp-tui");
        fs::write(&path, b"old").unwrap();
        for (bytes, expected) in [
            (archive(&[]), "update archive is empty"),
            (archive(&[("other", b"new")]), "update archive is unsafe"),
            (symlink_archive(), "update archive is unsafe"),
        ] {
            let digest = format!("{:x}", Sha256::digest(&bytes));
            let mut installer = StandaloneInstaller;
            assert_eq!(
                installer.install(&path, &bytes, &digest).unwrap_err(),
                expected
            );
            assert_eq!(fs::read(&path).unwrap(), b"old");
        }
    }

    #[test]
    fn rejects_symlink_archive_member() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("wp-tui");
        fs::write(&path, b"old").unwrap();
        let bytes = symlink_archive();
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let mut installer = StandaloneInstaller;
        assert_eq!(
            installer.install(&path, &bytes, &digest).unwrap_err(),
            "update archive is unsafe"
        );
        assert_eq!(fs::read(path).unwrap(), b"old");
    }

    #[test]
    fn official_standalone_is_allowed_but_managed_layouts_are_rejected() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("wp-tui");
        fs::write(&path, b"old").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(
            classify_installation(&path).unwrap(),
            InstallClassification::Standalone
        );
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
        let managed = vec![
            PathBuf::from("/home/linuxbrew/.linuxbrew/bin/wp-tui"),
            PathBuf::from("/opt/homebrew/bin/wp-tui"),
            PathBuf::from("/nix/store/abc-wp-tui/bin/wp-tui"),
            home.join(".cargo/bin/wp-tui"),
            home.join(".asdf/installs/wp-tui/1.0.0/bin/wp-tui"),
            home.join(".local/share/mise/installs/wp-tui/1.0.0/bin/wp-tui"),
            PathBuf::from("/snap/wp-tui/current/wp-tui"),
            PathBuf::from("/var/lib/flatpak/app/org.example.WpTui/current/active/files/bin/wp-tui"),
            PathBuf::from("/usr/bin/wp-tui"),
            PathBuf::from("/bin/wp-tui"),
            PathBuf::from("/sbin/wp-tui"),
            PathBuf::from("/opt/wp-tui/bin/wp-tui"),
        ];
        for path in managed {
            assert!(is_managed_layout(&path), "{}", path.display());
        }
        assert!(!is_managed_layout(&home.join(".local/bin/wp-tui")));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_current_executable() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().unwrap();
        let target = directory.path().join("real-wp-tui");
        let link = directory.path().join("wp-tui");
        fs::write(&target, b"binary").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert_eq!(
            classify_installation(&link).unwrap(),
            InstallClassification::Unsafe
        );
    }

    fn symlink_archive() -> Vec<u8> {
        let mut compressed =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut builder = tar::Builder::new(&mut compressed);
            let mut header = tar::Header::new_gnu();
            header.set_path("wp-tui").unwrap();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_link_name("../outside").unwrap();
            header.set_size(0);
            header.set_cksum();
            builder.append(&header, &[][..]).unwrap();
            builder.finish().unwrap();
        }
        compressed.finish().unwrap()
    }
}
