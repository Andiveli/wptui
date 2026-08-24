mod github;
mod install;

use std::path::Path;

use semver::Version;
use sha2::{Digest, Sha256};

pub use github::GithubReleaseClient;
pub use install::{InstallClassification, StandaloneInstaller, classify_installation};

const REPOSITORY: &str = "Andiveli/wptui";

pub trait ReleasePort {
    fn latest(&mut self) -> Result<Release, String>;
    fn download(&mut self, asset: &Asset) -> Result<Vec<u8>, String>;
}

pub trait InstallerPort {
    fn install(&mut self, current_exe: &Path, archive: &[u8], digest: &str) -> Result<(), String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Release {
    pub tag: String,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Asset {
    pub name: String,
    pub download_url: String,
    pub digest: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateNotice {
    pub version: Version,
}

pub fn latest_update<R: ReleasePort>(release: &mut R) -> Result<Option<UpdateNotice>, String> {
    let latest = release.latest()?;
    if latest.prerelease {
        return Ok(None);
    }
    let version = stable_version(&latest.tag)?;
    if !version.pre.is_empty() {
        return Ok(None);
    }
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())?;
    Ok((version > current).then_some(UpdateNotice { version }))
}

pub fn startup_check(tx: std::sync::mpsc::Sender<crate::app::events::AppInput>) {
    std::thread::spawn(move || {
        let mut client = GithubReleaseClient::new(REPOSITORY);
        if let Ok(Some(notice)) = latest_update(&mut client) {
            let _ = tx.send(crate::app::events::AppInput::App(
                crate::app::events::AppEvent::UpdateAvailable(notice.version.to_string()),
            ));
        }
    });
}

pub fn run_explicit_update() -> Result<bool, String> {
    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut release = GithubReleaseClient::new(REPOSITORY);
    let latest = release.latest()?;
    if latest.prerelease {
        return Ok(false);
    }
    let version = stable_version(&latest.tag)?;
    if !version.pre.is_empty() {
        return Ok(false);
    }
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())?;
    if version <= current {
        return Ok(false);
    }
    if classify_installation(&current_exe).map_err(|error| error.to_string())?
        != InstallClassification::Standalone
    {
        return Err("this installation is managed by a package manager".into());
    }
    let asset_name = target_asset_name().ok_or("this platform is unsupported")?;
    let asset = latest
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or("the latest release has no compatible asset")?;
    let digest =
        valid_digest(asset.digest.as_deref()).ok_or("the release has no valid SHA-256 digest")?;
    let archive = release.download(asset)?;
    verify_digest(&archive, digest)?;
    StandaloneInstaller.install(&current_exe, &archive, digest)?;
    Ok(true)
}

pub fn stable_version(tag: &str) -> Result<Version, String> {
    Version::parse(tag.trim_start_matches('v'))
        .map_err(|error| format!("invalid release version: {error}"))
}

pub fn target_asset_name_for(arch: &str, os: &str) -> Option<&'static str> {
    match (arch, os) {
        ("x86_64", "linux") => Some("wptui-x86_64-unknown-linux-gnu.tar.gz"),
        ("aarch64", "macos") => Some("wptui-aarch64-apple-darwin.tar.gz"),
        _ => None,
    }
}

fn target_asset_name() -> Option<&'static str> {
    target_asset_name_for(std::env::consts::ARCH, std::env::consts::OS)
}

fn valid_digest(value: Option<&str>) -> Option<&str> {
    let digest = value?.strip_prefix("sha256:")?;
    (digest.len() == 64
        && digest
            .chars()
            .all(|character| character.is_ascii_hexdigit()))
    .then_some(digest)
}

fn verify_digest(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    (actual.eq_ignore_ascii_case(expected))
        .then_some(())
        .ok_or_else(|| "download checksum mismatch".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeRelease(Release);
    impl ReleasePort for FakeRelease {
        fn latest(&mut self) -> Result<Release, String> {
            Ok(self.0.clone())
        }
        fn download(&mut self, _: &Asset) -> Result<Vec<u8>, String> {
            unreachable!()
        }
    }

    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            tag: tag.into(),
            prerelease,
            assets: vec![],
        }
    }

    #[test]
    fn stable_policy_ignores_prereleases_and_older_versions() {
        assert!(
            latest_update(&mut FakeRelease(release("v99.0.0-rc1", false)))
                .unwrap()
                .is_none()
        );
        assert!(
            latest_update(&mut FakeRelease(release("v0.1.0", false)))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn stable_policy_detects_newer_release() {
        assert_eq!(
            latest_update(&mut FakeRelease(release("v99.0.0", false)))
                .unwrap()
                .unwrap()
                .version,
            Version::new(99, 0, 0)
        );
    }

    #[test]
    fn target_assets_match_published_platforms() {
        assert_eq!(
            target_asset_name_for("x86_64", "linux"),
            Some("wptui-x86_64-unknown-linux-gnu.tar.gz")
        );
        assert_eq!(
            target_asset_name_for("aarch64", "macos"),
            Some("wptui-aarch64-apple-darwin.tar.gz")
        );
        assert_eq!(target_asset_name_for("aarch64", "linux"), None);
    }
}
