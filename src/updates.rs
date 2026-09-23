//! Stable release checks and verified, staged native updates.
pub mod install;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RELEASES: &str = "https://github.com/Ameyanagi/ReShiki/releases";
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const ENDPOINT: &str = "https://api.github.com/repos/Ameyanagi/ReShiki/releases/latest";
const MAX_RESPONSE: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Release {
    pub version: String,
}
impl Release {
    pub fn newer_than(&self, installed: &str) -> bool {
        match (Version::parse(&self.version), Version::parse(installed)) {
            (Ok(latest), Ok(current)) => latest > current && latest.pre.is_empty(),
            _ => false,
        }
    }
    pub fn url(&self) -> String {
        // Never open a URL supplied by a server or cache file.
        match Version::parse(&self.version) {
            Ok(version) if version.pre.is_empty() && version.build.is_empty() => {
                format!("{RELEASES}/tag/v{version}")
            }
            _ => RELEASES.into(),
        }
    }
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}
fn parse_release(bytes: &[u8]) -> Result<Release, String> {
    let data: ApiRelease = serde_json::from_slice(bytes)
        .map_err(|_| "The release service returned an unreadable response.".to_owned())?;
    let version = Version::parse(data.tag_name.strip_prefix('v').unwrap_or(&data.tag_name))
        .map_err(|_| "The release service returned an invalid version.".to_owned())?;
    if data.draft || data.prerelease || !version.pre.is_empty() || !version.build.is_empty() {
        return Err("No stable release was returned.".into());
    }
    Ok(Release {
        version: version.to_string(),
    })
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
}

pub fn automatic_enabled() -> bool {
    crate::compatibility::data_directory()
        .ok()
        .and_then(|dir| std::fs::read(dir.join("update-preferences.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<bool>(&bytes).ok())
        .unwrap_or(true)
}
pub async fn save_automatic(enabled: bool) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let dir = crate::compatibility::data_directory()?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        crate::storage::write_atomic(
            &dir.join("update-preferences.json"),
            if enabled { b"true" } else { b"false" },
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn check(manual: bool) -> Result<Release, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cached = tokio::task::spawn_blocking(move || {
        let path = crate::compatibility::data_directory()
            .ok()?
            .join("update-cache.json");
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice::<Cache>(&bytes)
            .ok()
            .filter(|cache| !manual && cache.fresh(now))
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Some(cache) = cached {
        return cache.result;
    }
    let result = fetch().await;
    let cache = Cache {
        checked_at: now,
        result: result.clone(),
    };
    // A read-only profile must not prevent checking or downloading releases.
    let _ = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let dir = crate::compatibility::data_directory()?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec(&cache).map_err(|e| e.to_string())?;
        crate::storage::write_atomic(&dir.join("update-cache.json"), &bytes)
    })
    .await;
    result
}

async fn fetch() -> Result<Release, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("ReShiki/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Could not start the update check: {e}"))?;
    let mut response = client
        .get(ENDPOINT)
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
    parse_release(&bytes)
}

pub async fn open_release(release: Option<Release>) -> Result<(), String> {
    let url = release.map(|r| r.url()).unwrap_or_else(|| RELEASES.into());
    tokio::task::spawn_blocking(move || {
        open::that(url).map_err(|e| format!("Could not open the browser: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically_and_never_downgrades() {
        let release = Release {
            version: "0.10.0".into(),
        };
        assert!(release.newer_than("0.9.9"));
        assert!(release.newer_than("0.10.0-rc.1"));
        assert!(!release.newer_than("0.10.0"));
        assert!(!release.newer_than("1.0.0"));
        assert!(
            !Release {
                version: "1.0.0-beta.1".into()
            }
            .newer_than("0.3.0")
        );
    }

    #[test]
    fn only_stable_release_metadata_is_accepted() -> Result<(), String> {
        let release = parse_release(br#"{"tag_name":"v0.4.0","draft":false,"prerelease":false,"html_url":"https://untrusted.example"}"#)?;
        assert_eq!(release.url(), format!("{RELEASES}/tag/v0.4.0"));
        for input in [
            br#"{"tag_name":"v0.4.0","draft":true,"prerelease":false}"#.as_slice(),
            br#"{"tag_name":"v0.4.0-beta.1","draft":false,"prerelease":false}"#,
            br#"{"tag_name":"v0.4.0","draft":false,"prerelease":true}"#,
            br#"{"tag_name":"../../evil","draft":false,"prerelease":false}"#,
            b"not json",
        ] {
            assert!(parse_release(input).is_err());
        }
        assert_eq!(
            Release {
                version: "../bad".into()
            }
            .url(),
            RELEASES
        );
        Ok(())
    }

    #[test]
    fn caches_successes_daily_and_failures_for_an_hour() {
        let mut cache = Cache {
            checked_at: 100,
            result: Ok(Release {
                version: "0.4.0".into(),
            }),
        };
        assert!(cache.fresh(101));
        assert!(!cache.fresh(99));
        assert!(!cache.fresh(86500));
        cache.result = Err("Offline".into());
        assert!(cache.fresh(3699));
        assert!(!cache.fresh(3700));
    }
}
