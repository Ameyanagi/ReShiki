//! Windows escape cases: junctions, symlinks, Win32 path syntax and 8.3
//! short names. They run only on the windows-2022 CI runner.
use super::*;
use crate::access::WriteMode;
use std::{
    io,
    os::windows::{
        fs::{symlink_dir, symlink_file},
        process::CommandExt as _,
    },
};

/// ERROR_PRIVILEGE_NOT_HELD: the account may not create symbolic links.
const PRIVILEGE_NOT_HELD: i32 = 1314;
const BOUND: Duration = Duration::from_secs(2);

/// Run one `cmd /C` line verbatim (no argument quoting) and require success.
fn cmd(line: &str) -> Output {
    let output = run_bounded(
        Command::new("cmd").raw_arg(format!("/C {line}")),
        Duration::from_secs(10),
    );
    assert!(output.status.success(), "cmd /C {line}: {output:?}");
    output
}

fn junction(link: &Path, target: &Path) {
    cmd(&format!(
        "mklink /J \"{}\" \"{}\"",
        link.display(),
        target.display()
    ));
}

/// The 8.3 spelling of an existing path
/// (<https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file#short-vs-long-names>).
fn short_path(path: &Path) -> PathBuf {
    let output = cmd(&format!("for %I in (\"{}\") do @echo %~sI", path.display()));
    PathBuf::from(String::from_utf8_lossy(&output.stdout).trim_end())
}

/// Whether a symlink was created. Without the privilege the case is skipped
/// with a printed reason, unless RESHIKI_REQUIRE_LINK_TESTS=1 (as in CI).
fn linked(case: &str, created: io::Result<()>) -> bool {
    match created {
        Ok(()) => true,
        Err(error) if error.raw_os_error() == Some(PRIVILEGE_NOT_HELD) => {
            let required =
                std::env::var_os("RESHIKI_REQUIRE_LINK_TESTS").is_some_and(|value| value == "1");
            assert!(
                !required,
                "{case}: RESHIKI_REQUIRE_LINK_TESTS=1 but this account cannot create symlinks: {error}"
            );
            println!("skipped {case}: this account cannot create symlinks ({error})");
            false
        }
        Err(error) => panic!("{case}: cannot create the symlink: {error}"),
    }
}

fn bounded_read(grants: &Grants, path: String) -> Result<Vec<u8>, AccessError> {
    let grants = grants.clone();
    run_with_timeout(BOUND, move || grants.read(&path, LIMIT, FORMATS))
}

/// Reads and writes of every `(read, write)` request pair are refused.
fn assert_refused(sandbox: &Sandbox, grants: &Grants, cases: &[(&str, &str)], title: &str) {
    let mut rows = Vec::new();
    for &(read, write) in cases {
        let result = bounded_read(grants, sandbox.path(read));
        rows.push((format!("read {read}"), read_outcome(&result)));
        assert!(result.is_err(), "read {read}: {result:?}");
        for mode in [WriteMode::CreateNew, WriteMode::Replace] {
            let result = grants.write_atomic(&sandbox.path(write), b"x", mode, FORMATS);
            rows.push((format!("{mode:?} {write}"), write_outcome(&result)));
            assert!(result.is_err(), "{mode:?} {write}: {result:?}");
        }
    }
    print_outcomes(["case", "outcome"], title, &rows);
    assert!(temps(&sandbox.root).is_empty());
    sandbox.assert_outside_untouched();
}

#[test]
fn a_junction_to_outside_is_refused_for_reads_and_writes() {
    let sandbox = Sandbox::new();
    junction(&sandbox.root.join("junction"), &sandbox.outside);
    let grants = sandbox.grants();
    assert_refused(
        &sandbox,
        &grants,
        &[
            ("junction/secret.mol", "junction/new.rsk"),
            ("junction/secret.mol", "junction/secret.mol"),
        ],
        "junction to outside",
    );
}

#[test]
fn escaping_directory_and_file_symlinks_are_refused() {
    let sandbox = Sandbox::new();
    let root = &sandbox.root;
    let links = [
        (
            "dir-absolute",
            symlink_dir(&sandbox.outside, root.join("dir-absolute")),
        ),
        (
            "dir-relative",
            symlink_dir(r"..\outside", root.join("dir-relative")),
        ),
        (
            "file-absolute.mol",
            symlink_file(sandbox.secret(), root.join("file-absolute.mol")),
        ),
        (
            "file-relative.mol",
            symlink_file(r"..\outside\secret.mol", root.join("file-relative.mol")),
        ),
    ];
    for (case, created) in links {
        if !linked(case, created) {
            return;
        }
    }
    let grants = sandbox.grants();
    assert_refused(
        &sandbox,
        &grants,
        &[
            ("dir-absolute/secret.mol", "dir-absolute/new.rsk"),
            ("dir-relative/secret.mol", "dir-relative/new.rsk"),
            ("file-absolute.mol", "file-absolute.mol"),
            ("file-relative.mol", "file-relative.mol"),
        ],
        "symlinks to outside",
    );
}

#[test]
fn symlinks_staying_inside_the_root_never_escape() {
    let sandbox = Sandbox::new();
    let root = &sandbox.root;
    let sub = root.join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("secret.mol"), INSIDE).unwrap();
    fs::write(root.join("real.mol"), INSIDE).unwrap();
    let links = [
        ("dir-absolute", symlink_dir(&sub, root.join("dir-absolute"))),
        (
            "dir-relative",
            symlink_dir("sub", root.join("dir-relative")),
        ),
        (
            "file-absolute.mol",
            symlink_file(root.join("real.mol"), root.join("file-absolute.mol")),
        ),
        (
            "file-relative.mol",
            symlink_file("real.mol", root.join("file-relative.mol")),
        ),
    ];
    for (case, created) in links {
        if !linked(case, created) {
            return;
        }
    }
    let grants = sandbox.grants();
    let mut rows = Vec::new();
    for path in [
        "dir-absolute/secret.mol",
        "dir-relative/secret.mol",
        "file-absolute.mol",
        "file-relative.mol",
    ] {
        let read = bounded_read(&grants, sandbox.path(path));
        rows.push((format!("read {path}"), read_outcome(&read)));
    }
    for path in ["dir-absolute/new-a.rsk", "dir-relative/new-r.rsk"] {
        let write = grants.write_atomic(&sandbox.path(path), b"x", WriteMode::CreateNew, FORMATS);
        rows.push((format!("CreateNew {path}"), write_outcome(&write)));
    }
    print_outcomes(
        ["case", "outcome"],
        "symlinks staying inside the root",
        &rows,
    );
    sandbox.assert_outside_untouched();
}

#[test]
fn win32_path_syntax_under_the_root_is_rejected_by_rule() {
    let sandbox = Sandbox::new();
    let grants = sandbox.grants();
    let root = text(&sandbox.root);
    let forward = root.replace('\\', "/");
    let (drive, rooted) = root.split_at(2);
    let letter = drive.trim_end_matches(':');
    let cases = [
        (format!("{forward}/a.rsk:zone"), "ads"),
        (format!("{forward}/con.rsk"), "reserved_name"),
        (format!("{forward}/COM1.svg"), "reserved_name"),
        (format!("{forward}/CONOUT$"), "reserved_name"),
        (format!("{forward}/sub/NUL"), "reserved_name"),
        (format!("{forward}/a.rsk."), "trailing_dot_space"),
        (format!("{forward}/a.rsk "), "trailing_dot_space"),
        (format!(r"\\?\{root}\a.rsk"), "verbatim"),
        (r"\\.\pipe\x".to_owned(), "device"),
        (r"\\localhost\C$\x".to_owned(), "unc"),
        (format!(r"\\localhost\{letter}${rooted}\a.rsk"), "unc"),
        (format!("{drive}a.rsk"), "drive_relative"),
        (format!(r"{rooted}\a.rsk"), "rooted_without_drive"),
    ];
    for (input, expected) in cases {
        let read = grants.read(&input, LIMIT, FORMATS);
        assert!(
            matches!(read, Err(AccessError::PathInvalid { rule, .. }) if rule == expected),
            "read {input}: {read:?}"
        );
        let write = grants.write_atomic(&input, b"x", WriteMode::CreateNew, FORMATS);
        assert!(
            matches!(write, Err(AccessError::PathInvalid { rule, .. }) if rule == expected),
            "write {input}: {write:?}"
        );
    }
    assert_eq!(fs::read_dir(&sandbox.root).unwrap().count(), 0);
    sandbox.assert_outside_untouched();
}

#[test]
fn short_names_fail_closed_at_the_root_and_resolve_inside_it() {
    let sandbox = Sandbox::new();
    let root = sandbox.base.join("long-granted-folder");
    fs::create_dir(&root).unwrap();
    let long_file = root.join("long-file-name.mol");
    fs::write(&long_file, INSIDE).unwrap();
    let short_root = short_path(&root);
    if short_root
        .as_os_str()
        .eq_ignore_ascii_case(root.as_os_str())
    {
        println!(
            "skipped: 8.3 short names are not generated on this volume ({})",
            root.display()
        );
        return;
    }
    let grants = Grants::open(std::slice::from_ref(&root), std::slice::from_ref(&root)).unwrap();
    // The short spelling of the root is not a granted spelling.
    let read = grants.read(
        &text(&short_root.join("long-file-name.mol")),
        LIMIT,
        FORMATS,
    );
    assert_eq!(read.unwrap_err().code(), "path_not_granted");
    let write = grants.write_atomic(
        &text(&short_root.join("b.rsk")),
        b"x",
        WriteMode::CreateNew,
        FORMATS,
    );
    assert_eq!(write.unwrap_err().code(), "path_not_granted");
    assert!(!root.join("b.rsk").exists());
    // A short name below the root resolves inside it.
    let short_file = short_path(&long_file);
    let short_name = short_file.file_name().unwrap();
    if short_name.eq_ignore_ascii_case("long-file-name.mol") {
        println!(
            "skipped: the file has no 8.3 short name ({})",
            long_file.display()
        );
        return;
    }
    assert_eq!(
        grants
            .read(&text(&root.join(short_name)), LIMIT, FORMATS)
            .unwrap(),
        INSIDE
    );
    sandbox.assert_outside_untouched();
}

#[test]
fn mixed_separators_and_a_lowercase_drive_letter_stay_inside() {
    let sandbox = Sandbox::new();
    fs::write(sandbox.root.join("a.mol"), INSIDE).unwrap();
    let grants = sandbox.grants();
    let root = text(&sandbox.root);
    let (drive, rest) = root.split_at(1);
    let spelled = format!("{}{}", drive.to_ascii_lowercase(), rest.replace('\\', "/"));
    assert_eq!(
        grants
            .read(&format!(r"{spelled}\a.mol"), LIMIT, FORMATS)
            .unwrap(),
        INSIDE
    );
    grants
        .write_atomic(
            &format!("{spelled}/b.rsk"),
            b"b",
            WriteMode::CreateNew,
            FORMATS,
        )
        .unwrap();
    assert_eq!(fs::read(sandbox.root.join("b.rsk")).unwrap(), b"b");
    sandbox.assert_outside_untouched();
}

#[test]
fn replace_succeeds_and_create_new_refuses_over_an_existing_file() {
    let sandbox = Sandbox::new();
    let target = sandbox.root.join("a.rsk");
    fs::write(&target, b"old").unwrap();
    let grants = sandbox.grants();
    let receipt = grants
        .write_atomic(&sandbox.path("a.rsk"), b"new", WriteMode::Replace, FORMATS)
        .unwrap();
    assert!(receipt.replaced);
    assert_eq!(fs::read(&target).unwrap(), b"new");
    let error = grants
        .write_atomic(
            &sandbox.path("a.rsk"),
            b"newer",
            WriteMode::CreateNew,
            FORMATS,
        )
        .unwrap_err();
    assert_eq!(error.code(), "file_exists");
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert!(temps(&sandbox.root).is_empty());
}

#[test]
fn executable_and_shortcut_extensions_are_never_written() {
    let sandbox = Sandbox::new();
    let grants = sandbox.grants();
    for extension in ["exe", "bat", "cmd", "lnk", "url", "EXE"] {
        let name = format!("a.{extension}");
        for mode in [WriteMode::CreateNew, WriteMode::Replace] {
            let error = grants
                .write_atomic(&sandbox.path(&name), b"x", mode, FORMATS)
                .unwrap_err();
            assert_eq!(error.code(), "extension_not_allowed", "{name} {mode:?}");
        }
    }
    assert_eq!(fs::read_dir(&sandbox.root).unwrap().count(), 0);
}
