//! Requests to GitHub's API, signed in or, for public repositories, not:
//! headers, limits, token refresh and one meaning for every status.
use super::{
    Error, auth,
    config::Endpoints,
    credentials::{self, Tokens},
};
use reqwest::{Method, StatusCode, header::HeaderMap};
use std::{sync::Arc, time::Duration};

pub(super) const JSON_LIMIT: usize = 8 << 20;
/// The largest drawing or preview, in decoded bytes, sent or received.
pub const FILE_LIMIT: usize = 64 << 20;

#[derive(Clone, Copy)]
pub(super) enum Accept {
    Json,
    /// The file's own bytes, with no base64 around them.
    Raw,
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    ends: Endpoints,
    client_id: String,
    session: Arc<tokio::sync::Mutex<Session>>,
    /// Signed out, for public repositories: no token, 60 requests an hour.
    public: bool,
}

struct Session {
    tokens: Option<Tokens>,
    /// Whether the credential store holds the current tokens.
    remembered: bool,
    /// Whether the credential store may hold a sign-in to forget.
    kept: bool,
}

pub(super) fn http(ends: &Endpoints) -> Result<reqwest::Client, Error> {
    let mut builder = crate::http::client()
        .user_agent(concat!("ReShiki/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30));
    // Only an override on this computer is plain http; it needs no proxy.
    if ends.api.starts_with("http://") || ends.web.starts_with("http://") {
        builder = builder.no_proxy();
    } else {
        builder = builder.https_only(true);
    }
    builder.build().map_err(|_| Error::Offline)
}

pub(super) async fn read(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Offline)? {
        if bytes.len() + chunk.len() > limit {
            return Err(Error::TooLarge(None));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

impl Client {
    /// `tokens` come from the credential store, or from a sign-in just made,
    /// with no login yet (then [`Self::remember`] stores them once GitHub has
    /// named the user).
    pub fn new(client_id: String, ends: Endpoints, tokens: Option<Tokens>) -> Result<Self, Error> {
        let kept = tokens
            .as_ref()
            .is_some_and(|tokens| !tokens.login.is_empty());
        Ok(Self {
            http: http(&ends)?,
            ends,
            client_id,
            session: Arc::new(tokio::sync::Mutex::new(Session {
                tokens,
                remembered: true,
                kept,
            })),
            public: false,
        })
    }

    /// A client that never signs in, for public repositories.
    pub fn public(ends: Endpoints) -> Result<Self, Error> {
        Ok(Self {
            public: true,
            ..Self::new(String::new(), ends, None)?
        })
    }

    pub fn is_public(&self) -> bool {
        self.public
    }

    /// Keep the sign-in for the next launch. `false`: the credential store
    /// failed, so the sign-in lasts until the app quits.
    pub async fn remember(&self, login: &str) -> bool {
        let mut session = self.session.lock().await;
        let Some(tokens) = &mut session.tokens else {
            return false;
        };
        tokens.login = login.to_owned();
        session.remembered = credentials::store(tokens).await.is_ok();
        session.kept |= session.remembered;
        session.remembered
    }

    pub async fn remembered(&self) -> bool {
        self.session.lock().await.remembered
    }

    /// Waits for a refresh in flight, so nothing is stored after signing out.
    /// An error means the credential store still holds the sign-in. A sign-in
    /// never stored leaves the store alone: with no keyring there is none.
    pub async fn sign_out(&self) -> Result<(), String> {
        let mut session = self.session.lock().await;
        session.tokens = None;
        if !std::mem::take(&mut session.kept) {
            return Ok(());
        }
        credentials::delete().await
    }

    /// The access token to send. Refreshes it when it is about to expire, or
    /// when GitHub refused `refused`; concurrent callers share one refresh.
    async fn access(&self, refused: Option<&str>) -> Result<String, Error> {
        if self.public {
            return Ok(String::new());
        }
        let mut session = self.session.lock().await;
        let tokens = session.tokens.as_ref().ok_or(Error::NotSignedIn)?;
        let expiring = tokens
            .access_expires_at
            .is_some_and(|at| at < auth::unix_now() + 60);
        if !expiring && refused != Some(tokens.access.as_str()) {
            return Ok(tokens.access.clone());
        }
        let refreshed = match &tokens.refresh {
            Some(refresh) => auth::refresh(&self.client_id, &self.ends, refresh).await,
            None => Err(Error::SignInExpired),
        };
        match refreshed {
            Ok(mut fresh) => {
                fresh.login.clone_from(&tokens.login);
                let access = fresh.access.clone();
                // GitHub has already retired the old pair: the new one is kept
                // in memory first, and stays there even when it cannot be stored.
                let stored = credentials::store(session.tokens.insert(fresh)).await;
                session.remembered = stored.is_ok();
                session.kept |= session.remembered;
                Ok(access)
            }
            Err(Error::SignInExpired) => {
                session.tokens = None;
                let _ = credentials::delete().await;
                Err(Error::SignInExpired)
            }
            Err(error) => Err(error),
        }
    }

    /// One API call; `path` is already percent-encoded.
    pub(super) async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
        accept: Accept,
        limit: usize,
    ) -> Result<Vec<u8>, Error> {
        let mut access = self.access(None).await?;
        let mut retried = false;
        loop {
            let mut request = self
                .http
                .request(method.clone(), self.ends.api(path))
                .header(
                    "Accept",
                    match accept {
                        Accept::Json => "application/vnd.github+json",
                        Accept::Raw => "application/vnd.github.raw+json",
                    },
                )
                .header("X-GitHub-Api-Version", "2022-11-28");
            if !self.public {
                request = request.bearer_auth(&access);
            }
            if let Some(body) = &body {
                request = request
                    .header("Content-Type", "application/json")
                    .body(body.clone());
            }
            let response = request.send().await.map_err(|_| Error::Offline)?;
            let status = response.status();
            if status.is_success() {
                return read(response, limit).await;
            }
            if status == StatusCode::UNAUTHORIZED && self.public {
                return Err(Error::NotSignedIn);
            }
            if status == StatusCode::UNAUTHORIZED && !retried {
                access = self.access(Some(&access)).await?;
                retried = true;
                continue;
            }
            let headers = response.headers().clone();
            // Only the rate-limit wording of a refusal is ever looked at.
            let text = read(response, 64 << 10).await.unwrap_or_default();
            return Err(refusal(status, &headers, &text));
        }
    }
}

fn refusal(status: StatusCode, headers: &HeaderMap, body: &[u8]) -> Error {
    let number = |name: &str| -> Option<u64> { headers.get(name)?.to_str().ok()?.parse().ok() };
    match status.as_u16() {
        401 => Error::SignInExpired,
        403 | 429 => {
            let retry = number("retry-after");
            let spent = number("x-ratelimit-remaining") == Some(0);
            let secondary = String::from_utf8_lossy(body).contains("secondary rate limit");
            if status.as_u16() == 403 && retry.is_none() && !spent && !secondary {
                return Error::NoAccess;
            }
            let reset = number("x-ratelimit-reset")
                .filter(|_| spent)
                .map(|at| at.saturating_sub(auth::unix_now()));
            Error::RateLimited(retry.or(reset).map(Duration::from_secs))
        }
        404 => Error::NotFound,
        409 | 422 => Error::Conflict,
        500.. => Error::Offline,
        _ => Error::Rejected("GitHub refused the request.".into()),
    }
}
