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
        return parse_release(&fetch_bytes(&client, &format!("{API}/latest")).await?);
    }
    // GitHub excludes prereleases from /latest. Follow release-list pages until
    // a published nightly is found, without accepting arbitrary prereleases.
    for page in 1..=10 {
        let bytes = fetch_bytes(&client, &format!("{API}?per_page=100&page={page}")).await?;
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

async fn fetch_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "Could not reach GitHub. Try again when you are online.".to_owned())?;
    if !response.status().is_success() {
        return Err(if matches!(response.status().as_u16(), 403 | 429) {
            "GitHub's request limit was reached. Please try again later.".into()
        } else {
            format!(
                "Update check unavailable (HTTP {}). Try again later.",
                response.status()
            )
        });
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "The update check was interrupted.".to_owned())?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
            return Err("The release response was too large.".into());
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
    // Reject nonnightly versions before making a request.
    release.portable_url(std::env::consts::OS, std::env::consts::ARCH)?;
    let client = release_client()?;
    let bytes = fetch_bytes(&client, &format!("{API}/tags/{}", release.tag()?)).await?;
    open_url(nightly_download_url(
        &release,
        &bytes,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )?)
    .await
}
async fn open_url(url: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        open::that(url).map_err(|e| format!("Could not open the browser: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(version: &str) -> Release {
        Release {
            version: version.into(),
        }
    }
    const NIGHTLY: &str = "0.9.1-nightly.20260929.36501221724.1";

    #[tokio::test]
    #[ignore = "Live official GitHub release discovery; no downloads or preference writes"]
    async fn discovers_published_stable_and_nightly_releases() {
        for channel in Channel::ALL {
            let release = fetch(channel).await.unwrap();
            assert_eq!(release.channel(), Some(channel));
            eprintln!("{channel}: {} ({})", release.version, release.url());
        }
    }

    #[test]
    fn compares_versions_within_channels_and_allows_explicit_channel_switches() {
        assert!(release("0.10.0").available_for("0.9.9"));
        assert!(release("0.10.0").available_for("0.10.0-rc.1"));
        assert!(!release("0.10.0").available_for("0.10.0"));
        assert!(!release("0.10.0").available_for("1.0.0"));
        assert!(!release("1.0.0-beta.1").available_for("0.3.0"));
        assert!(release(NIGHTLY).available_for("0.9.1"));
        assert!(release("0.9.0").available_for(NIGHTLY));
        assert!(!release(NIGHTLY).available_for(NIGHTLY));
        assert!(release(NIGHTLY).available_for("0.9.1-nightly.20260928.9999999999.9"));
        assert!(
            release("0.9.1-nightly.20260929.36501221724.10")
                .available_for("0.9.1-nightly.20260929.36501221724.9")
        );
        assert!(!release(NIGHTLY).available_for("0.9.1-nightly.20260929.36501221724.2"));
        assert!(!release(NIGHTLY).available_for("invalid"));
    }

    #[test]
    fn stable_metadata_stays_strict_and_urls_are_constructed_locally() {
        let parsed = parse_release(br#"{"tag_name":"v0.4.0","draft":false,"prerelease":false,"html_url":"https://untrusted.example"}"#).unwrap();
        assert_eq!(parsed.url(), format!("{RELEASES}/tag/v0.4.0"));
        for input in [
            br#"{"tag_name":"v0.4.0","draft":true,"prerelease":false}"#.as_slice(),
            br#"{"tag_name":"v0.4.0-beta.1","draft":false,"prerelease":false}"#,
            br#"{"tag_name":"v0.4.0","draft":false,"prerelease":true}"#,
            br#"{"tag_name":"../../evil","draft":false,"prerelease":false}"#,
            b"not json",
        ] {
            assert!(parse_release(input).is_err());
        }
        assert_eq!(release("../bad").url(), RELEASES);
    }

    #[test]
    fn nightly_discovery_rejects_other_prereleases_and_sorts_numeric_identifiers() {
        let data = serde_json::json!([
            {"tag_name":"v99.0.0","draft":false,"prerelease":false},
            {"tag_name":"v99.0.0-rc.1","draft":false,"prerelease":true},
            {"tag_name":"nightly-0.9.1-nightly.20260929.20.9","draft":false,"prerelease":true},
            {"tag_name":"nightly-0.9.1-nightly.20260929.20.10","draft":false,"prerelease":true},
            {"tag_name":"nightly-0.9.1-nightly.20260930.20.1","draft":true,"prerelease":true},
            {"tag_name":"nightly-0.9.1-nightly.20260930.21.1","draft":false,"prerelease":false},
            {"tag_name":"nightly-../../evil","draft":false,"prerelease":true},
            {"tag_name":"nightly-0.9.1-nightly.20260930.22.1+other","draft":false,"prerelease":true},
            {"tag_name":"nightly-0.9.1-nightly.20260930.22","draft":false,"prerelease":true}
        ]);
        let result = parse_nightlies(&serde_json::to_vec(&data).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(result.version, "0.9.1-nightly.20260929.20.10");
        assert_eq!(
            result.url(),
            format!("{RELEASES}/tag/nightly-{}", result.version)
        );
        assert_eq!(parse_nightlies(b"[]").unwrap(), None);
        assert!(parse_nightlies(b"not json").is_err());
    }

    #[test]
    fn preferences_preserve_legacy_opt_out_and_round_trip_channel_selection() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        assert_eq!(Preferences::load(path), Preferences::default());
        std::fs::write(path.join("update-preferences.json"), b"false").unwrap();
        assert_eq!(
            Preferences::load(path),
            Preferences {
                automatic: false,
                channel: Channel::Stable
            }
        );
        let preferences = Preferences {
            automatic: false,
            channel: Channel::Nightly,
        };
        preferences.save(path).unwrap();
        assert_eq!(Preferences::load(path), preferences);
        // Old stable builds still read the opt-out as a JSON boolean.
        assert!(
            !serde_json::from_slice::<bool>(
                &std::fs::read(path.join("update-preferences.json")).unwrap()
            )
            .unwrap()
        );
        std::fs::write(path.join("update-channel.json"), b"invalid").unwrap();
        assert_eq!(
            Preferences::load(path),
            Preferences {
                automatic: false,
                channel: Channel::Stable
            }
        );
    }

    #[test]
    fn portable_downloads_cover_the_six_published_nightly_packages() {
        for (os, arch, suffix) in [
            ("macos", "aarch64", "macos-arm64.zip"),
            ("macos", "x86_64", "macos-x64.zip"),
            ("windows", "aarch64", "windows-arm64.zip"),
            ("windows", "x86_64", "windows-x64.zip"),
            ("linux", "aarch64", "linux-arm64.tar.gz"),
            ("linux", "x86_64", "linux-x64.tar.gz"),
        ] {
            assert_eq!(
                release(NIGHTLY).portable_url(os, arch).unwrap(),
                format!("{RELEASES}/download/nightly-{NIGHTLY}/reshiki-{NIGHTLY}-{suffix}")
            );
        }
        assert!(release(NIGHTLY).portable_url("macos", "riscv64").is_err());
        assert!(release(NIGHTLY).portable_url("unknown", "x86_64").is_err());
        assert!(release("../bad").portable_url("linux", "x86_64").is_err());
        assert!(release("0.9.1").portable_url("linux", "x86_64").is_err());
    }

    #[test]
    fn nightly_downloads_prefer_published_installers_and_fall_back_to_legacy_archives() {
        for (os, arch, platform, installer, portable) in [
            ("macos", "aarch64", "macos-arm64", ".dmg", ".zip"),
            ("macos", "x86_64", "macos-x64", ".dmg", ".zip"),
            ("windows", "aarch64", "windows-arm64", "-setup.exe", ".zip"),
            ("windows", "x86_64", "windows-x64", "-setup.exe", ".zip"),
            ("linux", "aarch64", "linux-arm64", ".tar.gz", ".tar.gz"),
            ("linux", "x86_64", "linux-x64", ".tar.gz", ".tar.gz"),
        ] {
            let stem = format!("reshiki-{NIGHTLY}-{platform}");
            let mut response = serde_json::json!({
                "tag_name": format!("nightly-{NIGHTLY}"), "draft": false, "prerelease": true,
                "assets": [{"name": format!("{stem}{portable}"), "browser_download_url": "https://untrusted.example/archive"}]
            });
            let bytes = serde_json::to_vec(&response).unwrap();
            assert_eq!(
                nightly_download_url(&release(NIGHTLY), &bytes, os, arch).unwrap(),
                release(NIGHTLY).portable_url(os, arch).unwrap()
            );
            response["assets"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"name": format!("{stem}{installer}")}));
            let bytes = serde_json::to_vec(&response).unwrap();
            assert!(
                nightly_download_url(&release(NIGHTLY), &bytes, os, arch)
                    .unwrap()
                    .ends_with(installer)
            );
            response["draft"] = true.into();
            assert!(
                nightly_download_url(
                    &release(NIGHTLY),
                    &serde_json::to_vec(&response).unwrap(),
                    os,
                    arch
                )
                .is_err()
            );
            response["draft"] = false.into();
            response["assets"] = serde_json::json!([]);
            assert!(
                nightly_download_url(
                    &release(NIGHTLY),
                    &serde_json::to_vec(&response).unwrap(),
                    os,
                    arch
                )
                .is_err()
            );
        }
    }

    #[test]
    fn caches_are_channel_specific_and_manual_checks_bypass_them() {
        let mut cache = Cache {
            checked_at: 100,
            result: Ok(release("0.4.0")),
        };
        assert_ne!(Channel::Stable.cache_name(), Channel::Nightly.cache_name());
        assert!(cache.usable(Channel::Stable, false, 101));
        assert!(!cache.usable(Channel::Nightly, false, 101));
        assert!(!cache.usable(Channel::Stable, true, 101));
        assert!(!cache.fresh(99));
        assert!(!cache.fresh(86500));
        cache.result = Err("Offline".into());
        assert!(cache.fresh(3699));
        assert!(!cache.fresh(3700));
    }
}
