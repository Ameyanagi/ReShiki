//! The GitHub sign-in, kept in the operating system's credential store and
//! nowhere else: never in settings, logs, state files or recovery drafts.
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
#[path = "credentials/linux.rs"]
mod os;
#[cfg(target_os = "macos")]
#[path = "credentials/macos.rs"]
mod os;
#[cfg(windows)]
#[path = "credentials/windows.rs"]
mod os;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
use memory as os;

pub(super) mod memory;

/// The live tests use an entry of their own, never the user's sign-in.
const SERVICE: &str = if cfg!(test) {
    "dev.reshiki.ReShiki.github.test"
} else {
    "dev.reshiki.ReShiki.github"
};
#[cfg(any(target_os = "linux", target_os = "macos"))]
const ACCOUNT: &str = "default";
const FAILED: &str = "The sign-in store is not available.";

/// Times are seconds since 1970. `login` is empty until GitHub names the user.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub login: String,
    pub access: String,
    pub access_expires_at: Option<u64>,
    pub refresh: Option<String>,
    pub refresh_expires_at: Option<u64>,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens")
            .field("login", &self.login)
            .field("access_expires_at", &self.access_expires_at)
            .field("refresh_expires_at", &self.refresh_expires_at)
            .finish_non_exhaustive()
    }
}

/// Tests and overridden GitHub addresses never touch the real store.
fn in_memory() -> bool {
    cfg!(test) || super::config::endpoints().overridden
}

pub async fn load() -> Result<Option<Tokens>, String> {
    let bytes = if in_memory() {
        memory::read().await
    } else {
        os::read().await
    }?;
    bytes
        .map(|bytes| serde_json::from_slice(&bytes).map_err(|_| FAILED.to_owned()))
        .transpose()
}

pub async fn store(tokens: &Tokens) -> Result<(), String> {
    let bytes = serde_json::to_vec(tokens).map_err(|_| FAILED)?;
    if in_memory() {
        memory::write(bytes).await
    } else {
        os::write(bytes).await
    }
}

pub async fn delete() -> Result<(), String> {
    if in_memory() {
        memory::delete().await
    } else {
        os::delete().await
    }
}

#[cfg(test)]
pub(super) use os::{delete as os_delete, read as os_read, write as os_write};
