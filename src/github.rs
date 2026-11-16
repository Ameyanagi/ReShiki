//! Drawings kept in the user's GitHub repositories, through GitHub's REST API.
//! A drawing is saved on this computer first; Send makes one commit of every
//! changed drawing in a repository. There is no client secret anywhere: the
//! user signs in with a device code.
use std::time::Duration;

pub mod api;
pub mod auth;
pub mod client;
pub mod config;
pub mod credentials;
pub mod link;
#[cfg(test)]
pub(crate) mod mock;
pub mod send;
#[cfg(test)]
mod tests;
pub mod workcopy;

/// A repository by the names GitHub last gave it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoKey {
    pub owner: String,
    pub repo: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub id: u64,
    pub key: RepoKey,
    pub private: bool,
    pub default_branch: String,
    /// Whether the signed-in user may send to it.
    pub push: bool,
}

/// A folder, drawing, other chemistry file or preview in one repository
/// folder (or, read whole, the repository).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub sha: String,
    pub folder: bool,
}

/// The chemistry files listed besides drawings, as File › Open reads them,
/// and the label each shows.
const FORMATS: [(&str, &str); 6] = [
    ("mol", "MOL"),
    ("sdf", "SDF"),
    ("cdxml", "CDXML"),
    ("cdx", "CDX"),
    ("rxn", "RXN"),
    ("smi", "SMI"),
];

/// The label of a chemistry file that is not a drawing, by its extension.
pub fn format_label(name: &str) -> Option<&'static str> {
    let (_, extension) = name.rsplit_once('.')?;
    FORMATS
        .iter()
        .find(|(format, _)| extension.eq_ignore_ascii_case(format))
        .map(|(_, label)| *label)
}

/// A file the picker lists: a drawing or another chemistry file.
pub fn listed(name: &str) -> bool {
    name.ends_with(".rsk") || format_label(name).is_some()
}

/// The signed-in user and the repositories ReShiki may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub login: String,
    pub repos: Vec<Repo>,
}

impl Account {
    /// Signed in, but the GitHub App has no repository to work with yet.
    pub fn needs_install(&self) -> bool {
        self.repos.is_empty()
    }
}

/// Every message is fixed text: it never holds a token or a response body.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Sign in to GitHub first.")]
    NotSignedIn,
    #[error("Sign in to GitHub again.")]
    SignInExpired,
    #[error("Couldn't reach GitHub.")]
    Offline,
    #[error("{}", wait_text(*.0))]
    RateLimited(Option<Duration>),
    #[error("Not found on GitHub.")]
    NotFound,
    #[error("No access on GitHub.")]
    NoAccess,
    #[error("GitHub refused the change.")]
    Conflict,
    #[error("{0}")]
    Rejected(String),
    #[error("{}", too_large_text(.0.as_deref()))]
    TooLarge(Option<String>),
}

fn wait_text(wait: Option<Duration>) -> String {
    match wait {
        Some(wait) => format!("Try again in {} min", wait.as_secs().div_ceil(60).max(1)),
        None => "Try again later".into(),
    }
}

fn too_large_text(name: Option<&str>) -> String {
    match name {
        Some(name) => format!("“{name}” is too large for GitHub."),
        None => "GitHub's reply was too large.".into(),
    }
}
