use std::time::Duration;

use super::{Asset, Release, ReleasePort};

/// Maximum release archive size accepted by the updater.
///
/// Current Linux/macOS archives are about 14 MiB; 64 MiB leaves room for
/// growth while preserving an explicit allocation bound for remote data.
const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;

pub struct GithubReleaseClient {
    agent: ureq::Agent,
    repository: String,
}

impl GithubReleaseClient {
    pub fn new(repository: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(3)))
            .build();
        Self {
            agent: config.into(),
            repository: repository.into(),
        }
    }
}

#[derive(serde::Deserialize)]
struct ApiRelease {
    tag_name: String,
    prerelease: bool,
    assets: Vec<ApiAsset>,
}
#[derive(serde::Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

impl ReleasePort for GithubReleaseClient {
    fn latest(&mut self) -> Result<Release, String> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            self.repository
        );
        let response = self
            .agent
            .get(&url)
            .header("User-Agent", "wp-tui-update-check")
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|error| error.to_string())?;
        let api: ApiRelease = response
            .into_body()
            .read_json()
            .map_err(|error| error.to_string())?;
        Ok(Release {
            tag: api.tag_name,
            prerelease: api.prerelease,
            assets: api
                .assets
                .into_iter()
                .map(|asset| Asset {
                    name: asset.name,
                    download_url: asset.browser_download_url,
                    digest: asset.digest,
                })
                .collect(),
        })
    }

    fn download(&mut self, asset: &Asset) -> Result<Vec<u8>, String> {
        let mut response = self
            .agent
            .get(&asset.download_url)
            .header("User-Agent", "wp-tui-update")
            .header("Accept", "application/octet-stream")
            .call()
            .map_err(|error| error.to_string())?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_ASSET_BYTES)
            .read_to_vec()
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_body_limit_accepts_archives_above_ureqs_default_limit() {
        let bytes = vec![0_u8; 10 * 1024 * 1024 + 1];
        let mut body = ureq::Body::builder().data(bytes.clone());

        let received = body
            .with_config()
            .limit(MAX_ASSET_BYTES)
            .read_to_vec()
            .unwrap();

        assert_eq!(received.len(), bytes.len());
    }
}
