//! Folder grants of `reshiki --mcp` through the real binary: command-line
//! grants and `agent-access.json`, folders that are refused at startup, and
//! what the server never does: ask the client for roots, honor a client's
//! project folder, or log granted paths below debug level.
//!
//! Each child gets temporary RESHIKI_DATA_DIR, HOME, USERPROFILE, APPDATA
//! and LOCALAPPDATA folders through `Command::env`, so neither the
//! developer's configuration nor their home folder take part, and runs under
//! a 60 s watchdog.
#[path = "common/headless.rs"]
mod headless;

use headless::{McpSession, StderrMode};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::TempDir;

const LINE_TIMEOUT: Duration = Duration::from_secs(10);
/// A refused start exits before it reads stdin.
const REFUSED_EXIT: Duration = Duration::from_secs(10);
const BANNER: &str = "reshiki-mcp: info: ReShiki agent API (experimental) ";

/// Ethanol as a V2000 molfile.
const ETHANOL: &str = "
  ReShiki

  3  2  0  0  0  0  0  0  0  0999 V2000
    0.0000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    1.2990    0.7500    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    2.5981    0.0000    0.0000 O   0  0  0  0  0  0  0  0  0  0  0  0
  1  2  1  0  0  0  0
  2  3  1  0  0  0  0
M  END
";

/// A temporary tree holding the child's home, application data and data
/// folders, and the folders `in` (with `eth.mol`) and `out` it may be
/// granted.
struct Tree {
    root: TempDir,
}

impl Tree {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temporary tree");
        for folder in ["home", "appdata", "localappdata", "data", "in", "out"] {
            fs::create_dir(root.path().join(folder)).expect("create a folder");
        }
        fs::write(root.path().join("in").join("eth.mol"), ETHANOL).expect("eth.mol");
        Self { root }
    }

    /// `relative`, written with `/`, inside the tree.
    fn path(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.root.path().to_path_buf(), |path, name| path.join(name))
    }

    /// `relative`'s absolute path as text, for arguments.
    fn text(&self, relative: &str) -> String {
        self.path(relative)
            .to_str()
            .expect("a UTF-8 temporary path")
            .to_owned()
    }

    /// The child's environment, with `data` as its data folder.
    fn env(&self, data: &str) -> Vec<(&'static str, PathBuf)> {
        vec![
            ("RESHIKI_DATA_DIR", self.path(data)),
            ("HOME", self.path("home")),
            ("USERPROFILE", self.path("home")),
            ("APPDATA", self.path("appdata")),
            ("LOCALAPPDATA", self.path("localappdata")),
        ]
    }

    /// The folder the child takes as its home. Windows reports the profile
    /// folder whatever USERPROFILE says.
    fn child_home(&self) -> PathBuf {
        if cfg!(windows) {
            reshiki_io::compatibility::home_directory().expect("a home folder")
        } else {
            self.path("home")
        }
    }

    /// Starts `reshiki --mcp args` in this tree with `data` as its data
    /// folder and the variables `extra`.
    fn start(&self, args: &[&str], data: &str, extra: &[(&'static str, PathBuf)]) -> Client {
        let vars: Vec<(&str, PathBuf)> = self
            .env(data)
            .into_iter()
            .chain(extra.iter().cloned())
            .collect();
        let env: Vec<(&str, &OsStr)> = vars
            .iter()
            .map(|(name, value)| (*name, value.as_os_str()))
            .collect();
        Client {
            session: McpSession::start_with_env(args, &env, StderrMode::Captured),
            seen: Vec::new(),
            next: 1,
        }
    }

    /// Starts `reshiki --mcp args`, which must refuse to start: exit 2 with
    /// stdin still open, nothing on stdout and one stderr line, returned.
    fn refused(&self, args: &[&str], data: &str) -> String {
        let mut client = self.start(args, data, &[]);
        let status = client.session.wait_exit(REFUSED_EXIT);
        assert_eq!(status.code(), Some(2), "{args:?}");
        assert_eq!(client.session.rest(LINE_TIMEOUT), Vec::<Value>::new());
        let stderr = client.session.stderr(LINE_TIMEOUT);
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
        stderr
    }
}

/// A 2026-07-28 client that keeps every message it received.
struct Client {
    session: McpSession,
    seen: Vec<Value>,
    next: i64,
}

impl Client {
    /// The result of a `tools/call`.
    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.session
            .send(headless::modern_call(id, tool, arguments));
        let reply = self.session.recv_for(Some(&json!(id)), LINE_TIMEOUT);
        self.seen.push(reply.clone());
        reply
            .get("result")
            .unwrap_or_else(|| panic!("{tool}: {reply}"))
            .clone()
    }

    /// The value of a call that must succeed.
    fn ok(&mut self, tool: &str, arguments: Value) -> Value {
        let result = self.call(tool, arguments);
        assert_eq!(result["isError"], false, "{tool}: {result}");
        result["structuredContent"]["value"].clone()
    }

    /// The `{code, message}` of a call that must fail.
    fn error(&mut self, tool: &str, arguments: Value) -> Value {
        let result = self.call(tool, arguments);
        assert_eq!(result["isError"], true, "{tool}: {result}");
        result["structuredContent"]["error"].clone()
    }

    /// Opens `path`; returns the document handle.
    fn open(&mut self, path: &str) -> String {
        let value = self.ok("file_open", json!({"path": path, "format": "auto"}));
        assert_eq!(value["source"]["path"], path, "{value}");
        value["document"].as_str().expect("a handle").to_owned()
    }

    fn save(&mut self, document: &str, path: &str, overwrite: bool) -> Value {
        let arguments = json!({
            "document": document,
            "path": path,
            "format": null,
            "overwrite": overwrite,
            "pages": null,
        });
        self.call("file_save", arguments)
    }

    /// Ends the input: the server must exit 0. Every message must be free
    /// of client requests; returns stderr.
    fn finish(mut self) -> String {
        self.session.close_stdin();
        assert_eq!(self.session.wait_exit(LINE_TIMEOUT).code(), Some(0));
        self.seen.extend(self.session.rest(LINE_TIMEOUT));
        assert_no_client_requests(&self.seen);
        self.session.stderr(LINE_TIMEOUT)
    }
}

/// The server never asks the client for its roots or for input: no line
/// names `roots/list` or carries an `input_required` result.
fn assert_no_client_requests(messages: &[Value]) {
    for message in messages {
        let line = message.to_string();
        assert!(
            !line.contains("roots/list") && !line.contains("input_required"),
            "{line}"
        );
    }
}

/// Asserts a `path_not_granted` access error.
fn assert_not_granted(error: &Value) {
    assert_eq!(error["code"], "access_denied", "{error}");
    let message = error["message"].as_str().expect("a message");
    assert!(message.starts_with("path_not_granted: "), "{message}");
}

type Inventory = BTreeMap<PathBuf, Option<Vec<u8>>>;

/// Every folder (None) and file (its bytes) under `root`, by relative path.
fn inventory(root: &Path) -> Inventory {
    let mut entries = Inventory::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).expect("list a folder") {
            let path = entry.expect("a folder entry").path();
            let relative = path.strip_prefix(root).expect("inside").to_path_buf();
            if path.is_dir() {
                entries.insert(relative, None);
                folders.push(path);
            } else {
                entries.insert(relative, Some(fs::read(&path).expect("read a file")));
            }
        }
    }
    entries
}

/// The paths added, removed or changed between two inventories.
fn changes(before: &Inventory, after: &Inventory) -> BTreeSet<PathBuf> {
    before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .cloned()
        .collect()
}

/// `--allow-read IN --allow-write OUT`: each folder serves only its own
/// direction, an existing file is kept, only OUT changes and the default
/// log level never names a granted folder.
#[test]
fn read_and_write_grants_are_separate_and_only_out_changes() {
    let tree = Tree::new();
    let (input, output) = (tree.text("in"), tree.text("out"));
    fs::write(tree.path("out").join("x.mol"), ETHANOL).expect("out/x.mol");
    let before = inventory(tree.root.path());
    let args = ["--allow-read", &input, "--allow-write", &output];
    let mut client = tree.start(&args, "data", &[]);

    let document = client.open(&tree.text("in/eth.mol"));
    let error = client.error(
        "file_open",
        json!({"path": tree.text("out/x.mol"), "format": "auto"}),
    );
    assert_not_granted(&error);

    let saved = client.save(&document, &tree.text("out/e.svg"), false);
    assert_eq!(saved["isError"], false, "{saved}");
    let receipt = &saved["structuredContent"]["value"]["receipt"];
    assert_eq!(receipt["format"], "svg", "{receipt}");
    assert_eq!(receipt["replaced"], false, "{receipt}");
    let svg = fs::read(tree.path("out/e.svg")).expect("out/e.svg");
    assert!(
        svg.starts_with(b"<svg"),
        "{}",
        String::from_utf8_lossy(&svg)
    );
    assert_eq!(receipt["byte_len"], svg.len());

    let refused = client.save(&document, &tree.text("in/e.svg"), false);
    assert_eq!(refused["isError"], true, "{refused}");
    assert_not_granted(&refused["structuredContent"]["error"]);

    let kept = client.save(&document, &tree.text("out/e.svg"), false);
    let error = &kept["structuredContent"]["error"];
    assert_eq!(error["code"], "failed", "{kept}");
    let message = error["message"].as_str().expect("a message");
    assert!(message.starts_with("file_exists: "), "{message}");
    assert_eq!(fs::read(tree.path("out/e.svg")).expect("out/e.svg"), svg);

    let stderr = client.finish();
    assert!(stderr.starts_with(BANNER), "{stderr}");
    // Every granted path contains the tree's unique folder name, which no
    // quoting or escaping of a path changes.
    let unique = tree.root.path().file_name().expect("a temporary folder");
    let unique = unique.to_str().expect("a UTF-8 temporary folder");
    assert!(!stderr.contains(unique), "{stderr}");
    let changed = changes(&before, &inventory(tree.root.path()));
    assert_eq!(changed, BTreeSet::from([Path::new("out").join("e.svg")]));
}

/// At debug level the startup banner lists the granted folders.
#[test]
fn debug_logging_lists_the_granted_folders() {
    let tree = Tree::new();
    let (input, output) = (tree.text("in"), tree.text("out"));
    let args = [
        "--allow-read",
        &input,
        "--allow-write",
        &output,
        "--log-level",
        "debug",
    ];
    let stderr = tree.start(&args, "data", &[]).finish();
    // The banner Debug-formats each folder, escaping Windows separators.
    for folder in [&input, &output] {
        assert!(stderr.contains(&format!("{folder:?}")), "{stderr}");
    }
}

/// A folder listed in `agent-access.json` is granted without flags.
#[test]
fn agent_access_json_grants_a_folder() {
    let tree = Tree::new();
    let config = json!({"version": 1, "read": [tree.text("in")]});
    fs::write(
        tree.path("data").join("agent-access.json"),
        config.to_string(),
    )
    .expect("agent-access.json");
    let mut client = tree.start(&[], "data", &[]);
    client.open(&tree.text("in/eth.mol"));
    client.finish();
}

#[test]
fn a_malformed_agent_access_json_exits_2_naming_it() {
    let tree = Tree::new();
    let config = tree.path("data").join("agent-access.json");
    fs::write(&config, "{\"version\": 1, \"read\": [").expect("agent-access.json");
    let stderr = tree.refused(&[], "data");
    assert!(stderr.contains(&config.display().to_string()), "{stderr}");
}

/// A FIFO in place of `agent-access.json` is refused without waiting for
/// a writer.
#[cfg(unix)]
#[test]
fn a_fifo_agent_access_json_exits_2_without_hanging() {
    let tree = Tree::new();
    let config = tree.path("data").join("agent-access.json");
    let made = std::process::Command::new("mkfifo")
        .arg(&config)
        .status()
        .expect("run mkfifo");
    assert!(made.success(), "mkfifo failed");
    let stderr = tree.refused(&[], "data");
    assert!(stderr.contains(&config.display().to_string()), "{stderr}");
    assert!(stderr.contains("not a regular file"), "{stderr}");
}

#[test]
fn granting_the_home_folder_exits_2() {
    let tree = Tree::new();
    let home = tree.child_home();
    let home = home.to_str().expect("a UTF-8 home folder");
    let stderr = tree.refused(&["--allow-write", home], "data");
    assert!(
        stderr.starts_with("reshiki --mcp: folder is too broad to grant: "),
        "{stderr}"
    );
}

/// The data folder is protected before it exists: its parent cannot be
/// granted either.
#[test]
fn the_parent_of_a_data_folder_not_yet_created_cannot_be_granted() {
    let tree = Tree::new();
    fs::create_dir(tree.path("parent")).expect("parent");
    let parent = tree.text("parent");
    let stderr = tree.refused(&["--allow-read", &parent], "parent/data");
    assert!(
        stderr.starts_with("reshiki --mcp: folder is protected and cannot be granted: "),
        "{stderr}"
    );
    assert!(!tree.path("parent/data").exists());
}

/// Claude Code sets CLAUDE_PROJECT_DIR for the stdio servers it starts
/// (https://code.claude.com/docs/en/mcp); it grants nothing.
#[test]
fn a_client_project_folder_grants_nothing() {
    let tree = Tree::new();
    fs::create_dir(tree.path("proj")).expect("proj");
    let before = inventory(tree.root.path());
    let project = [("CLAUDE_PROJECT_DIR", tree.path("proj"))];
    let mut client = tree.start(&[], "data", &project);
    let imported = client.ok("import", json!({"format": "smiles", "text": "CCO"}));
    let document = imported["document"].as_str().expect("a handle");
    let refused = client.save(document, &tree.text("proj/x.svg"), false);
    assert_eq!(refused["isError"], true, "{refused}");
    assert_not_granted(&refused["structuredContent"]["error"]);
    client.finish();
    assert_eq!(
        changes(&before, &inventory(tree.root.path())),
        BTreeSet::new()
    );
}
