//! Which GitHub App this build signs in to, and where GitHub is.
use reqwest::Url;

/// The public identifiers of the ReShiki GitHub App (github.com/apps/reshiki).
/// There is no client secret: sign-in uses a device code. Empty would mean
/// this build has no GitHub sign-in.
const CLIENT_ID: &str = "Iv23li1avBvJkyU0uwhc";
const APP_SLUG: &str = "reshiki";

const API: &str = "https://api.github.com";
const WEB: &str = "https://github.com";

/// `None` when GitHub is not set up in this build.
pub fn client_id() -> Option<String> {
    configured(std::env::var("RESHIKI_GITHUB_CLIENT_ID").ok(), CLIENT_ID)
}

pub fn app_slug() -> Option<String> {
    configured(std::env::var("RESHIKI_GITHUB_APP_SLUG").ok(), APP_SLUG)
}

pub(super) fn configured(env: Option<String>, built_in: &str) -> Option<String> {
    Some(env.unwrap_or_else(|| built_in.into())).filter(|id| !id.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// Base addresses without a trailing slash.
    pub api: String,
    pub web: String,
    /// An override was set, so the sign-in is kept in memory only: a stale or
    /// hostile override never receives the tokens in the credential store.
    pub overridden: bool,
}

impl Endpoints {
    pub fn api(&self, path: &str) -> String {
        format!("{}{path}", self.api)
    }
    pub fn web(&self, path: &str) -> String {
        format!("{}{path}", self.web)
    }
}

/// GitHub, or the `RESHIKI_GITHUB_API` / `RESHIKI_GITHUB_WEB` overrides.
pub fn endpoints() -> Endpoints {
    resolve(
        std::env::var("RESHIKI_GITHUB_API").ok(),
        std::env::var("RESHIKI_GITHUB_WEB").ok(),
    )
}

pub(super) fn resolve(api: Option<String>, web: Option<String>) -> Endpoints {
    let overridden = api.is_some() || web.is_some();
    let pick = |value: Option<String>, default: &str| match value {
        Some(value) if allowed(&value) => value.trim_end_matches('/').to_owned(),
        Some(_) => {
            // Never the address itself: it may hold credentials.
            eprintln!("Ignored a GitHub address override: use https, or http on this computer.");
            default.to_owned()
        }
        None => default.to_owned(),
    };
    Endpoints {
        api: pick(api, API),
        web: pick(web, WEB),
        overridden,
    }
}

/// `https` anywhere, or plain `http` on this computer (the test server).
fn allowed(address: &str) -> bool {
    Url::parse(address).is_ok_and(|url| match url.scheme() {
        "https" => true,
        "http" => matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")),
        _ => false,
    })
}
