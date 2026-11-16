//! GitHub's REST API, one function per call. Replies are checked here, once.
use super::{
    Account, Entry, Error, Repo, RepoKey,
    client::{Accept, Client, FILE_LIMIT, JSON_LIMIT},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::Method;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::HashMap;

const UNREADABLE: &str = "GitHub sent an unreadable reply.";

async fn call<T: DeserializeOwned>(
    client: &Client,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<T, Error> {
    let body = body.map(|body| body.to_string().into_bytes());
    let bytes = client
        .send(method, path, body, Accept::Json, JSON_LIMIT)
        .await?;
    serde_json::from_slice(&bytes).map_err(|_| Error::Rejected(UNREADABLE.into()))
}

/// Object names go into request paths and file names: plain hex only.
fn sha(sha: String) -> Result<String, Error> {
    if !sha.is_empty() && sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(sha)
    } else {
        Err(Error::Rejected(UNREADABLE.into()))
    }
}

/// Percent-encode a path or query value, keeping `/` between folders.
pub fn encoded(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn base(repo: &RepoKey) -> String {
    format!("/repos/{}/{}", encoded(&repo.owner), encoded(&repo.repo))
}

#[derive(Deserialize)]
struct Named {
    sha: String,
}

#[derive(Deserialize)]
struct RepoReply {
    id: u64,
    name: String,
    owner: Owner,
    private: bool,
    default_branch: String,
    #[serde(default)]
    permissions: Permissions,
}
#[derive(Deserialize)]
struct Owner {
    login: String,
}
#[derive(Deserialize, Default)]
struct Permissions {
    #[serde(default)]
    push: bool,
}

/// An owner or repository names a folder of the working copy, so it must
/// be a name every platform can hold (`con` is a real GitHub account).
fn name_ok(name: &str) -> bool {
    !name.contains('/') && super::workcopy::valid_repo_path(name)
}

impl RepoReply {
    fn checked(self) -> Result<Repo, Error> {
        if !name_ok(&self.owner.login) || !name_ok(&self.name) {
            return Err(Error::Rejected(UNREADABLE.into()));
        }
        Ok(Repo {
            id: self.id,
            key: RepoKey {
                owner: self.owner.login,
                repo: self.name,
            },
            private: self.private,
            default_branch: self.default_branch,
            push: self.permissions.push,
        })
    }
}

pub async fn user(client: &Client) -> Result<String, Error> {
    #[derive(Deserialize)]
    struct User {
        login: String,
    }
    let user: User = call(client, Method::GET, "/user", None).await?;
    Ok(user.login)
}

/// One page: an account installs the app once, so 100 is beyond any real user.
pub async fn installations(client: &Client) -> Result<Vec<u64>, Error> {
    #[derive(Deserialize)]
    struct Reply {
        installations: Vec<Installation>,
    }
    #[derive(Deserialize)]
    struct Installation {
        id: u64,
    }
    let reply: Reply = call(
        client,
        Method::GET,
        "/user/installations?per_page=100",
        None,
    )
    .await?;
    Ok(reply.installations.iter().map(|one| one.id).collect())
}

pub async fn repos(client: &Client, installation: u64) -> Result<Vec<Repo>, Error> {
    #[derive(Deserialize)]
    struct Reply {
        repositories: Vec<RepoReply>,
    }
    let mut repos = Vec::new();
    for page in 1.. {
        let path =
            format!("/user/installations/{installation}/repositories?per_page=100&page={page}");
        let reply: Reply = call(client, Method::GET, &path, None).await?;
        let full = reply.repositories.len() == 100;
        // A repository with a name we cannot use is left out, not an error.
        repos.extend(
            reply
                .repositories
                .into_iter()
                .filter_map(|repo| repo.checked().ok()),
        );
        if !full {
            break;
        }
    }
    Ok(repos)
}

/// The signed-in user and every repository the app was given.
pub async fn account(client: &Client) -> Result<Account, Error> {
    let login = user(client).await?;
    let mut all = Vec::new();
    for installation in installations(client).await? {
        all.extend(repos(client, installation).await?);
    }
    Ok(Account { login, repos: all })
}

/// By id, so a renamed repository or a changed default branch is found.
pub async fn repo(client: &Client, id: u64) -> Result<Repo, Error> {
    let reply: RepoReply = call(client, Method::GET, &format!("/repositories/{id}"), None).await?;
    reply.checked()
}

/// By name, for a repository opened by link. Signed out, GitHub says
/// nothing of permissions, so it reads as one the user may not send to.
pub async fn repo_named(client: &Client, key: &RepoKey) -> Result<Repo, Error> {
    let reply: RepoReply = call(client, Method::GET, &base(key), None).await?;
    reply.checked()
}

/// Always private, with a first commit so drawings can be sent at once.
pub async fn create_repo(client: &Client, name: &str) -> Result<Repo, Error> {
    let body = json!({ "name": name, "private": true, "auto_init": true });
    let reply: RepoReply = call(client, Method::POST, "/user/repos", Some(body)).await?;
    reply.checked()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub commit: String,
    pub tree: String,
}

/// `Error::Conflict`: the repository is empty. `Error::NotFound`: no such branch.
pub async fn head(client: &Client, repo: &RepoKey, branch: &str) -> Result<Head, Error> {
    #[derive(Deserialize)]
    struct Ref {
        object: Named,
    }
    #[derive(Deserialize)]
    struct Commit {
        tree: Named,
    }
    let path = format!("{}/git/ref/heads/{}", base(repo), encoded(branch));
    let reference: Ref = call(client, Method::GET, &path, None).await?;
    let commit = sha(reference.object.sha)?;
    let path = format!("{}/git/commits/{commit}", base(repo));
    let reply: Commit = call(client, Method::GET, &path, None).await?;
    Ok(Head {
        commit,
        tree: sha(reply.tree.sha)?,
    })
}

/// The commit a branch or commit name points at, and its tree.
pub async fn commit(client: &Client, repo: &RepoKey, reference: &str) -> Result<Head, Error> {
    #[derive(Deserialize)]
    struct Reply {
        sha: String,
        commit: Commit,
    }
    #[derive(Deserialize)]
    struct Commit {
        tree: Named,
    }
    let path = format!("{}/commits/{}", base(repo), encoded(reference));
    let reply: Reply = call(client, Method::GET, &path, None).await?;
    Ok(Head {
        commit: sha(reply.sha)?,
        tree: sha(reply.commit.tree.sha)?,
    })
}

/// The folders, drawings (`.rsk`) and previews (`.svg`) of one folder.
pub async fn tree(client: &Client, repo: &RepoKey, tree: &str) -> Result<Vec<Entry>, Error> {
    #[derive(Deserialize)]
    struct Reply {
        tree: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        path: String,
        r#type: String,
        sha: String,
    }
    let path = format!("{}/git/trees/{tree}", base(repo));
    let reply: Reply = call(client, Method::GET, &path, None).await?;
    let mut entries = Vec::new();
    for item in reply.tree {
        let folder = item.r#type == "tree";
        let shown = folder
            || item.r#type == "blob"
                && (item.path.ends_with(".rsk") || item.path.ends_with(".svg"));
        if shown {
            entries.push(Entry {
                name: item.path,
                sha: sha(item.sha)?,
                folder,
            });
        }
    }
    Ok(entries)
}

/// The folders of one commit, each read from GitHub once.
pub struct Folders<'a> {
    client: &'a Client,
    repo: &'a RepoKey,
    root: String,
    seen: HashMap<String, Option<Vec<Entry>>>,
}

impl<'a> Folders<'a> {
    pub fn new(client: &'a Client, repo: &'a RepoKey, root: String) -> Self {
        Self {
            client,
            repo,
            root,
            seen: HashMap::new(),
        }
    }

    /// What `folder` holds (`""` is the top); `None` when there is no such folder.
    pub async fn list(&mut self, folder: &str) -> Result<Option<&[Entry]>, Error> {
        // The top first, then each folder on the way down.
        let mut paths = vec![""];
        if !folder.is_empty() {
            paths.extend(
                folder
                    .match_indices('/')
                    .map(|(at, _)| folder.split_at(at).0),
            );
            paths.push(folder);
        }
        for path in paths {
            if self.seen.contains_key(path) {
                continue;
            }
            let sha = if path.is_empty() {
                Some(self.root.clone())
            } else {
                let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
                let entries = self.seen.get(parent).and_then(|entries| entries.as_ref());
                entries.and_then(|entries| {
                    let found = entries
                        .iter()
                        .find(|entry| entry.folder && entry.name == name)?;
                    Some(found.sha.clone())
                })
            };
            let entries = match sha {
                Some(sha) => Some(tree(self.client, self.repo, &sha).await?),
                None => None,
            };
            self.seen.insert(path.to_owned(), entries);
        }
        Ok(self.seen.get(folder).and_then(|entries| entries.as_deref()))
    }

    /// The blob of the file at `path`, if the commit has it.
    pub async fn blob(&mut self, path: &str) -> Result<Option<String>, Error> {
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
        Ok(self.list(folder).await?.and_then(|entries| {
            let found = entries
                .iter()
                .find(|entry| !entry.folder && entry.name == name)?;
            Some(found.sha.clone())
        }))
    }
}

pub async fn blob(client: &Client, repo: &RepoKey, sha: &str) -> Result<Vec<u8>, Error> {
    let path = format!("{}/git/blobs/{sha}", base(repo));
    client
        .send(Method::GET, &path, None, Accept::Raw, FILE_LIMIT)
        .await
}

/// A drawing as it is on GitHub now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub commit: String,
    pub blob: String,
    pub bytes: Vec<u8>,
}

/// The file at `path` on the head of `branch`.
pub async fn download(
    client: &Client,
    repo: &RepoKey,
    branch: &str,
    path: &str,
) -> Result<Download, Error> {
    let head = head(client, repo, branch).await?;
    let found = Folders::new(client, repo, head.tree).blob(path).await?;
    let sha = found.ok_or(Error::NotFound)?;
    Ok(Download {
        commit: head.commit,
        bytes: blob(client, repo, &sha).await?,
        blob: sha,
    })
}

/// The file at `path` as it is at a branch or commit.
pub async fn download_at(
    client: &Client,
    repo: &RepoKey,
    reference: &str,
    path: &str,
) -> Result<Download, Error> {
    let head = commit(client, repo, reference).await?;
    let found = Folders::new(client, repo, head.tree).blob(path).await?;
    let sha = found.ok_or(Error::NotFound)?;
    Ok(Download {
        commit: head.commit,
        bytes: blob(client, repo, &sha).await?,
        blob: sha,
    })
}

async fn created(client: &Client, path: &str, body: Value) -> Result<String, Error> {
    let reply: Named = call(client, Method::POST, path, Some(body)).await?;
    sha(reply.sha)
}

pub async fn create_blob(client: &Client, repo: &RepoKey, bytes: &[u8]) -> Result<String, Error> {
    let body = json!({ "content": STANDARD.encode(bytes), "encoding": "base64" });
    created(client, &format!("{}/git/blobs", base(repo)), body).await
}

/// `files` are `(path, blob)` pairs laid over the tree `base_tree`.
pub async fn create_tree(
    client: &Client,
    repo: &RepoKey,
    base_tree: &str,
    files: &[(String, String)],
) -> Result<String, Error> {
    let tree: Vec<Value> = files
        .iter()
        .map(|(path, sha)| json!({ "path": path, "mode": "100644", "type": "blob", "sha": sha }))
        .collect();
    let body = json!({ "base_tree": base_tree, "tree": tree });
    created(client, &format!("{}/git/trees", base(repo)), body).await
}

pub async fn create_commit(
    client: &Client,
    repo: &RepoKey,
    message: &str,
    tree: &str,
    parent: &str,
) -> Result<String, Error> {
    let body = json!({ "message": message, "tree": tree, "parents": [parent] });
    created(client, &format!("{}/git/commits", base(repo)), body).await
}

/// Fast-forward only. `Error::Conflict`: the branch moved, or GitHub refused.
pub async fn update_ref(
    client: &Client,
    repo: &RepoKey,
    branch: &str,
    commit: &str,
) -> Result<(), Error> {
    let path = format!("{}/git/refs/heads/{}", base(repo), encoded(branch));
    let body = json!({ "sha": commit, "force": false });
    call::<Value>(client, Method::PATCH, &path, Some(body)).await?;
    Ok(())
}

/// The first commit of an empty repository, which has no branch to build on.
pub async fn init_readme(client: &Client, repo: &RepoKey) -> Result<(), Error> {
    let path = format!("{}/contents/README.md", base(repo));
    let text = format!("# {}\n", repo.repo);
    let body = json!({ "message": "Start repository", "content": STANDARD.encode(text) });
    call::<Value>(client, Method::PUT, &path, Some(body)).await?;
    Ok(())
}

/// One commit that touched a drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteVersion {
    pub commit: String,
    /// The first line of the message.
    pub message: String,
    /// The GitHub account, when the commit has one.
    pub login: Option<String>,
    pub author: String,
    /// When it was made, as GitHub writes it (`2026-11-15T14:02:00Z`).
    pub date: String,
}

/// Commits touching `path`, newest first, 20 a page, counted from `head`.
pub async fn file_commits(
    client: &Client,
    repo: &RepoKey,
    path: &str,
    head: &str,
    page: u32,
) -> Result<Vec<RemoteVersion>, Error> {
    #[derive(Deserialize)]
    struct Item {
        sha: String,
        commit: Commit,
        author: Option<Owner>,
    }
    #[derive(Deserialize)]
    struct Commit {
        message: String,
        author: Author,
    }
    #[derive(Deserialize)]
    struct Author {
        name: String,
        date: String,
    }
    let path = format!(
        "{}/commits?sha={head}&path={}&per_page=20&page={page}",
        base(repo),
        encoded(path)
    );
    let items: Vec<Item> = call(client, Method::GET, &path, None).await?;
    items
        .into_iter()
        .map(|item| {
            Ok(RemoteVersion {
                commit: sha(item.sha)?,
                message: item
                    .commit
                    .message
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .into(),
                login: item.author.map(|author| author.login),
                author: item.commit.author.name,
                date: item.commit.author.date,
            })
        })
        .collect()
}

/// The file as it was at `commit`. `Error::NotFound`: that commit deleted it.
pub async fn raw_at(
    client: &Client,
    repo: &RepoKey,
    path: &str,
    commit: &str,
) -> Result<Vec<u8>, Error> {
    let path = format!("{}/contents/{}?ref={commit}", base(repo), encoded(path));
    client
        .send(Method::GET, &path, None, Accept::Raw, FILE_LIMIT)
        .await
}

pub async fn star(client: &Client, owner: &str, repo: &str) -> Result<(), Error> {
    let path = format!("/user/starred/{}/{}", encoded(owner), encoded(repo));
    client
        .send(Method::PUT, &path, None, Accept::Json, JSON_LIMIT)
        .await?;
    Ok(())
}

/// Whether the signed-in user starred `owner/repo`.
pub async fn starred(client: &Client, owner: &str, repo: &str) -> Result<bool, Error> {
    let path = format!("/user/starred/{}/{}", encoded(owner), encoded(repo));
    match client
        .send(Method::GET, &path, None, Accept::Json, JSON_LIMIT)
        .await
    {
        Ok(_) => Ok(true),
        Err(Error::NotFound) => Ok(false),
        Err(error) => Err(error),
    }
}
