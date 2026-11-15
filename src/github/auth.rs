//! Sign-in with a device code, and token refresh. Neither sends a secret.
use super::{Error, client, config::Endpoints, credentials::Tokens};
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const REFUSED: &str = "GitHub refused the sign-in.";

/// The code the user types on GitHub, and how to wait for them.
#[derive(Clone)]
pub struct DeviceCode {
    pub user_code: String,
    device_code: String,
    /// Always on the configured GitHub site, whatever the reply named.
    pub verification_uri: String,
    pub interval: Duration,
    pub expires_at: Instant,
}

impl std::fmt::Debug for DeviceCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceCode")
            .field("verification_uri", &self.verification_uri)
            .field("interval", &self.interval)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Poll {
    Pending,
    /// Wait this long between polls from now on.
    SlowDown(Duration),
    Expired,
    Denied,
    Done(Tokens),
}

pub(super) fn unix_now() -> u64 {
    unix(SystemTime::now())
}

fn unix(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

async fn post(ends: &Endpoints, path: &str, body: serde_json::Value) -> Result<Vec<u8>, Error> {
    let response = client::http(ends)?
        .post(ends.web(path))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|_| Error::Offline)?;
    if response.status().is_server_error() {
        return Err(Error::Offline);
    }
    // GitHub reports sign-in errors in the body, with status 200 or 400.
    client::read(response, client::JSON_LIMIT).await
}

pub async fn request_code(client_id: &str, ends: &Endpoints) -> Result<DeviceCode, Error> {
    let bytes = post(
        ends,
        "/login/device/code",
        json!({ "client_id": client_id }),
    )
    .await?;
    parse_code(&bytes, ends)
}

pub(super) fn parse_code(bytes: &[u8], ends: &Endpoints) -> Result<DeviceCode, Error> {
    #[derive(Deserialize)]
    struct Reply {
        device_code: String,
        user_code: String,
        verification_uri: String,
        expires_in: u64,
        interval: u64,
    }
    let reply: Reply =
        serde_json::from_slice(bytes).map_err(|_| Error::Rejected(REFUSED.into()))?;
    let site = ends.web("/");
    Ok(DeviceCode {
        user_code: reply.user_code,
        device_code: reply.device_code,
        verification_uri: if reply.verification_uri.starts_with(&site) {
            reply.verification_uri
        } else {
            ends.web("/login/device")
        },
        interval: Duration::from_secs(reply.interval),
        expires_at: Instant::now() + Duration::from_secs(reply.expires_in),
    })
}

pub async fn poll(client_id: &str, ends: &Endpoints, code: &DeviceCode) -> Result<Poll, Error> {
    let body = json!({
        "client_id": client_id,
        "device_code": code.device_code,
        "grant_type": DEVICE_GRANT,
    });
    let bytes = post(ends, "/login/oauth/access_token", body).await?;
    parse_poll(&bytes, SystemTime::now())
}

/// A rejected refresh token is `Error::SignInExpired`; the user signs in again.
pub async fn refresh(client_id: &str, ends: &Endpoints, refresh: &str) -> Result<Tokens, Error> {
    let body = json!({
        "client_id": client_id,
        "grant_type": "refresh_token",
        "refresh_token": refresh,
    });
    let bytes = post(ends, "/login/oauth/access_token", body).await?;
    match parse_poll(&bytes, SystemTime::now())? {
        Poll::Done(tokens) => Ok(tokens),
        Poll::Expired => Err(Error::SignInExpired),
        _ => Err(Error::Rejected(REFUSED.into())),
    }
}

pub(super) fn parse_poll(bytes: &[u8], now: SystemTime) -> Result<Poll, Error> {
    #[derive(Deserialize)]
    struct Reply {
        error: Option<String>,
        interval: Option<u64>,
        access_token: Option<String>,
        expires_in: Option<u64>,
        refresh_token: Option<String>,
        refresh_token_expires_in: Option<u64>,
    }
    let reply: Reply =
        serde_json::from_slice(bytes).map_err(|_| Error::Rejected(REFUSED.into()))?;
    let at = |seconds: u64| unix(now) + seconds;
    match (reply.error.as_deref(), reply.access_token) {
        (None, Some(access)) if !access.is_empty() => Ok(Poll::Done(Tokens {
            login: String::new(),
            access,
            access_expires_at: reply.expires_in.map(at),
            refresh: reply.refresh_token,
            refresh_expires_at: reply.refresh_token_expires_in.map(at),
        })),
        (Some("authorization_pending"), _) => Ok(Poll::Pending),
        (Some("slow_down"), _) => Ok(Poll::SlowDown(Duration::from_secs(
            reply.interval.unwrap_or(10),
        ))),
        (Some("expired_token" | "incorrect_device_code" | "bad_refresh_token"), _) => {
            Ok(Poll::Expired)
        }
        (Some("access_denied"), _) => Ok(Poll::Denied),
        _ => Err(Error::Rejected(REFUSED.into())),
    }
}
