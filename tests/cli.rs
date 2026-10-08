//! `reshiki --cli convert` and `analyze` through the real binary: the
//! stdout/stderr and exit-code contract.
//!
//! Every run has an empty data folder, a temporary working folder and a
//! watchdog, and inherits RESHIKI_INCHI_HELPER as CI sets it
//! (.github/workflows/checks.yml).
#[path = "common/headless.rs"]
mod headless;

use std::{fs, path::Path, process::Output, time::Duration};
use tempfile::TempDir;

/// Long enough for a debug build's first chemistry engine call on CI.
const CONVERT: Duration = Duration::from_secs(120);
/// A usage error exits before any work.
const USAGE: Duration = Duration::from_secs(30);

fn cli(dir: &Path, args: &[&str]) -> Output {
    headless::run_cli(dir, args, b"", CONVERT)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Asserts a usage error: exit 2, nothing on stdout, and no file created.
fn assert_usage(dir: &TempDir, args: &[&str]) -> String {
    let before = files(dir);
    let output = headless::run_cli(dir.path(), args, b"", USAGE);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{args:?}: {stderr}");
    assert!(
        output.stdout.is_empty(),
        "{args:?}: {}",
        text(&output.stdout)
    );
    assert_eq!(files(dir), before, "{args:?} created a file");
    stderr
}

fn files(dir: &TempDir) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir.path())
        .expect("list the folder")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into())
        .collect();
    names.sort();
    names
}

/// The working folder with `eth.mol`, made by `convert --smiles CCO -o
/// eth.mol`, which leaves stdout empty.
fn with_ethanol() -> TempDir {
    let dir = tempfile::tempdir().expect("working folder");
    let output = cli(dir.path(), &["convert", "--smiles", "CCO", "-o", "eth.mol"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
    let molfile = fs::read_to_string(dir.path().join("eth.mol")).expect("eth.mol");
    assert!(molfile.contains("M  END"), "{molfile}");
    dir
}

#[test]
fn convert_writes_a_file_or_stdout() {
    let dir = with_ethanol();
    let output = cli(dir.path(), &["convert", "eth.mol", "--to", "mol"]);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(text(&output.stdout).contains("M  END"));
    assert!(!stderr.contains("reshiki: error"), "{stderr}");
    assert_eq!(files(&dir), ["eth.mol"]);
}

#[test]
fn convert_reads_a_pipeline_from_stdin() {
    let dir = tempfile::tempdir().expect("working folder");
    let first = cli(dir.path(), &["convert", "--smiles", "CCO", "--to", "mol"]);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    let second = headless::run_cli(
        dir.path(),
        &["convert", "-", "--from", "mol", "-o", "eth.svg"],
        &first.stdout,
        CONVERT,
    );
    assert_eq!(second.status.code(), Some(0), "{}", text(&second.stderr));
    assert!(second.stdout.is_empty(), "{}", text(&second.stdout));
    let svg = fs::read(dir.path().join("eth.svg")).expect("eth.svg");
    assert!(svg.starts_with(b"<svg"), "{}", text(&svg));
}

#[test]
fn convert_usage_errors_exit_2() {
    let dir = with_ethanol();
    assert_usage(&dir, &["convert", "eth.mol"]);
    let stdin = assert_usage(&dir, &["convert", "-"]);
    assert!(stdin.contains("needs --from"), "{stdin}");
    for output in ["x.rsk", "x.emf", "x.cdx"] {
        let stderr = assert_usage(&dir, &["convert", "eth.mol", "-o", output]);
        assert!(stderr.contains("unsupported output format"), "{stderr}");
    }
    assert_usage(&dir, &["convert", "eth.mol", "--pages", "--to", "png"]);
}

#[test]
fn an_existing_output_is_replaced_only_with_force() {
    let dir = with_ethanol();
    let kept = dir.path().join("kept.mol");
    fs::write(&kept, b"keep me").expect("existing output");
    let args = ["convert", "eth.mol", "-o", "kept.mol"];
    let output = cli(dir.path(), &args);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains("exists; pass --force to replace"),
        "{stderr}"
    );
    assert_eq!(fs::read(&kept).expect("kept.mol"), b"keep me");
    assert_eq!(
        files(&dir),
        ["eth.mol", "kept.mol"],
        "no temporary file is left"
    );
    let output = cli(dir.path(), &[&args[..], &["--force"][..]].concat());
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    let replaced = fs::read_to_string(&kept).expect("kept.mol");
    assert!(replaced.contains("M  END"), "{replaced}");
}

#[test]
fn unreadable_inputs_exit_1() {
    let dir = tempfile::tempdir().expect("working folder");
    fs::create_dir(dir.path().join("folder.mol")).expect("folder");
    for input in ["missing.mol", "folder.mol"] {
        let output = cli(dir.path(), &["convert", input, "--to", "mol"]);
        let stderr = text(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{input}: {stderr}");
        assert!(output.stdout.is_empty(), "{input}");
        assert!(
            stderr.starts_with("reshiki: error: cannot read"),
            "{stderr}"
        );
    }
}

#[test]
fn a_receipt_is_the_only_stdout_line() {
    let dir = tempfile::tempdir().expect("working folder");
    let output = cli(
        dir.path(),
        &["convert", "--smiles", "CCO", "-o", "eth.svg", "--receipt"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let line = stdout.strip_suffix('\n').expect("one line");
    assert!(!line.contains('\n'), "{stdout}");
    let receipt: serde_json::Value = serde_json::from_str(line).expect("receipt JSON");
    assert_eq!(receipt["receipt"]["format"], "svg");
    let size = fs::metadata(dir.path().join("eth.svg"))
        .expect("eth.svg")
        .len();
    assert_eq!(receipt["receipt"]["byte_len"], size);
    assert_eq!(receipt["api"]["stability"], "experimental");
}

#[test]
fn analyze_prints_one_json_line() {
    let dir = tempfile::tempdir().expect("working folder");
    let output = cli(dir.path(), &["analyze", "--smiles", "CCO"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let line = stdout.strip_suffix('\n').expect("one line");
    assert!(!line.contains('\n'), "{stdout}");
    let result: serde_json::Value = serde_json::from_str(line).expect("analyze JSON");
    assert_eq!(result["value"]["analysis"]["formula"], "C2H6O");
    assert_eq!(result["api"]["stability"], "experimental");
    let to = assert_usage(&dir, &["analyze", "--smiles", "CCO", "--to", "mol"]);
    assert!(to.contains("analyze does not take --to"), "{to}");
}

#[test]
fn unknown_commands_exit_2_quickly() {
    let dir = tempfile::tempdir().expect("working folder");
    let stderr = assert_usage(&dir, &["bogus"]);
    assert!(stderr.contains("unknown command `bogus`"), "{stderr}");
}
