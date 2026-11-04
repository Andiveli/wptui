//! Executable authority is obtained afresh from the trusted release API, never the news cache.
use std::path::{Path, PathBuf};

use semver::Version;

use super::{
    Asset, GithubReleaseClient, InstallClassification, InstallerPort, REPOSITORY, Release,
    StandaloneInstaller, classify_installation, stable_version, target_asset_name, valid_digest,
    verify_digest,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateTarget {
    pub tag: String,
    pub version: Version,
}

impl UpdateTarget {
    pub fn validate(&self) -> Result<(), String> {
        // Only a canonical stable semver tag is allowed as an API path component.
        if (self.tag != self.version.to_string() && self.tag != format!("v{}", self.version))
            || !self.version.pre.is_empty()
            || stable_version(&self.tag)? != self.version
            || self.version
                <= Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?
        {
            return Err("select a stable release newer than the running version".into());
        }
        Ok(())
    }
}

pub trait SelectedReleasePort {
    fn selected(&mut self, tag: &str) -> Result<(Release, bool), String>;
    fn download_selected(&mut self, asset: &Asset) -> Result<Vec<u8>, String>;
}

pub trait SelectedInstallerPort: InstallerPort {
    fn classify(&self, path: &Path) -> Result<InstallClassification, String>;
}
impl SelectedInstallerPort for StandaloneInstaller {
    fn classify(&self, path: &Path) -> Result<InstallClassification, String> {
        classify_installation(path).map_err(|e| e.to_string())
    }
}

pub fn install_selected(
    target: &UpdateTarget,
    executable: &Path,
    release: &mut impl SelectedReleasePort,
    installer: &mut impl SelectedInstallerPort,
) -> Result<PathBuf, String> {
    target.validate()?;
    if installer.classify(executable)? != InstallClassification::Standalone {
        return Err(
            "this installation is package-managed or unsafe; use its package manager".into(),
        );
    }
    let asset_name = target_asset_name().ok_or("this platform is unsupported")?;
    let (selected, draft) = release.selected(&target.tag)?;
    if draft
        || selected.prerelease
        || selected.tag != target.tag
        || stable_version(&selected.tag)? != target.version
    {
        return Err("the selected release is not the requested published stable version".into());
    }
    let asset = selected
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or("the selected release has no compatible asset")?;
    let digest =
        valid_digest(asset.digest.as_deref()).ok_or("the release has no valid SHA-256 digest")?;
    let archive = release.download_selected(asset)?;
    verify_digest(&archive, digest)?;
    installer.install(executable, &archive, digest)?;
    Ok(executable.to_path_buf())
}

pub fn run_selected_update(target: &UpdateTarget) -> Result<PathBuf, String> {
    // Capture once BEFORE replacement. On Linux current_exe after rename points
    // to the deleted old inode, not the newly installed executable.
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    install_selected(
        target,
        &executable,
        &mut GithubReleaseClient::new(REPOSITORY),
        &mut StandaloneInstaller,
    )
}

#[cfg(test)]
#[path = "news_update_tests.rs"]
mod tests;
