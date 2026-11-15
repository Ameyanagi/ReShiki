//! A small GitHub in this process, for tests and for showing the real app
//! without the network. It keeps repositories as commits of `path → blob`,
//! hands out device codes and tokens, and logs every request.
use super::{client::Client, config::Endpoints, credentials::Tokens};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

pub const LOGIN: &str = "octo-test";
type Files = BTreeMap<String, String>;

/// An opaque 40-hex object name; real git hashing is not needed.
pub fn sha(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .take(20)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub struct Commit {
    pub tree: String,
    pub parent: Option<String>,
    pub message: String,
}

#[derive(Default)]
pub struct Repo {
    pub id: u64,
    pub owner: String,
    pub name: String,
    pub private: bool,
    pub default_branch: String,
    pub push: bool,
    pub refs: HashMap<String, String>,
    pub commits: HashMap<String, Commit>,
    /// Every tree, as the files below it.
    pub trees: HashMap<String, Files>,
    pub blobs: HashMap<String, Vec<u8>>,
}

impl Repo {
    fn put_blob(&mut self, bytes: &[u8]) -> String {
        let name = sha(bytes);
        self.blobs.insert(name.clone(), bytes.to_vec());
        name
    }
    fn put_tree(&mut self, files: Files) -> String {
        let name = sha(format!("tree {files:?}").as_bytes());
        self.trees.insert(name.clone(), files);
        name
    }
    fn put_commit(&mut self, tree: String, parent: Option<String>, message: &str) -> String {
        let name = sha(format!("commit {tree} {parent:?} {message}").as_bytes());
        let message = message.to_owned();
        self.commits.insert(
            name.clone(),
            Commit {
                tree,
                parent,
                message,
            },
        );
        name
    }
    pub fn head(&self) -> Option<String> {
        self.refs.get(&self.default_branch).cloned()
    }
    pub fn files_at(&self, commit: &str) -> Files {
        self.trees[&self.commits[commit].tree].clone()
    }
    /// The files on the default branch, as `path → bytes`.
    pub fn files(&self) -> BTreeMap<String, Vec<u8>> {
        let files = self
            .head()
            .map(|head| self.files_at(&head))
            .unwrap_or_default();
        files
            .into_iter()
            .map(|(path, blob)| (path, self.blobs[&blob].clone()))
            .collect()
    }
    /// Someone else's push: write or delete one file on the default branch.
    pub fn remote_commit(&mut self, path: &str, bytes: Option<&[u8]>) -> String {
        let head = self.head();
        let mut files = head
            .as_ref()
            .map(|head| self.files_at(head))
            .unwrap_or_default();
        match bytes {
            Some(bytes) => files.insert(path.to_owned(), self.put_blob(bytes)),
            None => files.remove(path),
        };
        let tree = self.put_tree(files);
        let commit = self.put_commit(tree, head, &format!("Remote change to {path}"));
        self.refs
            .insert(self.default_branch.clone(), commit.clone());
        commit
    }
    /// Commits from `from` back to the first, newest first.
    fn history(&self, from: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut next = Some(from.to_owned());
        while let Some(commit) = next.filter(|commit| self.commits.contains_key(commit)) {
            next = self.commits[&commit].parent.clone();
            out.push(commit);
        }
        out
    }
    fn json(&self) -> Value {
        json!({
            "id": self.id, "name": self.name, "owner": { "login": self.owner },
            "private": self.private, "default_branch": self.default_branch,
            "permissions": { "push": self.push },
        })
    }
}

pub struct Logged {
    pub method: String,
    /// With its query, as sent.
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

struct Fault {
    method: String,
    path: String,
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: &'static str,
    /// Do the work, then lose the reply.
    apply: bool,
}

#[derive(Default)]
pub struct Model {
    /// Answers to the next polls: `pending`, `slow_down`, `expired`,
    /// `denied`, `incorrect`, `ok`. Empty: pending.
    pub script: VecDeque<&'static str>,
    /// Approve a code by itself after this many polls.
    pub approve_after: Option<usize>,
    polls: usize,
    issued: u32,
    pub access: HashSet<String>,
    pub refresh: HashSet<String>,
    pub repos: Vec<Repo>,
    pub starred: HashSet<String>,
    pub log: Vec<Logged>,
    faults: Vec<Fault>,
    /// `(method, path, repo, file, bytes)`: someone else's push that lands
    /// just before the next such request.
    pub races: Vec<(
        &'static str,
        String,
        &'static str,
        &'static str,
        Option<Vec<u8>>,
    )>,
}

impl Model {
    pub fn add_repo(&mut self, name: &str) -> &mut Repo {
        let id = self.repos.len() as u64 + 1;
        self.repos.push(Repo {
            id,
            owner: LOGIN.into(),
            name: name.into(),
            private: true,
            default_branch: "main".into(),
            push: true,
            ..Repo::default()
        });
        self.repos.last_mut().unwrap()
    }
    pub fn repo(&mut self, name: &str) -> &mut Repo {
        self.repos
            .iter_mut()
            .find(|repo| repo.name == name)
            .unwrap()
    }
    /// `octo-test/lab-notes`, private, with two drawings and a preview.
    pub fn seed_demo(&mut self) {
        let ring = include_bytes!("../../website/public/examples/benzofuran.rsk");
        let route = include_bytes!("../../website/public/examples/esterification.rsk");
        let repo = self.add_repo("lab-notes");
        repo.remote_commit("ring.rsk", Some(ring));
        repo.remote_commit(
            "ring.svg",
            Some(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
        );
        repo.remote_commit("scheme/route.rsk", Some(route));
    }
    pub fn issue(&mut self) -> (String, String) {
        self.issued += 1;
        let pair = (
            format!("access-{}", self.issued),
            format!("refresh-{}", self.issued),
        );
        self.access.insert(pair.0.clone());
        self.refresh.insert(pair.1.clone());
        pair
    }
    /// Answer the next `method path` (no query) with `status`.
    pub fn fail_next(&mut self, method: &str, path: &str, status: u16) {
        self.fault(method, path, status, vec![], "{}", false);
    }
    pub fn fail_with(
        &mut self,
        method: &str,
        path: &str,
        status: u16,
        headers: Vec<(&'static str, String)>,
        body: &'static str,
    ) {
        self.fault(method, path, status, headers, body, false);
    }
    /// Do the next `method path`, but answer 500: a reply lost on the way.
    pub fn apply_then_fail(&mut self, method: &str, path: &str) {
        self.fault(method, path, 500, vec![], "{}", true);
    }
    fn fault(
        &mut self,
        method: &str,
        path: &str,
        status: u16,
        headers: Vec<(&'static str, String)>,
        body: &'static str,
        apply: bool,
    ) {
        let (method, path) = (method.into(), path.into());
        self.faults.push(Fault {
            method,
            path,
            status,
            headers,
            body,
            apply,
        });
    }
    /// How many `method` requests went to a path starting with `prefix`.
    pub fn count(&self, method: &str, prefix: &str) -> usize {
        let hit = |one: &&Logged| one.method == method && one.path.starts_with(prefix);
        self.log.iter().filter(hit).count()
    }
}

struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

fn reply(status: u16, body: Value) -> Reply {
    Reply {
        status,
        headers: vec![],
        body: body.to_string().into_bytes(),
    }
}
fn raw(body: Vec<u8>) -> Reply {
    Reply {
        status: 200,
        headers: vec![],
        body,
    }
}
fn missing() -> Reply {
    reply(404, json!({ "message": "Not Found" }))
}

fn decode(text: &str) -> String {
    let mut out = Vec::new();
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hex: String = bytes.by_ref().take(2).map(char::from).collect();
            out.push(u8::from_str_radix(&hex, 16).unwrap());
        } else {
            out.push(byte);
        }
    }
    String::from_utf8(out).unwrap()
}

fn respond(model: &mut Model, base: &str, request: Logged) -> Reply {
    let method = request.method.clone();
    let full = request.path.clone();
    let token = request
        .headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer "));
    let signed_in = token.is_some_and(|token| model.access.contains(token));
    let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
    model.log.push(request);

    let (path, query) = full.split_once('?').unwrap_or((&full, ""));
    let query: HashMap<String, String> = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(name, value)| (name.to_owned(), decode(value)))
        .collect();
    let fault = model
        .faults
        .iter()
        .position(|f| f.method == method && f.path == path);
    let fault = fault.map(|at| model.faults.remove(at));
    if let Some(fault) = fault.as_ref().filter(|fault| !fault.apply) {
        let (status, headers) = (fault.status, fault.headers.clone());
        return Reply {
            status,
            headers,
            body: fault.body.into(),
        };
    }
    if let Some(at) = model
        .races
        .iter()
        .position(|race| race.0 == method && race.1 == path)
    {
        let (.., repo, file, bytes) = model.races.remove(at);
        model.repo(repo).remote_commit(file, bytes.as_deref());
    }
    let parts: Vec<String> = path.split('/').skip(1).map(decode).collect();
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    let answer = match (method.as_str(), parts.as_slice()) {
        ("POST", ["login", "device", "code"]) => reply(
            200,
            json!({
                "device_code": "device-1", "user_code": "ABCD-1234",
                "verification_uri": format!("{base}/login/device"),
                "expires_in": 900, "interval": 5,
            }),
        ),
        ("GET", ["login", "device"]) => {
            model.script.push_back("ok");
            raw(b"<h1>Approved</h1>".to_vec())
        }
        ("POST", ["login", "oauth", "access_token"]) => token_reply(model, &body),
        ("POST", ["_mock", "commit"]) => {
            let bytes = body["content"]
                .as_str()
                .map(|text| text.as_bytes().to_vec());
            let repo = model.repo(body["repo"].as_str().unwrap());
            repo.remote_commit(body["path"].as_str().unwrap(), bytes.as_deref());
            reply(200, json!({}))
        }
        (_, ["api", rest @ ..]) if signed_in => api(model, &method, rest, &query, &body),
        (_, ["api", ..]) => reply(401, json!({ "message": "Bad credentials" })),
        _ => missing(),
    };
    match fault {
        Some(_) => reply(500, json!({})),
        None => answer,
    }
}

fn token_reply(model: &mut Model, body: &Value) -> Reply {
    let granted = |model: &mut Model| {
        let (access, refresh) = model.issue();
        reply(
            200,
            json!({
                "access_token": access, "token_type": "bearer", "scope": "",
                "expires_in": 28800, "refresh_token": refresh, "refresh_token_expires_in": 15897600,
            }),
        )
    };
    if body["grant_type"] == "refresh_token" {
        return if model
            .refresh
            .remove(body["refresh_token"].as_str().unwrap_or_default())
        {
            granted(model)
        } else {
            reply(200, json!({ "error": "bad_refresh_token" }))
        };
    }
    model.polls += 1;
    let approved = model.approve_after.is_some_and(|after| model.polls > after);
    match model
        .script
        .pop_front()
        .unwrap_or(if approved { "ok" } else { "pending" })
    {
        "ok" => granted(model),
        "slow_down" => reply(200, json!({ "error": "slow_down", "interval": 10 })),
        "expired" => reply(200, json!({ "error": "expired_token" })),
        "denied" => reply(200, json!({ "error": "access_denied" })),
        "incorrect" => reply(200, json!({ "error": "incorrect_device_code" })),
        _ => reply(200, json!({ "error": "authorization_pending" })),
    }
}

fn api(
    model: &mut Model,
    method: &str,
    parts: &[&str],
    query: &HashMap<String, String>,
    body: &Value,
) -> Reply {
    let number =
        |name: &str, default: usize| query.get(name).map_or(default, |n| n.parse().unwrap());
    match (method, parts) {
        ("GET", ["user"]) => reply(200, json!({ "login": LOGIN })),
        ("GET", ["user", "installations"]) => {
            let installations = if model.repos.is_empty() {
                json!([])
            } else {
                json!([{ "id": 1 }])
            };
            reply(
                200,
                json!({ "total_count": 1, "installations": installations }),
            )
        }
        ("GET", ["user", "installations", "1", "repositories"]) => {
            let (size, page) = (number("per_page", 30), number("page", 1));
            let repos: Vec<Value> = model
                .repos
                .iter()
                .skip((page - 1) * size)
                .take(size)
                .map(Repo::json)
                .collect();
            reply(
                200,
                json!({ "total_count": model.repos.len(), "repositories": repos }),
            )
        }
        ("GET", ["repositories", id]) => {
            match model.repos.iter().find(|repo| repo.id.to_string() == *id) {
                Some(repo) => reply(200, repo.json()),
                None => missing(),
            }
        }
        ("POST", ["user", "repos"]) => {
            let name = body["name"].as_str().unwrap();
            if model.repos.iter().any(|repo| repo.name == name) {
                return reply(
                    422,
                    json!({ "message": "name already exists on this account" }),
                );
            }
            let repo = model.add_repo(name);
            repo.private = body["private"] == true;
            if body["auto_init"] == true {
                repo.remote_commit("README.md", Some(b"# new\n"));
            }
            reply(201, repo.json())
        }
        ("PUT", ["user", "starred", owner, repo]) => {
            model.starred.insert(format!("{owner}/{repo}"));
            Reply {
                status: 204,
                headers: vec![],
                body: vec![],
            }
        }
        (_, ["repos", owner, name, rest @ ..]) => {
            let found = model
                .repos
                .iter_mut()
                .find(|r| r.owner == *owner && r.name == *name);
            match found {
                Some(repo) => repo_api(repo, method, rest, query, body),
                None => missing(),
            }
        }
        _ => missing(),
    }
}

fn repo_api(
    repo: &mut Repo,
    method: &str,
    parts: &[&str],
    query: &HashMap<String, String>,
    body: &Value,
) -> Reply {
    let text = |name: &str| body[name].as_str().unwrap().to_owned();
    match (method, parts) {
        ("GET", ["git", "ref", "heads", branch]) => match repo.refs.get(*branch) {
            Some(commit) => reply(200, json!({ "object": { "sha": commit } })),
            None if repo.commits.is_empty() => {
                reply(409, json!({ "message": "Git Repository is empty." }))
            }
            None => missing(),
        },
        ("GET", ["git", "commits", commit]) => match repo.commits.get(*commit) {
            Some(commit) => reply(200, json!({ "tree": { "sha": commit.tree } })),
            None => missing(),
        },
        ("GET", ["git", "trees", tree]) => {
            let Some(files) = repo.trees.get(*tree).cloned() else {
                return missing();
            };
            // The folder's own entries: files, and one tree per folder below.
            let mut folders: BTreeMap<String, Files> = BTreeMap::new();
            let mut items = Vec::new();
            for (path, blob) in files {
                match path.split_once('/') {
                    Some((folder, rest)) => {
                        folders
                            .entry(folder.into())
                            .or_default()
                            .insert(rest.into(), blob);
                    }
                    None => items.push(
                        json!({ "path": path, "mode": "100644", "type": "blob", "sha": blob }),
                    ),
                }
            }
            for (folder, files) in folders {
                let sha = repo.put_tree(files);
                items.push(json!({ "path": folder, "mode": "040000", "type": "tree", "sha": sha }));
            }
            reply(
                200,
                json!({ "sha": tree, "tree": items, "truncated": false }),
            )
        }
        ("GET", ["git", "blobs", blob]) => match repo.blobs.get(*blob) {
            Some(bytes) => raw(bytes.clone()),
            None => missing(),
        },
        ("POST", ["git", "blobs"]) => {
            let bytes = STANDARD.decode(text("content")).unwrap();
            reply(201, json!({ "sha": repo.put_blob(&bytes) }))
        }
        ("POST", ["git", "trees"]) => {
            let mut files = repo.trees[body["base_tree"].as_str().unwrap()].clone();
            for item in body["tree"].as_array().unwrap() {
                let (path, sha) = (
                    item["path"].as_str().unwrap(),
                    item["sha"].as_str().unwrap(),
                );
                files.insert(path.into(), sha.into());
            }
            reply(201, json!({ "sha": repo.put_tree(files) }))
        }
        ("POST", ["git", "commits"]) => {
            let parent = body["parents"][0].as_str().map(str::to_owned);
            reply(
                201,
                json!({ "sha": repo.put_commit(text("tree"), parent, &text("message")) }),
            )
        }
        ("PATCH", ["git", "refs", "heads", branch]) => {
            let (commit, head) = (text("sha"), repo.refs.get(*branch).cloned());
            if !head.is_some_and(|head| repo.history(&commit).contains(&head)) {
                return reply(422, json!({ "message": "Update is not a fast forward" }));
            }
            repo.refs.insert((*branch).into(), commit.clone());
            reply(200, json!({ "object": { "sha": commit } }))
        }
        ("PUT", ["contents", "README.md"]) => {
            let bytes = STANDARD.decode(text("content")).unwrap();
            let commit = repo.remote_commit("README.md", Some(&bytes));
            reply(201, json!({ "commit": { "sha": commit } }))
        }
        ("GET", ["contents", path @ ..]) => {
            let files = repo
                .commits
                .get(&query["ref"])
                .map(|commit| &repo.trees[&commit.tree]);
            match files.and_then(|files| files.get(&path.join("/"))) {
                Some(blob) => raw(repo.blobs[blob].clone()),
                None => missing(),
            }
        }
        ("GET", ["commits"]) => {
            let (path, page) = (&query["path"], query["page"].parse::<usize>().unwrap());
            let size: usize = query["per_page"].parse().unwrap();
            let blob_at =
                |commit: &String| repo.trees[&repo.commits[commit].tree].get(path).cloned();
            let touched = |commit: &String| {
                let before = repo.commits[commit].parent.as_ref().and_then(blob_at);
                before != blob_at(commit)
            };
            let items: Vec<Value> = repo
                .history(&query["sha"])
                .iter()
                .filter(|commit| touched(commit))
                .skip((page - 1) * size)
                .take(size)
                .map(|commit| {
                    json!({
                        "sha": commit,
                        "commit": {
                            "message": repo.commits[commit].message,
                            "author": { "name": "Octo Test", "date": "2026-11-15T14:02:00Z" },
                        },
                        "author": { "login": LOGIN },
                    })
                })
                .collect();
            reply(200, json!(items))
        }
        _ => missing(),
    }
}

async fn serve(mut stream: TcpStream, model: Arc<Mutex<Model>>, base: String) {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    let request = loop {
        let read = stream.read(&mut buffer).await.unwrap();
        bytes.extend_from_slice(&buffer[..read]);
        let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            assert_ne!(read, 0, "The request ended inside its headers");
            continue;
        };
        let head = String::from_utf8(bytes[..end].to_vec()).unwrap();
        let mut lines = head.lines();
        let mut first = lines.next().unwrap().split(' ');
        let (method, path) = (first.next().unwrap().into(), first.next().unwrap().into());
        let headers: HashMap<String, String> = lines
            .filter_map(|line| line.split_once(": "))
            .map(|(name, value)| (name.to_lowercase(), value.to_owned()))
            .collect();
        let length = headers
            .get("content-length")
            .map_or(0, |n| n.parse::<usize>().unwrap());
        if bytes.len() < end + 4 + length {
            assert_ne!(read, 0, "The request ended inside its body");
            continue;
        }
        let body = bytes[end + 4..end + 4 + length].to_vec();
        break Logged {
            method,
            path,
            headers,
            body,
        };
    };
    let reply = respond(&mut model.lock().unwrap(), &base, request);
    let mut head = format!(
        "HTTP/1.1 {} Mock\r\nContent-Length: {}\r\nConnection: close\r\n",
        reply.status,
        reply.body.len()
    );
    for (name, value) in &reply.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    // A client that stopped listening (an over-limit reply) is not an error.
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(&reply.body).await;
    let _ = stream.shutdown().await;
}

pub struct Mock {
    pub model: Arc<Mutex<Model>>,
    pub base: String,
}

impl Mock {
    /// Listen on this computer; port 0 picks a free one.
    pub async fn start(port: u16) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let model = Arc::new(Mutex::new(Model::default()));
        let (shared, address) = (model.clone(), base.clone());
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                tokio::spawn(serve(stream, shared.clone(), address.clone()));
            }
        });
        Self { model, base }
    }
    pub fn model(&self) -> std::sync::MutexGuard<'_, Model> {
        self.model.lock().unwrap()
    }
    pub fn ends(&self) -> Endpoints {
        Endpoints {
            api: format!("{}/api", self.base),
            web: self.base.clone(),
            overridden: true,
        }
    }
    /// A sign-in the mock accepts, good for eight hours.
    pub fn tokens(&self) -> Tokens {
        let (access, refresh) = self.model().issue();
        let now = super::auth::unix_now();
        Tokens {
            login: LOGIN.into(),
            access,
            access_expires_at: Some(now + 28800),
            refresh: Some(refresh),
            refresh_expires_at: Some(now + 15897600),
        }
    }
    pub fn client(&self) -> Client {
        Client::new("mock-client".into(), self.ends(), Some(self.tokens())).unwrap()
    }
}

/// The mock for a real app run:
/// `RESHIKI_GITHUB_CLIENT_ID=mock RESHIKI_GITHUB_API=http://127.0.0.1:47123/api
/// RESHIKI_GITHUB_WEB=http://127.0.0.1:47123 RESHIKI_DATA_DIR=<fresh folder>`.
/// A code is approved after two polls. `POST /_mock/commit` with
/// `{"repo", "path", "content"}` makes someone else's change (`content: null`
/// deletes).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Serve the mock for GUI evidence"]
async fn serve_github_mock() {
    let port =
        std::env::var("RESHIKI_GITHUB_MOCK_PORT").map_or(47123, |port| port.parse().unwrap());
    let mock = Mock::start(port).await;
    mock.model().seed_demo();
    mock.model().approve_after = Some(2);
    println!("Mock GitHub at {}", mock.base);
    std::future::pending::<()>().await;
}
