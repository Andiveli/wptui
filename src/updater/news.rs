//! Display-only stable-release history. Nothing in this module authorizes an install.
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use semver::Version;

use super::stable_version;

const MISSING_NOTES: &str = "Release notes were not provided for this version.";
const MAX_PAGE_BYTES: usize = 1024 * 1024;
const MAX_PAGES: u32 = 8;
const PAGE_SIZE: usize = 100;
const MAX_CACHE_BYTES: usize = MAX_PAGES as usize * (MAX_PAGE_BYTES + 1) + 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseNote {
    pub version: Version,
    /// Original, validated stable tag; no asset URLs or executable authority.
    pub tag: String,
    pub title: String,
    pub body: String,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewsHistory {
    pub releases: Vec<ReleaseNote>,
    /// True when a failed request forced a cache fallback (even if cache is absent).
    pub stale: bool,
    /// True if exhaustion was not confirmed, or no usable cache exists on failure.
    pub incomplete: bool,
    /// Fetch error. An empty list with an error never means "up to date".
    pub error: Option<String>,
    pub cache_error: Option<String>,
}

/// Injectable raw JSON page seam. Pages start at 1; an empty array ends history.
/// Implementations must bound their own I/O; the loader also validates byte/count limits.
pub trait NewsPagePort {
    fn page(&mut self, page: u32) -> Result<Vec<u8>, String>;
}

#[derive(Clone, Debug)]
pub struct GithubNewsClient {
    agent: ureq::Agent,
}

impl Default for GithubNewsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GithubNewsClient {
    pub fn new() -> Self {
        Self {
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(3)))
                .build()
                .into(),
        }
    }
}

impl NewsPagePort for GithubNewsClient {
    fn page(&mut self, page: u32) -> Result<Vec<u8>, String> {
        self.agent
            .get(&format!(
                "https://api.github.com/repos/{}/releases?per_page={PAGE_SIZE}&page={page}",
                super::REPOSITORY
            ))
            .header("User-Agent", "wp-tui-news")
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|error| error.to_string())?
            .body_mut()
            .with_config()
            .limit(MAX_PAGE_BYTES as u64)
            .read_to_vec()
            .map_err(|error| error.to_string())
    }
}

pub fn installed_note() -> ReleaseNote {
    let version = Version::parse(env!("CARGO_PKG_VERSION")).expect("valid package version");
    ReleaseNote {
        tag: format!("v{version}"),
        title: format!("Installed version {version}"),
        version,
        body: display_text(include_str!("../../assets/release-notes.md").trim()),
        published_at: None,
    }
}

/// Fetch at most 800 entries / eight requests (3s each for the GitHub port).
/// Cache is a bounded JSON array of raw pages, revalidated on every read. It is
/// display-only and untrusted; only note fields are exposed, never asset metadata.
/// Call off the UI thread. The caller chooses a dedicated cache file path.
pub fn load_history<P: NewsPagePort>(
    port: &mut P,
    installed: &Version,
    cache_path: &Path,
) -> NewsHistory {
    match fetch_pages(port) {
        Ok((pages, raw)) => {
            let (releases, incomplete) = notes(pages, installed);
            let cache_error = write_cache(cache_path, &raw)
                .err()
                .map(|error| display_text(&error.to_string()));
            NewsHistory {
                releases,
                stale: false,
                incomplete,
                error: None,
                cache_error,
            }
        }
        Err(error) => {
            let cached = read_cache(cache_path).ok();
            let (releases, incomplete) = cached
                .map(|pages| notes(pages, installed))
                .unwrap_or_else(|| (vec![], true));
            NewsHistory {
                releases,
                stale: true,
                incomplete,
                error: Some(display_text(&error)),
                cache_error: None,
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct ApiNote {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

type Pages = Vec<Vec<ApiNote>>;

fn parse<T: serde::de::DeserializeOwned>(bytes: Vec<u8>) -> Result<T, String> {
    ureq::Body::builder()
        .data(bytes)
        .read_json()
        .map_err(|error| error.to_string())
}

fn fetch_pages<P: NewsPagePort>(port: &mut P) -> Result<(Pages, Vec<u8>), String> {
    let mut pages = vec![];
    let mut raw = vec![b'['];
    for page in 1..=MAX_PAGES {
        let bytes = port.page(page)?;
        if bytes.len() > MAX_PAGE_BYTES {
            return Err("release history page exceeds size limit".into());
        }
        let entries: Vec<ApiNote> = parse(bytes.clone())?;
        if entries.len() > PAGE_SIZE {
            return Err("release history page exceeds entry limit".into());
        }
        if page > 1 {
            raw.push(b',');
        }
        raw.extend_from_slice(&bytes);
        let exhausted = entries.is_empty();
        pages.push(entries);
        if exhausted {
            break;
        }
    }
    raw.push(b']');
    Ok((pages, raw))
}

fn notes(pages: Pages, installed: &Version) -> (Vec<ReleaseNote>, bool) {
    let incomplete = pages.last().is_none_or(|page| !page.is_empty());
    let mut releases = vec![];
    for entry in pages.into_iter().flatten() {
        if entry.draft || entry.prerelease {
            continue;
        }
        // Validate BEFORE display cleanup: escape-laden tags must never become
        // apparently valid versions after sanitization.
        let Ok(version) = stable_version(&entry.tag_name) else {
            continue;
        };
        if !version.pre.is_empty() || version <= *installed {
            continue;
        }
        let title = display_text(entry.name.as_deref().unwrap_or(&entry.tag_name));
        let body = display_text(entry.body.as_deref().unwrap_or(MISSING_NOTES));
        releases.push(ReleaseNote {
            version,
            tag: display_text(&entry.tag_name),
            title,
            body: if body.trim().is_empty() {
                MISSING_NOTES.into()
            } else {
                body
            },
            published_at: entry.published_at.map(|date| display_text(&date)),
        });
    }
    releases.sort_by(|left, right| right.version.cmp(&left.version));
    releases.dedup_by(|left, right| left.version == right.version);
    (releases, incomplete)
}

fn read_cache(path: &Path) -> Result<Pages, String> {
    let mut bytes = vec![];
    File::open(path)
        .and_then(|file| {
            file.take(MAX_CACHE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_CACHE_BYTES {
        return Err("release notes cache exceeds size limit".into());
    }
    let pages: Pages = parse(bytes)?;
    if pages.is_empty()
        || pages.len() > MAX_PAGES as usize
        || pages.iter().any(|page| page.len() > PAGE_SIZE)
        || pages[..pages.len() - 1].iter().any(Vec::is_empty)
    {
        return Err("invalid release notes cache".into());
    }
    Ok(pages)
}

fn write_cache(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// Strip terminal commands (CSI, OSC, DCS, SOS, PM, APC, including C1 forms),
/// controls and invisible formatting. Preserve only ordinary text, LF and TAB.
fn display_text(text: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Text,
        Escape,
        Csi,
        String,
        StringEscape,
    }
    let mut state = State::Text;
    let mut result = String::new();
    for c in text.chars() {
        state = match state {
            State::Escape => match c {
                '[' => State::Csi,
                ']' | 'P' | 'X' | '^' | '_' => State::String,
                '\u{20}'..='\u{2f}' => State::Escape,
                _ => State::Text,
            },
            State::Csi => {
                if ('\u{40}'..='\u{7e}').contains(&c) {
                    State::Text
                } else {
                    State::Csi
                }
            }
            State::String => match c {
                '\u{7}' | '\u{9c}' => State::Text,
                '\u{1b}' => State::StringEscape,
                _ => State::String,
            },
            State::StringEscape => match c {
                '\\' | '\u{7}' | '\u{9c}' => State::Text,
                '\u{1b}' => State::StringEscape,
                _ => State::String,
            },
            State::Text => match c {
                '\u{1b}' => State::Escape,
                '\u{9b}' => State::Csi,
                '\u{90}' | '\u{98}' | '\u{9d}' | '\u{9e}' | '\u{9f}' => State::String,
                _ => {
                    if c == '\n'
                        || c == '\t'
                        || (!c.is_control()
                            && !matches!(c,
                        '\u{ad}' | '\u{61c}' | '\u{180e}' | '\u{200b}'..='\u{200f}' |
                        '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}' |
                        '\u{fff9}'..='\u{fffb}' | '\u{e0000}'..='\u{e007f}'))
                    {
                        result.push(c);
                    }
                    State::Text
                }
            },
        };
    }
    result
}

#[cfg(test)]
#[path = "news_tests.rs"]
mod tests;
