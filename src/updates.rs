//! Channel-aware release checks and verified, staged stable updates.
pub mod install;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RELEASES: &str = "https://github.com/Ameyanagi/ReShiki/releases";
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const API: &str = "https://api.github.com/repos/Ameyanagi/ReShiki/releases";
const MAX_RESPONSE: usize = 8 * 1024 * 1024;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Stable,
    Nightly,
}
impl Channel {
    pub const ALL: [Self; 2] = [Self::Stable, Self::Nightly];

    fn cache_name(self) -> &'static str {
        match self {
            Self::Stable => "update-cache.json",
            Self::Nightly => "update-cache-nightly.json",
        }
    }

    fn accepts(self, version: &Version) -> bool {
        if !version.build.is_empty() {
            return false;
        }
        match self {
            Self::Stable => version.pre.is_empty(),
            Self::Nightly => {
                // Match scripts/prepare_nightly.py, not unrelated beta/RC releases.
                let parts: Vec<_> = version.pre.as_str().split('.').collect();
                let [kind, date, run, attempt] = parts.as_slice() else {
                    return false;
                };
                *kind == "nightly"
                    && date.len() == 8
                    && [date, run, attempt].iter().all(|part| {
                        !part.starts_with('0')
                            && !part.is_empty()
                            && part.bytes().all(|byte| byte.is_ascii_digit())
                    })
            }
        }
    }
}
impl std::fmt::Display for Channel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Stable => "Stable",
            Self::Nightly => "Nightly",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preferences {
    pub automatic: bool,
    pub channel: Channel,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            automatic: true,
            channel: Channel::Stable,
        }
    }
}
impl Preferences {
    fn load(directory: &std::path::Path) -> Self {
        Self {
            automatic: std::fs::read(directory.join("update-preferences.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or(true),
            channel: std::fs::read(directory.join("update-channel.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default(),
        }
    }
    fn save(self, directory: &std::path::Path) -> Result<(), String> {
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        // Preserve the boolean format read by older stable releases, including
        // when a user returns from a nightly after opting out of checks.
        crate::storage::write_atomic(
            &directory.join("update-preferences.json"),
            if self.automatic { b"true" } else { b"false" },
        )?;
        let channel = serde_json::to_vec(&self.channel).map_err(|e| e.to_string())?;
        crate::storage::write_atomic(&directory.join("update-channel.json"), &channel)
    }
}
pub fn preferences() -> Preferences {
    crate::compatibility::data_directory()
        .ok()
        .map(|directory| Preferences::load(&directory))
        .unwrap_or_default()
}
pub async fn save_preferences(preferences: Preferences) -> Result<(), String> {
    tokio::task::spawn_blocking(move || preferences.save(&crate::compatibility::data_directory()?))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Release {
    pub version: String,
}
impl Release {
    pub fn channel(&self) -> Option<Channel> {
        let version = Version::parse(&self.version).ok()?;
        Channel::ALL
            .into_iter()
            .find(|channel| channel.accepts(&version))
    }

    /// A channel switch can intentionally install a lower semantic version.
    /// Within either channel, never offer the same or an older build.
    pub fn available_for(&self, installed: &str) -> bool {
        let (Ok(latest), Ok(current), Some(channel)) = (
            Version::parse(&self.version),
            Version::parse(installed),
            self.channel(),
        ) else {
            return false;
        };
        let switching = match channel {
            Channel::Stable => Channel::Nightly.accepts(&current),
            Channel::Nightly => Channel::Stable.accepts(&current),
        };
        switching || latest > current
    }

    fn tag(&self) -> Result<String, String> {
        match self.channel() {
            Some(Channel::Stable) => Ok(format!("v{}", self.version)),
            Some(Channel::Nightly) => Ok(format!("nightly-{}", self.version)),
            None => Err("Invalid release version".into()),
        }
    }

    pub fn url(&self) -> String {
        // Never open a URL supplied by a server or cache file.
        self.tag()
            .map(|tag| format!("{RELEASES}/tag/{tag}"))
            .unwrap_or_else(|_| RELEASES.into())
    }

    pub fn portable_url(&self, os: &str, arch: &str) -> Result<String, String> {
        if self.channel() != Some(Channel::Nightly) {
            return Err("Portable downloads are offered for nightly builds.".into());
        }
        let arch = match arch {
            "aarch64" => "arm64",
            "x86_64" => "x64",
            _ => return Err("No nightly download is available for this architecture.".into()),
        };
        let suffix = match os {
            "macos" | "windows" => format!("{os}-{arch}.zip"),
            "linux" => format!("linux-{arch}.tar.gz"),
            _ => return Err("No nightly download is available for this platform.".into()),
        };
        Ok(format!(
            "{RELEASES}/download/{}/reshiki-{}-{suffix}",
            self.tag()?,
            self.version
        ))
    }
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}
impl ApiRelease {
    fn release(self, channel: Channel) -> Option<Release> {
        if self.draft || self.prerelease != (channel == Channel::Nightly) {
            return None;
        }
        let tag = match channel {
            Channel::Stable => self.tag_name.strip_prefix('v').unwrap_or(&self.tag_name),
            Channel::Nightly => self.tag_name.strip_prefix("nightly-")?,
        };
        let version = Version::parse(tag).ok()?;
        channel.accepts(&version).then(|| Release {
            version: version.to_string(),
        })
    }
}
fn parse_release(bytes: &[u8]) -> Result<Release, String> {
    let data: ApiRelease = serde_json::from_slice(bytes)
        .map_err(|_| "The release service returned an unreadable response.".to_owned())?;
    data.release(Channel::Stable)
        .ok_or_else(|| "No stable release was returned.".into())
}
fn parse_nightlies(bytes: &[u8]) -> Result<Option<Release>, String> {
    let data: Vec<ApiRelease> = serde_json::from_slice(bytes)
        .map_err(|_| "The release service returned an unreadable response.".to_owned())?;
    Ok(data
        .into_iter()
        .filter_map(|data| data.release(Channel::Nightly))
        .max_by_key(|release| Version::parse(&release.version).ok()))
}

#[derive(Serialize, Deserialize)]
struct Cache {
    checked_at: u64,
    result: Result<Release, String>,
}
impl Cache {
    fn fresh(&self, now: u64) -> bool {
        let interval = if self.result.is_ok() {
            CHECK_INTERVAL.as_secs()
        } else {
            3600
        };
        now.checked_sub(self.checked_at)
            .is_some_and(|age| age < interval)
    }
    fn usable(&self, channel: Channel, manual: bool, now: u64) -> bool {
        !manual
            && self.fresh(now)
            && self
                .result
                .as_ref()
                .map_or(true, |release| release.channel() == Some(channel))
    }
}

pub async fn check(channel: Channel, manual: bool) -> Result<Release, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cached = tokio::task::spawn_blocking(move || {
        let path = crate::compatibility::data_directory()
            .ok()?
            .join(channel.cache_name());
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice::<Cache>(&bytes)
            .ok()
            .filter(|cache| cache.usable(channel, manual, now))
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Some(cache) = cached {
        return cache.result;
    }
    let result = fetch(channel).await;
    let cache = Cache {
        checked_at: now,
        result: result.clone(),
    };
    // A read-only profile must not prevent checking or downloading releases.
    let _ = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let dir = crate::compatibility::data_directory()?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec(&cache).map_err(|e| e.to_string())?;
        crate::storage::write_atomic(&dir.join(channel.cache_name()), &bytes)
    })
    .await;
    result
}

fn release_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("ReShiki/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Could not start the update check: {e}"))
}

async fn fetch(channel: Channel) -> Result<Release, String> {
    let client = release_client()?;
    if channel == Channel::Stable {
        return parse_release(
            &fetch_bytes(&client, &format!("{API}/latest"))
                .await
                .map_err(FetchError::message)?,
        );
    }
    // GitHub excludes prereleases from /latest. Follow release-list pages until
    // a published nightly is found, without accepting arbitrary prereleases.
    for page in 1..=10 {
        let bytes = fetch_bytes(&client, &format!("{API}?per_page=100&page={page}"))
            .await
            .map_err(FetchError::message)?;
        if let Some(release) = parse_nightlies(&bytes)? {
            return Ok(release);
        }
        let count = serde_json::from_slice::<Vec<serde_json::Value>>(&bytes)
            .map_err(|_| "The release service returned an unreadable response.")?
            .len();
        if count < 100 {
            break;
        }
    }
    Err(
        "No published nightly build is available. Try again after the nightly build finishes."
            .into(),
    )
}

enum FetchError {
    Unavailable(String),
    Rejected(String),
    TooLarge,
}
impl FetchError {
    fn message(self) -> String {
        match self {
            Self::Unavailable(message) | Self::Rejected(message) => message,
            Self::TooLarge => "The release response was too large.".into(),
        }
    }
}

async fn fetch_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, FetchError> {
    let mut response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| {
            FetchError::Unavailable("Could not reach GitHub. Try again when you are online.".into())
        })?;
    let status = response.status();
    if !status.is_success() {
        if matches!(status.as_u16(), 404 | 410) {
            return Err(FetchError::Rejected(
                "The selected release is no longer available. Check for updates again.".into(),
            ));
        }
        if matches!(status.as_u16(), 403 | 429) {
            return Err(FetchError::Unavailable(
                "GitHub's request limit was reached. Please try again later.".into(),
            ));
        }
        let message = format!("Update check unavailable (HTTP {status}). Try again later.");
        return Err(
            if status.is_server_error() || status == reqwest::StatusCode::REQUEST_TIMEOUT {
                FetchError::Unavailable(message)
            } else {
                // Other client errors and redirects are not temporary outages.
                FetchError::Rejected(message)
            },
        );
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| FetchError::Unavailable("The update check was interrupted.".into()))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
            return Err(FetchError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn open_release(release: Option<Release>) -> Result<(), String> {
    open_url(release.map(|r| r.url()).unwrap_or_else(|| RELEASES.into())).await
}
fn nightly_download_url(
    release: &Release,
    bytes: &[u8],
    os: &str,
    arch: &str,
) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Asset {
        name: String,
    }
    #[derive(Deserialize)]
    struct Downloads {
        tag_name: String,
        draft: bool,
        prerelease: bool,
        assets: Vec<Asset>,
    }
    let portable = release.portable_url(os, arch)?;
    let downloads: Downloads = serde_json::from_slice(bytes)
        .map_err(|_| "The release service returned unreadable downloads.".to_owned())?;
    if downloads.tag_name != release.tag()? || downloads.draft || !downloads.prerelease {
        return Err("The selected nightly release is no longer available.".into());
    }
    let installer = portable.strip_suffix(".zip").and_then(|stem| match os {
        "macos" => Some(format!("{stem}.dmg")),
        "windows" => Some(format!("{stem}-setup.exe")),
        _ => None,
    });
    // Use only expected names to build trusted GitHub URLs. Older nightlies have
    // portable archives only; server-supplied download URLs are never opened.
    installer
        .into_iter()
        .chain(std::iter::once(portable))
        .find(|url| {
            let filename = url.rsplit('/').next().unwrap_or_default();
            downloads.assets.iter().any(|asset| asset.name == filename)
        })
        .ok_or_else(|| "No matching download is available for this nightly release.".into())
}

pub async fn open_nightly_download(release: Release) -> Result<(), String> {
    open_nightly_download_with(
        release,
        std::env::consts::OS,
        std::env::consts::ARCH,
        async |url| {
            let client = release_client().map_err(FetchError::Unavailable)?;
            fetch_bytes(&client, &url).await
        },
        open_url,
    )
    .await
}

async fn open_nightly_download_with(
    release: Release,
    os: &str,
    arch: &str,
    lookup: impl AsyncFnOnce(String) -> Result<Vec<u8>, FetchError>,
    open: impl AsyncFnOnce(String) -> Result<(), String>,
) -> Result<(), String> {
    // Validate the cached version and target before making any request or opening
    // a browser. A transient API failure must not block its trusted archive URL.
    let portable = release.portable_url(os, arch)?;
    let url = match lookup(format!("{API}/tags/{}", release.tag()?)).await {
        Ok(bytes) => nightly_download_url(&release, &bytes, os, arch)?,
        Err(FetchError::Unavailable(_)) => portable,
        Err(error) => return Err(error.message()),
    };
    open(url).await
}
async fn open_url(url: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        open::that(url).map_err(|e| format!("Could not open the browser: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests;
