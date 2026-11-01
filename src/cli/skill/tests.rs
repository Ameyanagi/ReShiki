use super::*;
use crate::cli::{FAILURE, SUCCESS, args, commands, run_with};
use reshiki_agent::{
    access::{AccessError, extensions::FILE_FORMATS},
    ops::{budget::Budgets, catalog::SPECS, documents::budgets_json, error::ErrorKind},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
const EXECUTABLE: &str = "/opt/reshiki/reshiki";
#[cfg(windows)]
const EXECUTABLE: &str = r"C:\ReShiki\reshiki.exe";

fn inputs() -> SkillInputs {
    SkillInputs::live(
        "reshiki".into(),
        "/opt/reshiki/reshiki".into(),
        "<app>".into(),
    )
}

async fn call(folder: &Path, flags: &[&str]) -> (i32, String, String) {
    let mut args: Vec<OsString> = vec![
        "skill".into(),
        "-o".into(),
        folder.into(),
        "--executable".into(),
        EXECUTABLE.into(),
    ];
    args.extend(flags.iter().map(OsString::from));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run_with(args, &mut out, &mut err).await;
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, files);
            } else {
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/");
                files.insert(name, fs::read(path).unwrap());
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files);
    files
}

#[tokio::test]
async fn writes_exactly_the_skill_files_with_empty_stdout() {
    let folder = tempfile::tempdir().unwrap();
    let (code, out, err) = call(folder.path(), &[]).await;
    assert_eq!(code, SUCCESS, "{err}");
    assert!(out.is_empty());
    assert!(err.is_empty());
    let root = folder.path().join("reshiki");
    let written = snapshot(&root);
    assert_eq!(written.keys().map(String::as_str).collect::<Vec<_>>(), {
        let mut expected = FILES.to_vec();
        expected.sort();
        expected
    });
    let live = SkillInputs::live(
        "reshiki".into(),
        EXECUTABLE.into(),
        crate::updates::CURRENT_VERSION.into(),
    );
    for (path, bytes) in files(&live) {
        assert_eq!(written[path], bytes, "{path}");
    }
    let (code, out, err) = call(folder.path(), &[]).await;
    assert_eq!(code, FAILURE);
    assert!(out.is_empty());
    assert_eq!(
        err,
        format!(
            "reshiki: error: {} exists; pass --force to replace\n",
            root.display()
        )
    );
    assert_eq!(snapshot(&root), written, "a second run writes nothing");
}

#[tokio::test]
async fn force_rewrites_generated_files_and_keeps_user_files() {
    let folder = tempfile::tempdir().unwrap();
    assert_eq!(call(folder.path(), &[]).await.0, SUCCESS);
    let root = folder.path().join("reshiki");
    let written = snapshot(&root);
    for path in FILES {
        fs::write(root.join(path), "stale").unwrap();
    }
    fs::write(root.join("user.txt"), "keep me").unwrap();
    let (code, out, err) = call(folder.path(), &["--force"]).await;
    assert_eq!(code, SUCCESS, "{err}");
    assert!(out.is_empty());
    let mut expected = written;
    expected.insert("user.txt".into(), b"keep me".to_vec());
    assert_eq!(snapshot(&root), expected);
}

#[tokio::test]
async fn receipt_is_one_labelled_line_after_success_with_a_custom_name() {
    let folder = tempfile::tempdir().unwrap();
    let (code, out, err) = call(folder.path(), &["--name", "chem-2", "--receipt"]).await;
    assert_eq!(code, SUCCESS, "{err}");
    assert_eq!(out.lines().count(), 1);
    let receipt: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(receipt["experimental"], true);
    assert_eq!(
        receipt["api"]["operation_api"],
        crate::envelope::OPERATION_API_VERSION
    );
    assert_eq!(
        receipt["skill"],
        json!({
            "path": folder.path().join("chem-2").to_string_lossy(), "name": "chem-2", "files": FILES,
        })
    );
    assert!(
        fs::read_to_string(folder.path().join("chem-2/SKILL.md"))
            .unwrap()
            .contains("name: \"chem-2\"")
    );
}

#[tokio::test]
async fn default_executable_is_used_as_returned_by_current_exe() {
    let folder = tempfile::tempdir().unwrap();
    let args = vec!["skill".into(), "-o".into(), folder.path().into()];
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(run_with(args, &mut out, &mut err).await, SUCCESS, "{err:?}");
    let executable = std::env::current_exe().unwrap();
    let live = SkillInputs::live(
        "reshiki".into(),
        executable.to_str().unwrap().into(),
        crate::updates::CURRENT_VERSION.into(),
    );
    assert_eq!(
        fs::read(folder.path().join("reshiki/SKILL.md")).unwrap(),
        render_skill_md(&live).as_bytes()
    );
}

#[tokio::test]
async fn missing_parent_or_a_file_parent_creates_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let missing = folder.path().join("missing");
    let (code, out, err) = call(&missing, &["--receipt"]).await;
    assert_eq!(code, FAILURE);
    assert!(out.is_empty());
    assert!(err.starts_with(&format!(
        "reshiki: error: cannot open the folder {}:",
        missing.display()
    )));
    assert!(!missing.exists());
    fs::write(&missing, "parent is a file").unwrap();
    assert_eq!(call(&missing, &[]).await.0, FAILURE);
    assert_eq!(fs::read(&missing).unwrap(), b"parent is a file");
}

#[tokio::test]
async fn a_file_at_the_skill_name_is_kept_even_with_force() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("reshiki");
    fs::write(&root, "user file").unwrap();
    for flags in [&[][..], &["--force"][..]] {
        let (code, out, err) = call(folder.path(), flags).await;
        assert_eq!(code, FAILURE, "{err}");
        assert!(out.is_empty());
        assert_eq!(fs::read(&root).unwrap(), b"user file");
    }
}

#[tokio::test]
async fn generated_directories_are_checked_before_any_file_is_written() {
    for directory in ["references", "assets", "assets/examples"] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("reshiki");
        fs::create_dir(&root).unwrap();
        if directory == "assets/examples" {
            fs::create_dir(root.join("assets")).unwrap();
        }
        fs::write(root.join(SKILL_MD), "old skill").unwrap();
        fs::write(root.join(directory), "not a directory").unwrap();
        let before = snapshot(&root);
        let (code, out, err) = call(folder.path(), &["--force", "--receipt"]).await;
        assert_eq!(code, FAILURE, "{directory}: {err}");
        assert!(out.is_empty());
        assert!(err.contains("is a symbolic link or not a folder"), "{err}");
        assert_eq!(snapshot(&root), before);
    }
}

#[tokio::test]
async fn force_creates_missing_subfolders_and_a_partial_failure_keeps_prior_files() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("reshiki");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("references")).unwrap();
    // Renaming a file over a directory fails after SKILL.md was written.
    fs::create_dir(root.join(CLI_REFERENCE)).unwrap();
    let (code, out, err) = call(folder.path(), &["--force", "--receipt"]).await;
    assert_eq!(code, FAILURE, "{err}");
    assert!(out.is_empty(), "no receipt after partial failure");
    assert!(root.join(SKILL_MD).is_file());
    assert!(root.join("assets/examples").is_dir());
    assert!(!root.join(TOOLS_REFERENCE).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_generated_directories_never_redirect_writes() {
    use std::os::unix::fs::symlink;
    for directory in ["", "references", "assets", "assets/examples"] {
        let folder = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("sentinel"), "untouched").unwrap();
        let root = folder.path().join("reshiki");
        let link = if directory.is_empty() {
            root.clone()
        } else {
            fs::create_dir(&root).unwrap();
            fs::write(root.join(SKILL_MD), "old skill").unwrap();
            if directory == "assets/examples" {
                fs::create_dir(root.join("assets")).unwrap();
            }
            root.join(directory)
        };
        symlink(outside.path(), &link).unwrap();
        for flags in [&[][..], &["--force", "--receipt"][..]] {
            let (code, out, err) = call(folder.path(), flags).await;
            assert_eq!(code, FAILURE, "{directory}: {err}");
            assert!(out.is_empty());
            assert_eq!(
                snapshot(outside.path()),
                BTreeMap::from([("sentinel".into(), b"untouched".to_vec())])
            );
            if !directory.is_empty() {
                assert_eq!(fs::read(root.join(SKILL_MD)).unwrap(), b"old skill");
            }
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn force_replaces_a_generated_file_symlink_and_accepts_the_named_parent_symlink() {
    use std::os::unix::fs::symlink;
    let folder = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("sentinel"), "untouched").unwrap();
    let parent = folder.path().join("parent");
    symlink(outside.path(), &parent).unwrap();
    assert_eq!(call(&parent, &[]).await.0, SUCCESS);
    let root = parent.join("reshiki");
    fs::remove_file(root.join(SKILL_MD)).unwrap();
    symlink(outside.path().join("sentinel"), root.join(SKILL_MD)).unwrap();
    assert_eq!(call(&parent, &["--force"]).await.0, SUCCESS);
    assert!(
        !fs::symlink_metadata(root.join(SKILL_MD))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read(outside.path().join("sentinel")).unwrap(),
        b"untouched"
    );
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent-skill/reshiki")
}

#[test]
fn golden_files_match_the_live_renderers() {
    let golden = snapshot(&fixture());
    assert_eq!(golden.len(), FILES.len());
    let rendered = files(&inputs());
    assert_eq!(
        rendered.iter().map(|(path, _)| *path).collect::<Vec<_>>(),
        FILES
    );
    for (path, bytes) in rendered {
        let expected = String::from_utf8(golden[path].clone())
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(
            bytes,
            expected.as_bytes(),
            "{path}: regenerate only for an intentional change"
        );
    }
    assert_eq!(
        golden[PROPOSAL_SCHEMA],
        include_bytes!("../../../tests/fixtures/agent-contract/proposal-schema.json")
    );
}

/// Rewrites the golden from the live renderers, for an intentional change
/// only (tests/fixtures/agent-skill/README.md).
#[test]
#[ignore = "rewrites tests/fixtures/agent-skill/reshiki"]
fn regenerate_the_golden() {
    for (path, bytes) in files(&inputs()) {
        let target = fixture().join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }
}

#[test]
fn generated_frontmatter_and_links_follow_the_portable_spec() {
    let text = render_skill_md(&inputs());
    let (frontmatter, _) = text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("\n---\n")
        .unwrap();
    let mut fields = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    for line in frontmatter.lines() {
        if line == "metadata:" {
            continue;
        }
        let (key, value) = line.split_once(": ").unwrap();
        let value: String = serde_json::from_str(value).expect("every value is a string");
        if let Some(key) = key.strip_prefix("  ") {
            assert!(metadata.insert(key, value).is_none());
        } else {
            assert!(fields.insert(key, value).is_none());
        }
    }
    assert_eq!(
        fields.keys().copied().collect::<Vec<_>>(),
        ["compatibility", "description", "license", "name"]
    );
    assert_eq!(
        metadata.keys().copied().collect::<Vec<_>>(),
        ["generated-by", "reshiki-operation-api", "reshiki-version"]
    );
    assert_eq!(fields["name"], "reshiki");
    assert!(args::valid_skill_name(&fields["name"]));
    assert!((1..=1024).contains(&fields["description"].chars().count()));
    assert!(!fields["description"].contains(['<', '>']));
    assert!(fields["compatibility"].chars().count() <= 500);
    assert!(!frontmatter.contains("/opt/reshiki/reshiki"));
    assert!(text.lines().count() <= 500);
    for link in text
        .split("](")
        .skip(1)
        .map(|rest| rest.split_once(')').unwrap().0)
    {
        assert!(FILES.contains(&link), "{link} resolves inside the skill");
        // References are one level below the root; the required example lives in assets/examples.
        assert!(link.split('/').count() <= 2 || link == PROPOSAL_EXAMPLE);
    }
}

#[test]
fn references_cover_the_live_catalog_limits_errors_and_formats() {
    let tools = references::tools();
    assert!(
        tools.contains("## Proposal\n"),
        "Proposal anchor resolves locally"
    );
    for spec in SPECS {
        assert!(
            tools.contains(&format!("\n## {}\n", spec.name)),
            "{}",
            spec.name
        );
    }
    let cli = references::cli();
    assert!(cli.contains(&crate::cli::help(None)));
    for spec in commands::COMMANDS {
        assert!(
            cli.contains(&format!("reshiki --cli {}\n", spec.name)),
            "{}",
            spec.name
        );
        assert!(
            cli.contains(&crate::cli::help(Some(spec))),
            "{} verbatim",
            spec.name
        );
    }
    let limits = references::limits();
    for kind in ErrorKind::ALL {
        assert!(limits.contains(kind.code()), "{}", kind.code());
    }
    for reason in AccessError::REASONS {
        assert!(limits.contains(reason.code), "{}", reason.code);
    }
    for format in FILE_FORMATS {
        assert!(limits.contains(&format!(".{}", format.ext)));
    }
    fn keys(value: &Value, text: &str) {
        for (key, value) in value.as_object().unwrap() {
            assert!(text.contains(key), "{key}");
            if value.is_object() {
                keys(value, text);
            }
        }
    }
    keys(&budgets_json(&Budgets::default()), &limits);
    for code in commands::catalog_json(None)["exit_codes"]
        .as_array()
        .unwrap()
    {
        assert!(limits.contains(&format!("| {} |", code["code"])));
    }
    assert!(limits.contains(crate::cli::heap::HEAP_VARIABLE));
}

/// Small splitter for the generated POSIX recipes: single-quoted words may
/// contain spaces, and escaped apostrophes can appear between quoted spans.
fn posix_words(line: &str) -> Vec<OsString> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => quoted = !quoted,
            '\\' if !quoted => word.push(chars.next().unwrap()),
            c if c.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    words.push(OsString::from(std::mem::take(&mut word)));
                }
            }
            c => word.push(c),
        }
    }
    assert!(!quoted);
    if !word.is_empty() {
        words.push(word.into());
    }
    words
}

#[test]
fn every_posix_recipe_parses_and_the_example_is_a_raw_compose_proposal() {
    let text = render_skill_md(&inputs());
    let mut count = 0;
    for line in text
        .lines()
        .filter(|line| line.starts_with("'/opt/reshiki/reshiki' --cli "))
    {
        let words = posix_words(line);
        let [_, cli, command, rest @ ..] = words.as_slice() else {
            panic!("{line}");
        };
        assert_eq!(cli, "--cli");
        assert!(args::parse(command, rest).is_ok(), "{line}");
        count += 1;
    }
    assert_eq!(count, 11);
    let example: Value = serde_json::from_str(include_str!("esterification.json")).unwrap();
    assert!(
        reshiki_agent::ops::catalog::decode_check(
            "compose",
            json!({
                "proposal": example, "style_document": null,
            })
        )
        .is_ok()
    );
}

/// Read a block with its actual fence delimiter, including longer fences
/// required when executable paths contain runs of backticks.
fn block(text: &str, language: &str) -> String {
    let mut lines = text.lines();
    let opening = lines
        .find(|line| line.starts_with("```") && line.trim_start_matches('`') == language)
        .unwrap();
    let delimiter = opening.trim_end_matches(language);
    let mut body = String::new();
    for line in lines {
        if line == delimiter {
            return body;
        }
        body.push_str(line);
        body.push('\n');
    }
    panic!("unclosed {language} block");
}

#[test]
fn setup_configs_preserve_hostile_executables_and_the_live_timeout() {
    for executable in [
        "/opt/reshiki/reshiki",
        r"C:\Program Files\O'Brien & Co; $env\reshiki.exe",
        "/opt/a```b\u{2019}c/reshiki",
    ] {
        let setup = references::setup(executable);
        let desktop: Value = serde_json::from_str(&block(&setup, "json")).unwrap();
        let expected_args = json!(["--mcp", "--allow-write", "AGENT_FOLDER"]);
        assert_eq!(desktop["mcpServers"]["reshiki"]["command"], executable);
        assert_eq!(desktop["mcpServers"]["reshiki"]["args"], expected_args);
        let toml = block(&setup, "toml");
        assert!(toml.starts_with("[mcp_servers.reshiki]\n"));
        let fields: BTreeMap<_, _> = toml
            .lines()
            .skip(1)
            .map(|line| line.split_once(" = ").unwrap())
            .collect();
        assert_eq!(
            serde_json::from_str::<String>(fields["command"]).unwrap(),
            executable
        );
        assert_eq!(
            serde_json::from_str::<Value>(fields["args"]).unwrap(),
            expected_args
        );
        assert_eq!(fields["startup_timeout_sec"], "20");
        assert_eq!(
            fields["tool_timeout_sec"],
            Budgets::default().op_deadline.as_secs().to_string()
        );
        assert_eq!(
            posix_words(&block(&setup, "sh")),
            [
                "claude",
                "mcp",
                "add",
                "--transport",
                "stdio",
                "--scope",
                "user",
                "reshiki",
                "--",
                executable,
                "--mcp",
                "--allow-write",
                "AGENT_FOLDER",
            ]
            .map(OsString::from)
        );
        let powershell = block(&setup, "powershell");
        assert!(
            !powershell.contains("-- & "),
            "the executable is an argument"
        );
        if executable.contains('\'') {
            assert!(powershell.contains("O''Brien"));
        }
        if executable.contains('\u{2019}') {
            assert!(powershell.contains("\u{2019}\u{2019}"));
        }
        assert_eq!(block(&setup, "text"), *crate::cli::MCP_USAGE);
    }
}
