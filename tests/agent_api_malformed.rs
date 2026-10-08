//! The malformed-input corpus of `reshiki --mcp` and `reshiki --cli`
//! through the real binary (gate G5): malformed input never crashes the
//! binary or corrupts stdout, and the server keeps answering afterwards.
//!
//! tests/fixtures/agent-api/malformed/cases.json lists the MCP cases, and
//! README.md beside it documents the manifest, where each expected outcome
//! comes from and the committed fixtures. The cases run by phase:
//! - `modern` cases share one process; after each, `server/discover` with
//!   the full `_meta` must be answered;
//! - `legacy_post_init` cases share one process per revision, which first
//!   runs `initialize` and `notifications/initialized`; after each,
//!   `tools/list` must be answered;
//! - each `legacy_pre_init` case gets a fresh process per revision; after
//!   it, `initialize` with that revision must succeed.
//!
//! Each process may read `<tree>/in` and write `<tree>/out`, and gets
//! temporary RESHIKI_DATA_DIR, HOME, USERPROFILE, APPDATA and LOCALAPPDATA
//! folders inside the same tree; it inherits RESHIKI_INCHI_HELPER as CI sets
//! it (.github/workflows/checks.yml). Once its input ends it must exit 0
//! within 10 s, with every stdout line one JSON-RPC 2.0 object, no
//! `panicked at` on stderr and nothing created, removed or changed outside
//! `<tree>/out`.
#[path = "common/headless.rs"]
mod headless;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use headless::{McpSession, StderrMode};
use reshiki_agent::ops::budget::Budgets;
use reshiki_mcp::server::SUPPORTED;
use reshiki_model::{document::Document, pictures::Picture};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tempfile::TempDir;

const CASES: &str = include_str!("fixtures/agent-api/malformed/cases.json");
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/agent-api/malformed"
);
/// The revision served without `initialize`.
const MODERN: &str = "2026-07-28";
const LINE_TIMEOUT: Duration = Duration::from_secs(10);
/// A case's replies: long enough for a debug build to read a line of tens
/// of MiB or to lay out 32 molecules.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
const EXIT_TIMEOUT: Duration = Duration::from_secs(10);

/// One manifest entry; README.md documents each field.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    phase: Phase,
    #[serde(default)]
    revision: Option<String>,
    /// Runs only on this platform family when set ("unix").
    #[serde(default)]
    platform: Option<String>,
    /// The longest the replies may take, in milliseconds.
    #[serde(default)]
    within_ms: Option<u64>,
    /// How many times the case may be sent while every reply is a result.
    #[serde(default)]
    attempts: Option<u32>,
    source: String,
    lines: Value,
    expect: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Modern,
    LegacyPreInit,
    LegacyPostInit,
}

fn cases() -> Vec<Case> {
    let cases: Vec<Case> = serde_json::from_str(CASES).expect("cases.json parses");
    let mut names = BTreeSet::new();
    for case in &cases {
        assert!(names.insert(&case.name), "duplicate case {}", case.name);
        assert!(!case.source.is_empty(), "{}: no source", case.name);
        assert_eq!(
            case.revision.is_some(),
            case.phase != Phase::Modern,
            "{}: legacy cases and only they name a revision",
            case.name
        );
    }
    cases
}

/// Whether `case` runs on this platform.
fn runs_here(case: &Case) -> bool {
    match case.platform.as_deref() {
        None => true,
        Some("unix") => cfg!(unix),
        Some(other) => panic!("{}: unknown platform {other}", case.name),
    }
}

/// The advertised revisions, newest first.
fn supported() -> Vec<String> {
    SUPPORTED.iter().map(ToString::to_string).collect()
}

/// The revisions served through `initialize`.
fn legacy_revisions() -> Vec<String> {
    let supported = supported();
    assert_eq!(supported.first().map(String::as_str), Some(MODERN));
    supported.into_iter().skip(1).collect()
}

/// `revision`, or every legacy revision for "*".
fn expand(revision: &str) -> Vec<String> {
    if revision == "*" {
        legacy_revisions()
    } else {
        assert!(legacy_revisions().iter().any(|known| known == revision));
        vec![revision.to_owned()]
    }
}

/// A temporary tree holding the child's home, application data and data
/// folders, the granted folders `in` and `out`, and `outside`, which is not
/// granted.
struct Tree {
    root: TempDir,
}

impl Tree {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temporary tree");
        let tree = Self { root };
        create_private_folders(tree.root.path());
        for folder in ["in", "in/folder.mol", "out", "outside"] {
            fs::create_dir(tree.path(folder)).expect("create a folder");
        }
        fs::write(tree.path("outside/eth.mol"), "never read\n").expect("outside/eth.mol");
        #[cfg(unix)]
        {
            let made = std::process::Command::new("mkfifo")
                .arg(tree.path("in/pipe.mol"))
                .status()
                .expect("run mkfifo");
            assert!(made.success(), "mkfifo failed");
        }
        tree
    }

    /// `relative`, written with `/`, inside the tree.
    fn path(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.root.path().to_path_buf(), |path, name| path.join(name))
    }

    /// Starts `reshiki --mcp` granted `in` for reading and `out` for writing.
    fn start(&self) -> McpSession {
        let read = self.path("in");
        let write = self.path("out");
        let args = [
            "--allow-read",
            read.to_str().expect("a UTF-8 temporary path"),
            "--allow-write",
            write.to_str().expect("a UTF-8 temporary path"),
        ];
        let vars = private_env(self.root.path());
        McpSession::start_with_env(&args, &os_env(&vars), StderrMode::Captured)
    }

    /// Every path in the tree outside `out`, with each regular file's bytes.
    fn outside_out(&self) -> Snapshot {
        snapshot(self.root.path(), Some(&self.path("out")))
    }
}

/// The variables that point the child's data, home and application data
/// folders at folders inside `root`, so a snapshot of `root` sees what the
/// child writes there.
fn private_env(root: &Path) -> [(&'static str, PathBuf); 5] {
    [
        ("RESHIKI_DATA_DIR", root.join("data")),
        ("HOME", root.join("home")),
        ("USERPROFILE", root.join("home")),
        ("APPDATA", root.join("appdata")),
        ("LOCALAPPDATA", root.join("localappdata")),
    ]
}

/// Creates the folders [`private_env`] names, empty.
fn create_private_folders(root: &Path) {
    for (_, folder) in private_env(root) {
        fs::create_dir_all(folder).expect("create a private folder");
    }
}

/// `vars` as [`Command::env`](std::process::Command::env) pairs.
fn os_env<'a>(vars: &'a [(&'static str, PathBuf)]) -> Vec<(&'static str, &'a OsStr)> {
    vars.iter()
        .map(|(name, value)| (*name, value.as_os_str()))
        .collect()
}

/// What the `"${…}"` placeholders of a case's lines stand for.
struct Context<'a> {
    tree: &'a Tree,
    revision: Option<&'a str>,
}

impl Context<'_> {
    /// `text` with each `"${kind:argument}"`, quotes included, replaced by
    /// the JSON it stands for.
    fn substitute(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("\"${") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 3..];
            let end = after
                .find("}\"")
                .unwrap_or_else(|| panic!("unterminated placeholder in {text}"));
            out.push_str(&self.value(&after[..end]));
            rest = &after[end + 2..];
        }
        out.push_str(rest);
        out
    }

    fn value(&self, token: &str) -> String {
        let (kind, argument) = token.split_once(':').unwrap_or((token, ""));
        let path = |folder: &str| {
            json_string(
                self.tree
                    .path(&format!("{folder}/{argument}"))
                    .to_str()
                    .expect("a UTF-8 temporary path"),
            )
        };
        match kind {
            "meta" => headless::modern_meta().to_string(),
            "revision" => json_string(self.revision.expect("a legacy revision")),
            "in" => path("in"),
            "outside" => path("outside"),
            "fixture-text" => {
                json_string(&fs::read_to_string(fixture(argument)).expect("fixture text"))
            }
            "fixture-base64" => {
                json_string(&STANDARD.encode(fs::read(fixture(argument)).expect("fixture")))
            }
            "generated" => generated(argument),
            other => panic!("unknown placeholder {other}"),
        }
    }

    /// The bytes a case writes, in one write: each piece of `lines` ends in
    /// a newline, which is added when it has none.
    fn bytes(&self, lines: &Value) -> Vec<u8> {
        let pieces = match lines {
            Value::Array(pieces) => pieces.iter().collect(),
            piece => vec![piece],
        };
        let mut bytes = Vec::new();
        for piece in pieces {
            let mut line = match piece {
                Value::String(text) => self.substitute(text).into_bytes(),
                Value::Object(form) => match (form.get("base64"), form.get("json")) {
                    (Some(Value::String(base64)), None) if form.len() == 1 => {
                        STANDARD.decode(base64).expect("valid base64 lines")
                    }
                    (None, Some(message)) if form.len() == 1 => {
                        self.substitute(&message.to_string()).into_bytes()
                    }
                    _ => panic!("unknown lines form {piece}"),
                },
                other => panic!("unknown lines form {other}"),
            };
            if !line.ends_with(b"\n") {
                line.push(b'\n');
            }
            bytes.extend(line);
        }
        bytes
    }
}

fn json_string(text: &str) -> String {
    Value::from(text).to_string()
}

fn fixture(name: &str) -> PathBuf {
    Path::new(FIXTURES).join(name)
}

/// The JSON a `"${generated:…}"` placeholder stands for, made at test time.
fn generated(name: &str) -> String {
    let budgets = Budgets::default();
    match name {
        "depth-200" => format!("{}{}", "[".repeat(200), "]".repeat(200)),
        "compose-32" => headless::compose_32().to_string(),
        "cdx-over-budget" => json_string(&"A".repeat(budgets.max_cdx_base64 + 1)),
        "picture-30000" => json_string(&picture_drawing(30_000)),
        "objects-over-budget" => json_string(&atoms_drawing(budgets.max_objects + 1)),
        other => panic!("unknown generated payload {other}"),
    }
}

/// A grayscale PNG of `width`×`height`; with `pixels` None only the header
/// and a token IDAT chunk are written, so it declares a size it never holds.
fn png(width: u32, height: u32, pixels: Option<&[u8]>) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("PNG header");
    match pixels {
        Some(pixels) => writer.write_image_data(pixels).expect("PNG pixels"),
        None => writer
            .write_chunk(png::chunk::IDAT, &[0x78, 0x9c, 0x03, 0x00])
            .expect("PNG IDAT"),
    }
    writer.finish().expect("PNG end");
    bytes
}

/// A native drawing holding one picture whose PNG declares `side`×`side`
/// pixels.
fn picture_drawing(side: u32) -> String {
    let picture = Picture::import(&png(1, 1, Some(&[0]))).expect("a 1x1 picture");
    let mut drawing = serde_json::to_value(picture.document().current()).expect("serialize");
    drawing["graphics"][0]["picture"] = Value::from(STANDARD.encode(png(side, side, None)));
    drawing.to_string()
}

/// A native drawing of `count` carbon atoms, each with only the fields a
/// native atom requires, so the drawing stays within
/// `Budgets::max_text_bytes`.
fn atoms_drawing(count: usize) -> String {
    let mut drawing = serde_json::to_value(Document::default().current()).expect("serialize");
    drawing["atoms"] = (1..=count as u64)
        .map(|id| json!({"id": id, "element": "C", "position": {"x": 0, "y": 0}}))
        .collect();
    let text = drawing.to_string();
    assert!(text.len() <= Budgets::default().max_text_bytes);
    text
}

/// Whether `message` is what `expect` describes; README.md documents the
/// forms.
fn check(expect: &Value, message: &Value, supported: &[String]) -> Result<(), String> {
    let mismatch = |what: &str| Err(format!("{what}: expected {expect}"));
    let id = expect.get("id");
    if message.get("id") != id {
        return mismatch("id");
    }
    if let Some(code) = expect.get("error") {
        let error = &message["error"];
        if error["code"] != *code {
            return mismatch("error code");
        }
        if expect
            .get("message")
            .is_some_and(|text| error["message"] != *text)
        {
            return mismatch("error message");
        }
        if expect
            .get("data")
            .is_some_and(|data| error["data"] != *data)
        {
            return mismatch("error data");
        }
        // -32022 lists exactly the advertised revisions.
        if *code == -32022 && error["data"]["supported"] != json!(supported) {
            return mismatch("supported revisions");
        }
        return Ok(());
    }
    let Some(result) = message.get("result") else {
        return mismatch("a result");
    };
    if let Some(fields) = expect.get("result") {
        if result.get("isError") == Some(&Value::Bool(true)) {
            return mismatch("a result that is no tool error");
        }
        if let Value::Object(fields) = fields {
            if fields
                .iter()
                .any(|(name, value)| result.get(name) != Some(value))
            {
                return mismatch("result fields");
            }
        } else if *fields != Value::Bool(true) {
            panic!("bad result expectation {expect}");
        }
        return Ok(());
    }
    let Some(code) = expect.get("tool_error") else {
        panic!("unknown expectation {expect}");
    };
    if result["isError"] != true {
        return mismatch("a tool error");
    }
    let error = &result["structuredContent"]["error"];
    let text = error["message"].as_str().unwrap_or_default();
    if error["code"] != *code {
        return mismatch("tool error code");
    }
    if let Some(prefix) = expect["prefix"].as_str()
        && !text.starts_with(&format!("{prefix}: "))
    {
        return mismatch("tool error prefix");
    }
    if expect
        .get("message")
        .is_some_and(|expected| error["message"] != *expected)
    {
        return mismatch("tool error message");
    }
    if let Some(part) = expect.get("contains").and_then(Value::as_str)
        && !text.contains(part)
    {
        return mismatch("tool error message part");
    }
    Ok(())
}

/// A running process of one phase and what proves it still serves.
struct Process {
    tree: Tree,
    session: McpSession,
    before: Snapshot,
    phase: Phase,
    revision: Option<String>,
    sentinels: usize,
    supported: Vec<String>,
}

impl Process {
    fn start(phase: Phase, revision: Option<&str>) -> Self {
        let tree = Tree::new();
        let before = tree.outside_out();
        let session = tree.start();
        let mut process = Self {
            tree,
            session,
            before,
            phase,
            revision: revision.map(str::to_owned),
            sentinels: 0,
            supported: supported(),
        };
        if phase == Phase::LegacyPostInit {
            process.initialize("init");
            process.session.send(json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
            }));
        }
        process
    }

    fn context(&self) -> String {
        match &self.revision {
            Some(revision) => format!("{:?} {revision}", self.phase),
            None => format!("{:?}", self.phase),
        }
    }

    /// `initialize` with this process's revision, which must succeed.
    fn initialize(&mut self, id: &str) {
        let revision = self.revision.clone().expect("a legacy revision");
        self.session.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": revision,
                "capabilities": {},
                "clientInfo": {"name": "corpus", "version": "1.0.0"},
            },
        }));
        let reply = self.session.recv_for(Some(&json!(id)), REPLY_TIMEOUT);
        assert_eq!(
            reply["result"]["protocolVersion"],
            revision,
            "{}: {reply}",
            self.context()
        );
    }

    /// Asks the phase's sentinel question; the answer must arrive.
    fn sentinel(&mut self, case: &str) {
        self.sentinels += 1;
        let id = format!("sentinel-{}", self.sentinels);
        let context = format!("{}: {case}: sentinel", self.context());
        match self.phase {
            Phase::Modern => {
                self.session.send(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": "server/discover",
                    "params": {"_meta": headless::modern_meta()},
                }));
                let reply = self.session.recv_for(Some(&json!(id)), REPLY_TIMEOUT);
                assert_eq!(
                    reply["result"]["supportedVersions"],
                    json!(self.supported),
                    "{context}: {reply}"
                );
            }
            Phase::LegacyPostInit => {
                self.session
                    .send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/list"}));
                let reply = self.session.recv_for(Some(&json!(id)), REPLY_TIMEOUT);
                assert!(reply["result"]["tools"].is_array(), "{context}: {reply}");
            }
            Phase::LegacyPreInit => self.initialize(&id),
        }
    }

    /// Sends `case`'s lines, checks the replies it expects, then the
    /// sentinel; nothing else may have arrived by then. A case with
    /// `attempts` is sent again, after the sentinel, while every reply is a
    /// result and attempts are left.
    ///
    /// The watchdog restarts for each attempt with the time its send, its
    /// replies and the sentinel may each take, so a shared process lives as
    /// long as its cases need.
    fn run(&mut self, case: &Case) {
        let context = format!("{}: {}", self.context(), case.name);
        let expects: Vec<&Value> = match &case.expect {
            Value::Array(expects) => expects.iter().collect(),
            Value::String(word) if word == "silent" => Vec::new(),
            expect => vec![expect],
        };
        let bytes = Context {
            tree: &self.tree,
            revision: self.revision.as_deref(),
        }
        .bytes(&case.lines);
        let watchdog = REPLY_TIMEOUT * (expects.len() as u32 + 2);
        let attempts = case.attempts.unwrap_or(1);
        for attempt in 1..=attempts {
            self.session.rearm_watchdog(watchdog);
            let started = Instant::now();
            self.session.send_raw(&bytes);
            let mut replies: Vec<Value> = expects
                .iter()
                .map(|expect| self.session.recv_for(expect.get("id"), REPLY_TIMEOUT))
                .collect();
            if let Some(limit) = case.within_ms {
                let elapsed = started.elapsed();
                assert!(
                    elapsed <= Duration::from_millis(limit),
                    "{context}: replied after {elapsed:?}"
                );
            }
            let again =
                attempt < attempts && replies.iter().all(|reply| reply.get("result").is_some());
            if !again {
                // Replies sharing an id may arrive in any order.
                for expect in &expects {
                    let reasons: Vec<String> = replies
                        .iter()
                        .map(|reply| {
                            check(expect, reply, &self.supported)
                                .err()
                                .unwrap_or_default()
                        })
                        .collect();
                    let Some(index) = reasons.iter().position(String::is_empty) else {
                        panic!("{context}: attempt {attempt}: {reasons:?} in {replies:?}");
                    };
                    replies.remove(index);
                }
            }
            self.sentinel(&case.name);
            let unexpected = self.session.take_ready();
            assert_eq!(
                unexpected,
                Vec::<Value>::new(),
                "{context}: unexpected output"
            );
            if !again {
                return;
            }
        }
    }

    /// Ends the input: the process must exit 0 in time without writing
    /// anything else, panicking or touching anything outside `out`.
    fn finish(mut self) {
        let context = self.context();
        self.session.rearm_watchdog(EXIT_TIMEOUT + 2 * LINE_TIMEOUT);
        self.session.close_stdin();
        let status = self.session.wait_exit(EXIT_TIMEOUT);
        assert_eq!(status.code(), Some(0), "{context}");
        let rest = self.session.rest(LINE_TIMEOUT);
        assert_eq!(rest, Vec::<Value>::new(), "{context}: unexpected output");
        let stderr = self.session.stderr(LINE_TIMEOUT);
        assert!(!stderr.contains("panicked at"), "{context}: {stderr}");
        let after = self.tree.outside_out();
        assert!(
            after == self.before,
            "{context}: the tree changed outside out: {:?}",
            after.iter().map(|(path, _)| path).collect::<Vec<_>>()
        );
    }
}

#[test]
fn the_corpus_has_forty_cases_or_more() {
    assert!(cases().len() >= 40);
}

#[test]
fn modern_cases_share_one_process() {
    let cases = cases();
    let mut process = Process::start(Phase::Modern, None);
    for case in cases
        .iter()
        .filter(|case| case.phase == Phase::Modern && runs_here(case))
    {
        process.run(case);
    }
    process.finish();
}

#[test]
fn legacy_cases_after_initialize_share_one_process_per_revision() {
    let cases = cases();
    for revision in legacy_revisions() {
        let mut process = Process::start(Phase::LegacyPostInit, Some(&revision));
        for case in cases.iter().filter(|case| {
            case.phase == Phase::LegacyPostInit
                && runs_here(case)
                && case
                    .revision
                    .as_deref()
                    .is_some_and(|wanted| expand(wanted).contains(&revision))
        }) {
            process.run(case);
        }
        process.finish();
    }
}

#[test]
fn each_legacy_case_before_initialize_gets_a_fresh_process() {
    for case in cases()
        .iter()
        .filter(|case| case.phase == Phase::LegacyPreInit && runs_here(case))
    {
        for revision in expand(case.revision.as_deref().expect("a revision")) {
            let mut process = Process::start(Phase::LegacyPreInit, Some(&revision));
            process.run(case);
            process.finish();
        }
    }
}

/// Paths relative to a root, each regular file with its bytes.
type Snapshot = Vec<(PathBuf, Option<Vec<u8>>)>;

/// Every path under `root`, relative to it, with each regular file's bytes;
/// what the folder `skip` holds is left out. A FIFO is never opened.
fn snapshot(root: &Path, skip: Option<&Path>) -> Snapshot {
    let mut entries = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).expect("list a folder") {
            let path = entry.expect("a folder entry").path();
            let kind = fs::symlink_metadata(&path).expect("metadata").file_type();
            let bytes = kind
                .is_file()
                .then(|| fs::read(&path).expect("read a file"));
            entries.push((
                path.strip_prefix(root).expect("inside").to_path_buf(),
                bytes,
            ));
            if kind.is_dir() && skip != Some(path.as_path()) {
                folders.push(path);
            }
        }
    }
    entries.sort();
    entries
}

/// `reshiki --cli` refuses malformed command lines and inputs with exit 2
/// (usage) or 1 (failure) within 30 s, so a GUI started by mistake fails
/// by timeout, with nothing on stdout and no file created or changed, in
/// the working folder or in the data, home and application data folders,
/// which are inside it.
#[test]
fn cli_corpus() {
    let dir = tempfile::tempdir().expect("working folder");
    let root = dir.path();
    create_private_folders(root);
    let vars = private_env(root);
    let env = os_env(&vars);
    fs::create_dir(root.join("folder.smi")).expect("folder.smi");
    let over = Budgets::default().max_text_bytes + 1;
    fs::write(root.join("big.txt"), "C".repeat(over)).expect("big.txt");
    let drawing =
        r#"{"version":19,"atoms":[{"id":1,"element":"C","position":{"x":0,"y":0}}],"bonds":[]}"#;
    fs::write(root.join("in.rsk"), drawing).expect("in.rsk");
    fs::write(root.join("existing.svg"), "keep me\n").expect("existing.svg");
    let before = snapshot(root, None);
    let cases: [(&[&str], i32, &str); 8] = [
        (&["bogus"], 2, "reshiki: unknown command `bogus`"),
        (&["convert"], 2, "reshiki: convert needs an input"),
        (
            &["convert", "missing.mol", "-o", "out.mol"],
            1,
            "reshiki: error: cannot read missing.mol",
        ),
        (
            &["convert", "folder.smi", "-o", "out.mol"],
            1,
            "reshiki: error: cannot read folder.smi",
        ),
        (
            &["convert", "big.txt", "--from", "smiles", "-o", "out.mol"],
            1,
            "reshiki: error: input exceeds",
        ),
        (
            &["convert", "--smiles", "CCO", "--to", "xyz"],
            2,
            "reshiki: unsupported output format `xyz`",
        ),
        (
            &["convert", "in.rsk", "--to", "xyz", "-o", "out.xyz"],
            2,
            "reshiki: unsupported output format `xyz`",
        ),
        (
            &["convert", "in.rsk", "-o", "existing.svg"],
            1,
            "reshiki: error: existing.svg exists; pass --force to replace",
        ),
    ];
    for (args, code, stderr) in cases {
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        let output = headless::run_cli_with_env(root, &args, &env, b"", Duration::from_secs(30));
        let text = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(code), "{args:?}: {text}");
        assert!(
            output.stdout.is_empty(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(text.starts_with(stderr), "{args:?}: {text}");
        // Compared with assert!, so a failure does not print 16 MiB.
        assert!(
            snapshot(root, None) == before,
            "{args:?}: the working folder changed"
        );
    }
}
