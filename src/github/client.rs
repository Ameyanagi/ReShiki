//! Requests to GitHub's API, signed in or, for public repositories, not:
//! headers, limits, token refresh and one meaning for every status.
use super::{
    Error, auth,
    config::Endpoints,
    credentials::{self, Tokens},
};
use reqwest::{Method, StatusCode, header::HeaderMap};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, PoisonError},
    time::Duration,
};

pub(super) const JSON_LIMIT: usize = 8 << 20;
/// The largest drawing or preview, in decoded bytes, sent or received.
pub const FILE_LIMIT: usize = 64 << 20;
/// The most bytes of replies a client keeps (see [`Replies`]).
const KEPT_LIMIT: usize = 64 << 20;

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
    replies: Arc<Mutex<Replies>>,
}

/// Replies GitHub never changes (an object by its name), by request path,
/// for as long as the client lives. Past [`KEPT_LIMIT`] bytes, the least
/// recently used go first.
#[derive(Default)]
pub(super) struct Replies {
    by_path: HashMap<String, (Vec<u8>, u64)>,
    bytes: usize,
    clock: u64,
}

impl Replies {
    pub(super) fn get(&mut self, path: &str) -> Option<Vec<u8>> {
        self.clock += 1;
        let (bytes, used) = self.by_path.get_mut(path)?;
        *used = self.clock;
        Some(bytes.clone())
    }

    pub(super) fn put(&mut self, path: String, bytes: Vec<u8>, limit: usize) {
        if bytes.len() > limit {
            return;
        }
        self.clock += 1;
        self.bytes += bytes.len();
        if let Some((old, _)) = self.by_path.insert(path, (bytes, self.clock)) {
            self.bytes -= old.len();
        }
        while self.bytes > limit {
            let oldest = (self.by_path.iter())
                .min_by_key(|(_, (_, used))| *used)
                .map(|(path, _)| path.clone());
            let Some((gone, _)) = oldest.and_then(|path| self.by_path.remove(&path)) else {
                break;
            };
            self.bytes -= gone.len();
        }
    }

    #[cfg(test)]
    pub(super) fn bytes(&self) -> usize {
        self.bytes
    }
}

struct Session {
    tokens: Option<Tokens>,
    /// Whether the credential store holds the current tokens.
    remembered: bool,
    /// Whether the credential store may hold a sign-in to forget.
    kept: bool,
}

/// One transport for every client, signed in or not, and for sign-in, so
/// connections (over HTTP/2, one for many requests at once) are reused.
pub(super) fn http(ends: &Endpoints) -> Result<reqwest::Client, Error> {
    static HTTPS: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    static LOCAL: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    // Only an override on this computer is plain http; it needs no proxy.
    let local = ends.api.starts_with("http://") || ends.web.starts_with("http://");
    let shared = if local { &LOCAL } else { &HTTPS };
    let build = || {
        let builder = crate::http::client()
            .user_agent(concat!("ReShiki/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30));
        let builder = match local {
            true => builder.no_proxy(),
            false => builder.https_only(true),
        };
        builder.build().ok()
    };
    shared.get_or_init(build).clone().ok_or(Error::Offline)
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
            replies: Arc::default(),
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

    fn replies(&self) -> std::sync::MutexGuard<'_, Replies> {
        self.replies.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A GET whose reply never changes, such as an object by its name: asked
    /// of GitHub once while the client lives.
    pub(super) async fn get_kept(
        &self,
        path: &str,
        accept: Accept,
        limit: usize,
    ) -> Result<Vec<u8>, Error> {
        if let Some(bytes) = self.replies().get(path) {
            return Ok(bytes);
        }
        let bytes = self.send(Method::GET, path, None, accept, limit).await?;
        self.replies()
            .put(path.to_owned(), bytes.clone(), KEPT_LIMIT);
        Ok(bytes)
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
            } else if [Method::PUT, Method::POST, Method::PATCH].contains(&method) {
                // GitHub refuses a write that states no length (411), and an
                // empty body states none.
                request = request.header("Content-Length", "0");
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
