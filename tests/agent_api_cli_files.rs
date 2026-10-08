//! `reshiki --cli` files through the real binary. Paths on the command line
//! carry the user's own authority: no folder grants, no extension
//! allowlist, and none of the MCP file tools' checks on what a path names.
//!
//! Every run has an empty data folder, a temporary working folder and a
//! watchdog, and inherits RESHIKI_INCHI_HELPER as CI sets it
//! (.github/workflows/checks.yml).
#[path = "common/headless.rs"]
mod headless;

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Output,
    time::Duration,
};

/// Long enough for a debug build's first chemistry engine call on CI.
const CONVERT: Duration = Duration::from_secs(120);

fn cli(dir: &Path, args: &[&str]) -> Output {
    headless::run_cli(dir, args, b"", CONVERT)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Every path under `root`, relative to it.
fn inventory(root: &Path) -> BTreeSet<PathBuf> {
    let mut paths = BTreeSet::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).expect("list a folder") {
            let path = entry.expect("a folder entry").path();
            if path.is_dir() {
                folders.push(path.clone());
            }
            paths.insert(path.strip_prefix(root).expect("inside").to_path_buf());
        }
    }
    paths
}

/// Asserts a failed run: exit 1, nothing on stdout and an error on stderr;
/// returns stderr.
fn assert_failed(output: &Output) -> String {
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
    assert!(stderr.starts_with("reshiki: error: "), "{stderr}");
    stderr
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("a UTF-8 temporary path")
}

#[test]
fn an_existing_output_is_kept_unless_forced() {
    let dir = tempfile::tempdir().expect("working folder");
    let out = dir.path().join("out");
    fs::create_dir(&out).expect("out");
    let target = out.join("e.mol");
    let target_arg = path_text(&target);

    let output = cli(
        dir.path(),
        &["convert", "--smiles", "CCO", "-o", target_arg],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
    let ethanol = fs::read(&target).expect("out/e.mol");
    assert!(text(&ethanol).contains("M  END"), "{}", text(&ethanol));

    let propanol = ["convert", "--smiles", "CCCO", "-o", target_arg];
    let stderr = assert_failed(&cli(dir.path(), &propanol));
    assert!(
        stderr.contains("exists; pass --force to replace"),
        "{stderr}"
    );
    assert_eq!(fs::read(&target).expect("out/e.mol"), ethanol);
    assert_eq!(inventory(&out), BTreeSet::from([PathBuf::from("e.mol")]));

    let output = cli(dir.path(), &[&propanol[..], &["--force"]].concat());
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let replaced = fs::read(&target).expect("out/e.mol");
    assert_ne!(replaced, ethanol);
    assert!(text(&replaced).contains("M  END"), "{}", text(&replaced));
    assert_eq!(inventory(&out), BTreeSet::from([PathBuf::from("e.mol")]));
}

#[test]
fn an_output_in_a_missing_folder_exits_1_and_creates_nothing() {
    let dir = tempfile::tempdir().expect("working folder");
    let target = dir.path().join("missing").join("e.mol");
    let stderr = assert_failed(&cli(
        dir.path(),
        &["convert", "--smiles", "CCO", "-o", path_text(&target)],
    ));
    assert!(stderr.contains("cannot write"), "{stderr}");
    assert_eq!(inventory(dir.path()), BTreeSet::new());
}

/// An output, new or replaced, gets the mode a plain `File::create` gets
/// in the same folder under the inherited umask.
#[cfg(unix)]
#[test]
fn an_output_gets_the_mode_file_create_gives() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o7777;
    let dir = tempfile::tempdir().expect("working folder");
    fs::File::create(dir.path().join("plain.txt")).expect("plain.txt");
    let expected = mode(&dir.path().join("plain.txt"));
    for args in [
        &["convert", "--smiles", "CCO", "-o", "e.mol"][..],
        &["convert", "--smiles", "CCCO", "-o", "e.mol", "--force"],
    ] {
        let output = cli(dir.path(), args);
        assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
        assert_eq!(mode(&dir.path().join("e.mol")), expected, "{args:?}");
    }
}

/// A file name that is not UTF-8 is written as given wherever the file
/// system allows it. APFS refuses such names; there the run fails like any
/// other write and leaves nothing behind.
#[cfg(unix)]
#[test]
fn a_non_utf8_output_name_is_written_as_given() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
    let name = OsStr::from_bytes(b"e\xff.mol");
    let probe = tempfile::tempdir().expect("probe folder");
    let supported = fs::File::create(probe.path().join(name)).is_ok();
    let dir = tempfile::tempdir().expect("working folder");
    let target = dir.path().join(name);
    let args = [
        OsStr::new("convert"),
        OsStr::new("--smiles"),
        OsStr::new("CCO"),
        OsStr::new("-o"),
        target.as_os_str(),
    ];
    let output = headless::run_cli_os(dir.path(), &args, b"", CONVERT);
    if supported {
        assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
        let molfile = fs::read(dir.path().join(name)).expect("the output");
        assert!(text(&molfile).contains("M  END"), "{}", text(&molfile));
        assert_eq!(inventory(dir.path()), BTreeSet::from([PathBuf::from(name)]));
    } else {
        assert_failed(&output);
        assert_eq!(inventory(dir.path()), BTreeSet::new());
    }
}

/// Input from a FIFO is read as the user names it: the CLI does not apply
/// the MCP file tools' regular-file check.
#[cfg(unix)]
#[test]
fn input_from_a_fifo_is_read() {
    let dir = tempfile::tempdir().expect("working folder");
    let fifo = dir.path().join("input.fifo");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("run mkfifo");
    assert!(made.success(), "mkfifo failed");
    // Opening a FIFO for writing waits for its reader, the CLI.
    let writer = {
        let fifo = fifo.clone();
        std::thread::spawn(move || fs::write(fifo, "CCO"))
    };
    let output = cli(
        dir.path(),
        &["convert", "input.fifo", "--from", "smiles", "-o", "e.mol"],
    );
    // A failed run may leave the writer waiting; it is left behind.
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    writer.join().expect("writer").expect("write the FIFO");
    let molfile = fs::read(dir.path().join("e.mol")).expect("e.mol");
    assert!(text(&molfile).contains("M  END"), "{}", text(&molfile));
}
