use super::*;
use crate::updater::{Asset, Release, sha256_hex};

struct Source {
    release: Release,
    draft: bool,
    tags: Vec<String>,
    downloads: usize,
}
impl SelectedReleasePort for Source {
    fn selected(&mut self, tag: &str) -> Result<(Release, bool), String> {
        self.tags.push(tag.into());
        Ok((self.release.clone(), self.draft))
    }
    fn download_selected(&mut self, _: &Asset) -> Result<Vec<u8>, String> {
        self.downloads += 1;
        Ok(b"archive".to_vec())
    }
}
struct Installer {
    classification: InstallClassification,
    calls: usize,
    data: Vec<u8>,
}
impl InstallerPort for Installer {
    fn install(&mut self, _: &Path, bytes: &[u8], _: &str) -> Result<(), String> {
        self.calls += 1;
        self.data = bytes.to_vec();
        Ok(())
    }
}
impl SelectedInstallerPort for Installer {
    fn classify(&self, _: &Path) -> Result<InstallClassification, String> {
        Ok(self.classification)
    }
}
fn fixture() -> (UpdateTarget, Source, Installer) {
    (
        UpdateTarget {
            tag: "v99.1.0".into(),
            version: Version::new(99, 1, 0),
        },
        Source {
            release: Release {
                tag: "v99.1.0".into(),
                prerelease: false,
                assets: vec![Asset {
                    name: target_asset_name().unwrap().into(),
                    download_url: "unused".into(),
                    digest: Some(format!("sha256:{}", sha256_hex(b"archive"))),
                }],
            },
            draft: false,
            tags: vec![],
            downloads: 0,
        },
        Installer {
            classification: InstallClassification::Standalone,
            calls: 0,
            data: b"old".to_vec(),
        },
    )
}
#[test]
fn installs_exact_displayed_intermediate_version_and_returns_bound_path() {
    let (target, mut source, mut installer) = fixture();
    let path = Path::new("/bound/wp-tui");
    assert_eq!(
        install_selected(&target, path, &mut source, &mut installer).unwrap(),
        path
    );
    assert_eq!(source.tags, ["v99.1.0"]);
    assert_eq!(source.downloads, 1);
    assert_eq!(installer.calls, 1);
    assert_eq!(installer.data, b"archive");
}
#[test]
fn rejects_untrusted_release_and_installation_without_download_or_mutation() {
    for case in [
        "prerelease",
        "draft",
        "mismatch",
        "version",
        "digest",
        "managed",
        "unsafe",
        "old",
        "beta",
        "tag",
    ] {
        let (mut target, mut source, mut installer) = fixture();
        match case {
            "prerelease" => source.release.prerelease = true,
            "draft" => source.draft = true,
            "mismatch" => source.release.tag = "v99.2.0".into(),
            "version" => target.version = Version::new(99, 2, 0),
            "digest" => source.release.assets[0].digest = None,
            "managed" => installer.classification = InstallClassification::PackageManaged,
            "unsafe" => installer.classification = InstallClassification::Unsafe,
            "old" => {
                target.tag = "v0.0.1".into();
                target.version = Version::new(0, 0, 1);
            }
            "beta" => {
                target.tag = "v99.1.0-rc1".into();
                target.version = Version::parse("99.1.0-rc1").unwrap();
            }
            "tag" => target.tag = "../../main".into(),
            _ => unreachable!(),
        }
        assert!(
            install_selected(
                &target,
                Path::new("/bound/wp-tui"),
                &mut source,
                &mut installer
            )
            .is_err(),
            "{case}"
        );
        assert_eq!(source.downloads, 0, "{case}");
        assert_eq!(installer.calls, 0, "{case}");
        assert_eq!(installer.data, b"old", "{case}");
    }
}
#[test]
fn checksum_mismatch_rejects_download_before_installer_mutation() {
    let (target, mut source, mut installer) = fixture();
    source.release.assets[0].digest = Some(format!("sha256:{}", "00".repeat(32)));
    assert_eq!(
        install_selected(
            &target,
            Path::new("/bound/wp-tui"),
            &mut source,
            &mut installer
        )
        .unwrap_err(),
        "download checksum mismatch"
    );
    assert_eq!(source.tags, ["v99.1.0"]);
    assert_eq!(source.downloads, 1);
    assert_eq!(installer.calls, 0);
    assert_eq!(installer.data, b"old");
}

#[test]
fn latest_cli_policy_still_accepts_newer_stable_and_ignores_beta() {
    struct Latest(Release);
    impl crate::updater::ReleasePort for Latest {
        fn latest(&mut self) -> Result<Release, String> {
            Ok(self.0.clone())
        }
        fn download(&mut self, _: &Asset) -> Result<Vec<u8>, String> {
            panic!("not a download check")
        }
    }
    let (_, source, _) = fixture();
    let mut latest = Latest(source.release);
    assert_eq!(
        crate::updater::latest_update(&mut latest)
            .unwrap()
            .unwrap()
            .version,
        Version::new(99, 1, 0)
    );
    latest.0.prerelease = true;
    assert!(
        crate::updater::latest_update(&mut latest)
            .unwrap()
            .is_none()
    );
}
