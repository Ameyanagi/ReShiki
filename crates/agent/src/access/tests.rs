use super::{
    path::parse,
    root::{Kind, Root, normalize},
    write::Hooks,
    *,
};
use std::{
    fs,
    path::{Path, PathBuf},
    slice,
    sync::Arc,
};

const FORMATS: &[&str] = &["rsk", "mol"];

// Grants is shared across threads and cloned into hosts.
const _: () = {
    const fn shareable<T: Send + Sync + Clone>() {}
    shareable::<Grants>()
};

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

/// A canonical temporary folder, so request spellings are predictable.
fn sandbox() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    let base = normalize(base).unwrap();
    (dir, base)
}

fn folder(base: &Path, name: &str) -> PathBuf {
    let path = base.join(name);
    fs::create_dir(&path).unwrap();
    path
}

fn both(root: &Path) -> Grants {
    Grants::open(&[root.to_path_buf()], &[root.to_path_buf()]).unwrap()
}

fn temps(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".reshiki-") && name.ends_with(".tmp"))
        .collect()
}

fn rule(input: &str) -> &'static str {
    match parse(input) {
        Err(AccessError::PathInvalid { rule, .. }) => rule,
        other => panic!("{input:?} was not rejected lexically: {other:?}"),
    }
}

fn absolute(rest: &str) -> String {
    if cfg!(windows) {
        format!("C:\\{rest}")
    } else {
        format!("/{rest}")
    }
}

#[test]
fn reads_and_writes_inside_a_root_succeed() {
    let (_dir, base) = sandbox();
    fs::write(base.join("a.mol"), b"molfile").unwrap();
    let sub = folder(&base, "sub");
    let grants = both(&base);

    assert_eq!(
        grants
            .read(&text(&base.join("a.mol")), 64, FORMATS)
            .unwrap(),
        b"molfile"
    );
    let target = text(&sub.join("b.rsk"));
    let receipt = grants
        .write_atomic(&target, b"first", WriteMode::CreateNew, FORMATS)
        .unwrap();
    assert_eq!(
        receipt,
        WriteReceipt {
            path: target.clone(),
            bytes: 5,
            replaced: false,
            warnings: vec![],
        }
    );
    let receipt = grants
        .write_atomic(&target, b"second", WriteMode::Replace, FORMATS)
        .unwrap();
    assert!(receipt.replaced);
    assert_eq!(fs::read(sub.join("b.rsk")).unwrap(), b"second");
    let receipt = grants
        .write_atomic(
            &text(&base.join("c.rsk")),
            b"new",
            WriteMode::Replace,
            FORMATS,
        )
        .unwrap();
    assert!(!receipt.replaced);
    assert_eq!(grants.read(&target, 64, FORMATS).unwrap(), b"second");
    assert!(temps(&base).is_empty());
    assert!(temps(&sub).is_empty());
}

#[test]
fn read_and_write_roots_stay_separate() {
    let (_dir, base) = sandbox();
    let readable = folder(&base, "readable");
    let writable = folder(&base, "writable");
    fs::write(readable.join("a.mol"), b"r").unwrap();
    fs::write(writable.join("a.mol"), b"w").unwrap();
    let grants = Grants::open(slice::from_ref(&readable), slice::from_ref(&writable)).unwrap();

    let error = grants
        .read(&text(&writable.join("a.mol")), 64, FORMATS)
        .unwrap_err();
    assert_eq!(error.code(), "path_not_granted");
    assert_eq!(
        error,
        AccessError::PathNotGranted {
            path: text(&writable.join("a.mol")),
            access: "reading",
            roots: vec![text(&readable)],
        }
    );
    assert!(error.to_string().contains(&text(&readable)), "{error}");

    let error = grants
        .write_atomic(
            &text(&readable.join("b.rsk")),
            b"x",
            WriteMode::CreateNew,
            FORMATS,
        )
        .unwrap_err();
    assert_eq!(error.code(), "path_not_granted");
    assert!(error.to_string().contains(&text(&writable)), "{error}");
    assert!(!readable.join("b.rsk").exists());

    let error = Grants::none()
        .read(&text(&readable.join("a.mol")), 64, FORMATS)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("no folders are granted for reading"),
        "{error}"
    );
    assert_eq!(
        grants.summary(),
        GrantSummary {
            read: vec![text(&readable)],
            write: vec![text(&writable)],
        }
    );
}

#[test]
fn the_longest_prefix_wins_between_nested_roots() {
    let (_dir, base) = sandbox();
    let inner = folder(&base, "inner");
    for order in [[base.clone(), inner.clone()], [inner.clone(), base.clone()]] {
        let grants = Grants::open(&order, &[]).unwrap();
        let request = parse(&text(&inner.join("x.mol"))).unwrap();
        let (root, rest) = grants.locate(Kind::Read, &request).unwrap();
        assert_eq!(root.display, inner);
        assert_eq!(rest, Path::new("x.mol"));
        // A root itself is not a file inside it: the remainder must be
        // non-empty, so the outer root matches with `inner` as the remainder.
        let request = parse(&text(&inner)).unwrap();
        let (root, rest) = grants.locate(Kind::Read, &request).unwrap();
        assert_eq!(root.display, base);
        assert_eq!(rest, Path::new("inner"));
    }
    // Whole components only: `inner2` is not inside `inner`.
    let grants = Grants::open(slice::from_ref(&inner), &[]).unwrap();
    let request = parse(&text(&base.join("inner2").join("x.mol"))).unwrap();
    assert!(grants.locate(Kind::Read, &request).is_err());
}

#[test]
fn empty_paths_are_invalid() {
    assert_eq!(rule(""), "empty");
}

#[test]
fn overlong_paths_are_invalid() {
    let at_limit = absolute(&"a".repeat(4096 - absolute("").len()));
    assert_eq!(at_limit.len(), 4096);
    assert!(parse(&at_limit).is_ok());
    assert_eq!(rule(&absolute(&"a".repeat(4096))), "too_long");
}

#[test]
fn nul_characters_are_invalid() {
    assert_eq!(rule(&absolute("a\0b.rsk")), "nul");
}

#[test]
fn file_uris_are_invalid_with_a_hint() {
    assert_eq!(rule("file:///tmp/a.rsk"), "file_uri");
    assert_eq!(rule("FILE:/tmp/a.rsk"), "file_uri");
    let error = parse("file:///tmp/a.rsk").unwrap_err();
    assert!(error.to_string().contains("plain absolute path"), "{error}");
}

#[test]
fn relative_paths_are_invalid() {
    assert_eq!(rule("a.rsk"), "relative");
    assert_eq!(rule("./a.rsk"), "relative");
}

#[test]
fn parent_components_are_invalid() {
    assert_eq!(rule(&absolute("r/../a.rsk")), "dotdot");
}

#[test]
fn trailing_separators_are_invalid() {
    assert_eq!(rule(&absolute("r/")), "trailing_separator");
    assert_eq!(rule(&absolute("")), "trailing_separator");
}

#[test]
fn dotdot_and_outside_paths_are_rejected_before_any_io() {
    let (_dir, base) = sandbox();
    let root = folder(&base, "root");
    let outside = folder(&base, "outside");
    fs::write(outside.join("secret.mol"), b"secret").unwrap();
    let grants = both(&root);

    let escape = format!("{}/../outside/secret.mol", text(&root));
    let error = grants.read(&escape, 64, FORMATS).unwrap_err();
    assert!(
        matches!(error, AccessError::PathInvalid { rule: "dotdot", .. }),
        "{error:?}"
    );
    let error = grants
        .write_atomic(&escape, b"x", WriteMode::Replace, FORMATS)
        .unwrap_err();
    assert!(
        matches!(error, AccessError::PathInvalid { rule: "dotdot", .. }),
        "{error:?}"
    );
    // Outside paths are refused by matching alone: a missing file is
    // path_not_granted, not file_not_found.
    for name in ["secret.mol", "missing.mol"] {
        let path = text(&outside.join(name));
        assert_eq!(
            grants.read(&path, 64, FORMATS).unwrap_err().code(),
            "path_not_granted"
        );
        assert_eq!(
            grants
                .write_atomic(&path, b"x", WriteMode::Replace, FORMATS)
                .unwrap_err()
                .code(),
            "path_not_granted"
        );
    }
    assert_eq!(fs::read(outside.join("secret.mol")).unwrap(), b"secret");
    assert!(!outside.join("missing.mol").exists());
    assert!(temps(&outside).is_empty());
}

#[test]
fn read_limits_and_file_types() {
    let (_dir, base) = sandbox();
    fs::write(base.join("big.mol"), [b'x'; 11]).unwrap();
    fs::write(base.join("exact.mol"), [b'x'; 10]).unwrap();
    fs::create_dir(base.join("folder.mol")).unwrap();
    let grants = both(&base);

    let error = grants
        .read(&text(&base.join("big.mol")), 10, FORMATS)
        .unwrap_err();
    assert_eq!(
        error,
        AccessError::FileTooLarge {
            path: text(&base.join("big.mol")),
            limit: 10,
        }
    );
    assert_eq!(
        grants
            .read(&text(&base.join("exact.mol")), 10, FORMATS)
            .unwrap()
            .len(),
        10
    );
    assert_eq!(
        grants
            .read(&text(&base.join("folder.mol")), 10, FORMATS)
            .unwrap_err()
            .code(),
        "not_a_regular_file"
    );
    assert_eq!(
        grants
            .read(&text(&base.join("missing.mol")), 10, FORMATS)
            .unwrap_err()
            .code(),
        "file_not_found"
    );
}

#[test]
fn create_new_never_replaces_an_existing_file() {
    let (_dir, base) = sandbox();
    fs::write(base.join("a.rsk"), b"old").unwrap();
    let grants = both(&base);
    let error = grants
        .write_atomic(
            &text(&base.join("a.rsk")),
            b"new",
            WriteMode::CreateNew,
            FORMATS,
        )
        .unwrap_err();
    assert_eq!(error.code(), "file_exists");
    assert_eq!(fs::read(base.join("a.rsk")).unwrap(), b"old");
    assert!(temps(&base).is_empty());
}

#[test]
fn create_new_loses_a_race_to_a_competing_file() {
    let (_dir, base) = sandbox();
    let target = base.join("a.rsk");
    let grants = both(&base);
    let competitor = || fs::write(&target, b"competitor").unwrap();
    let hooks = Hooks {
        before_publish: Some(&competitor),
        skip_temp_removal: false,
    };
    let error = grants
        .write_with(
            &text(&target),
            b"mine",
            WriteMode::CreateNew,
            FORMATS,
            &hooks,
        )
        .unwrap_err();
    assert_eq!(error.code(), "file_exists");
    assert_eq!(fs::read(&target).unwrap(), b"competitor");
    assert!(temps(&base).is_empty());
}

#[test]
fn create_new_loses_a_race_to_a_competing_directory() {
    let (_dir, base) = sandbox();
    let target = base.join("a.rsk");
    let grants = both(&base);
    let competitor = || fs::create_dir(&target).unwrap();
    let hooks = Hooks {
        before_publish: Some(&competitor),
        skip_temp_removal: false,
    };
    let error = grants
        .write_with(
            &text(&target),
            b"mine",
            WriteMode::CreateNew,
            FORMATS,
            &hooks,
        )
        .unwrap_err();
    assert_eq!(error.code(), "file_exists");
    assert!(target.is_dir());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    assert!(temps(&base).is_empty());
}

#[test]
fn replace_never_replaces_a_directory() {
    let (_dir, base) = sandbox();
    let target = base.join("a.rsk");
    let grants = both(&base);
    // Checked before publication...
    fs::create_dir(&target).unwrap();
    let error = grants
        .write_atomic(&text(&target), b"mine", WriteMode::Replace, FORMATS)
        .unwrap_err();
    assert_eq!(error.code(), "not_a_regular_file");
    fs::remove_dir(&target).unwrap();
    // ...and refused by the rename when it races in afterwards.
    let competitor = || fs::create_dir(&target).unwrap();
    let hooks = Hooks {
        before_publish: Some(&competitor),
        skip_temp_removal: false,
    };
    let result = grants.write_with(&text(&target), b"mine", WriteMode::Replace, FORMATS, &hooks);
    assert!(result.is_err(), "{result:?}");
    assert!(target.is_dir());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    assert!(temps(&base).is_empty());
}

#[test]
fn replace_keeps_the_old_bytes_until_the_rename() {
    let (_dir, base) = sandbox();
    let target = base.join("a.rsk");
    fs::write(&target, b"old").unwrap();
    let grants = both(&base);
    let observe = || {
        assert_eq!(fs::read(&target).unwrap(), b"old");
        let staged = temps(&base);
        assert_eq!(staged.len(), 1);
        assert_eq!(fs::read(base.join(&staged[0])).unwrap(), b"new");
    };
    let hooks = Hooks {
        before_publish: Some(&observe),
        skip_temp_removal: false,
    };
    let receipt = grants
        .write_with(&text(&target), b"new", WriteMode::Replace, FORMATS, &hooks)
        .unwrap();
    assert!(receipt.replaced);
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert!(temps(&base).is_empty());
}

#[test]
fn an_interrupted_publication_leaves_a_complete_destination() {
    let (_dir, base) = sandbox();
    let target = base.join("a.rsk");
    let grants = both(&base);
    let hooks = Hooks {
        before_publish: None,
        skip_temp_removal: true,
    };
    grants
        .write_with(
            &text(&target),
            b"complete",
            WriteMode::CreateNew,
            FORMATS,
            &hooks,
        )
        .unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"complete");
    // Documented: the temporary name can be left beside the destination.
    let left = temps(&base);
    assert_eq!(left.len(), 1);
    assert_eq!(fs::read(base.join(&left[0])).unwrap(), b"complete");
}

#[test]
fn extensions_are_allowlisted() {
    let (_dir, base) = sandbox();
    fs::write(base.join("a.exe"), b"x").unwrap();
    fs::write(base.join("UPPER.MOL"), b"mol").unwrap();
    let grants = both(&base);

    let error = grants
        .read(&text(&base.join("a.exe")), 64, FORMATS)
        .unwrap_err();
    assert_eq!(
        error,
        AccessError::ExtensionNotAllowed {
            path: text(&base.join("a.exe")),
            allowed: vec!["rsk".into(), "mol".into()],
        }
    );
    assert_eq!(
        grants
            .read(&text(&base.join("UPPER.MOL")), 64, FORMATS)
            .unwrap(),
        b"mol"
    );
    for name in ["b.exe", "noextension", "b.rsk.bat"] {
        let error = grants
            .write_atomic(&text(&base.join(name)), b"x", WriteMode::CreateNew, FORMATS)
            .unwrap_err();
        assert_eq!(error.code(), "extension_not_allowed", "{name}");
        assert!(!base.join(name).exists());
    }
}

#[test]
fn final_names_are_checked_before_writing() {
    let (_dir, base) = sandbox();
    let grants = both(&base);
    let longest = format!("{}.rsk", "a".repeat(251));
    let cases = [
        (".hidden.rsk".to_owned(), "leading_dot"),
        (format!("a{longest}"), "name_too_long"),
        ("bell\u{7}.rsk".to_owned(), "control_character"),
    ];
    for (name, expected) in cases {
        let error = grants
            .write_atomic(
                &text(&base.join(&name)),
                b"x",
                WriteMode::CreateNew,
                FORMATS,
            )
            .unwrap_err();
        assert!(
            matches!(error, AccessError::PathInvalid { rule, .. } if rule == expected),
            "{name:?}: {error:?}"
        );
    }
    assert_eq!(fs::read_dir(&base).unwrap().count(), 0);
    assert_eq!(longest.len(), 255);
    grants
        .write_atomic(
            &text(&base.join(&longest)),
            b"x",
            WriteMode::CreateNew,
            FORMATS,
        )
        .unwrap();
}

#[test]
fn messages_echo_at_most_512_characters() {
    let long = absolute(&format!("{}.rsk", "é".repeat(1000)));
    let error = Grants::none().read(&long, 64, FORMATS).unwrap_err();
    let AccessError::PathNotGranted { path, .. } = error else {
        panic!("{error:?}");
    };
    assert_eq!(path.chars().count(), 512);
    assert!(path.ends_with('…'));
}

#[test]
fn roots_must_be_existing_absolute_folders() {
    let (_dir, base) = sandbox();
    fs::write(base.join("file.mol"), b"x").unwrap();
    assert_eq!(
        Root::open(&base.join("missing")).unwrap_err(),
        GrantError::NotFound(base.join("missing"))
    );
    assert_eq!(
        Root::open(&base.join("file.mol")).unwrap_err(),
        GrantError::NotADirectory(base.join("file.mol"))
    );
    assert!(matches!(
        Root::open(Path::new("relative")),
        Err(GrantError::Config(_))
    ));
    assert_eq!(GRANT_EXIT_CODE, 2);
}

#[test]
fn the_open_hook_runs_between_canonicalization_and_the_open() {
    let (_dir, base) = sandbox();
    let root = folder(&base, "root");
    let remove = || fs::remove_dir(&root).unwrap();
    assert_eq!(
        Root::open_with_hook(&root, &remove).unwrap_err(),
        GrantError::NotFound(root.clone())
    );
}

#[cfg(unix)]
#[test]
fn a_root_swapped_for_a_symlink_while_opening_is_refused() {
    let (_dir, base) = sandbox();
    let root = folder(&base, "root");
    let elsewhere = folder(&base, "elsewhere");
    let swap = || {
        fs::rename(&root, base.join("moved")).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &root).unwrap();
    };
    assert_eq!(
        Root::open_with_hook(&root, &swap).unwrap_err(),
        GrantError::Changed(root.clone())
    );
}

#[cfg(unix)]
#[test]
fn escapes_and_os_denials_map_to_distinct_codes() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, base) = sandbox();
    let root = folder(&base, "root");
    let outside = folder(&base, "outside");
    fs::write(outside.join("secret.mol"), b"secret").unwrap();
    std::os::unix::fs::symlink(outside.join("secret.mol"), root.join("link.mol")).unwrap();
    let grants = both(&root);
    let error = grants
        .read(&text(&root.join("link.mol")), 64, FORMATS)
        .unwrap_err();
    assert_eq!(error.code(), "path_escapes_root");
    assert!(
        error
            .to_string()
            .ends_with("is outside the granted folder or denied by the operating system"),
        "{error}"
    );
    // cap-std's own refusal has no OS error code; the operating system's does.
    fs::write(root.join("locked.mol"), b"locked").unwrap();
    fs::set_permissions(root.join("locked.mol"), fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(root.join("locked.mol")).is_err() {
        assert_eq!(
            grants
                .read(&text(&root.join("locked.mol")), 64, FORMATS)
                .unwrap_err()
                .code(),
            "os_denied"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_symlinked_root_matches_both_spellings() {
    let (_dir, base) = sandbox();
    let real = folder(&base, "real");
    let link = base.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    fs::write(real.join("a.mol"), b"mol").unwrap();
    let grants = both(&link);
    assert_eq!(grants.summary().read, vec![text(&link)]);
    for spelling in [&link, &real] {
        assert_eq!(
            grants
                .read(&text(&spelling.join("a.mol")), 64, FORMATS)
                .unwrap(),
            b"mol"
        );
    }
    grants
        .write_atomic(
            &text(&link.join("b.rsk")),
            b"rsk",
            WriteMode::CreateNew,
            FORMATS,
        )
        .unwrap();
    assert_eq!(fs::read(real.join("b.rsk")).unwrap(), b"rsk");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_tmp_and_private_tmp_name_the_same_root() {
    let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let display = dir.path().to_path_buf();
    let private = Path::new("/private").join(display.strip_prefix("/").unwrap());
    fs::write(display.join("a.mol"), b"mol").unwrap();
    let grants = both(&display);
    assert_eq!(grants.read[0].canonical, private);
    for spelling in [&display, &private] {
        assert_eq!(
            grants
                .read(&text(&spelling.join("a.mol")), 64, FORMATS)
                .unwrap(),
            b"mol"
        );
    }
    // Matching is exact-case even on a case-insensitive volume.
    let upper = text(&display.join("a.mol")).replacen("/tmp/", "/TMP/", 1);
    assert_eq!(
        grants.read(&upper, 64, FORMATS).unwrap_err().code(),
        "path_not_granted"
    );
}

#[cfg(windows)]
#[test]
fn windows_canonical_paths_are_normalized() {
    assert_eq!(
        normalize(PathBuf::from(r"\\?\c:\Users\me\docs")).unwrap(),
        PathBuf::from(r"C:\Users\me\docs")
    );
    assert_eq!(
        normalize(PathBuf::from(r"C:\plain")).unwrap(),
        PathBuf::from(r"C:\plain")
    );
    for unsupported in [
        r"\\?\UNC\server\share\docs",
        r"\\server\share\docs",
        r"\\.\pipe\docs",
        r"\\?\Volume{00000000-0000-0000-0000-000000000000}\docs",
    ] {
        assert!(
            matches!(
                normalize(PathBuf::from(unsupported)),
                Err(GrantError::Unsupported(_))
            ),
            "{unsupported}"
        );
    }
}

#[cfg(windows)]
#[test]
fn windows_lexical_rules() {
    let cases = [
        (r"\\?\C:\r\a.rsk", "verbatim"),
        (r"\\server\share\a.rsk", "unc"),
        (r"\\.\pipe\x", "device"),
        ("C:a.rsk", "drive_relative"),
        (r"\r\a.rsk", "rooted_without_drive"),
        (r"C:\r\a.rsk:zone", "ads"),
        (r"C:\r\con.rsk", "reserved_name"),
        (r"C:\r\COM1.svg", "reserved_name"),
        (r"C:\r\CONOUT$", "reserved_name"),
        (r"C:\r\sub\NUL", "reserved_name"),
        ("C:\\r\\lpt\u{b9} .rsk", "reserved_name"),
        (r"C:\r\a.rsk.", "trailing_dot_space"),
        (r"C:\r\a.rsk ", "trailing_dot_space"),
    ];
    for (input, expected) in cases {
        assert_eq!(rule(input), expected, "{input}");
    }
    assert!(parse(r"c:/r\mixed/a.rsk").is_ok());
}

#[cfg(windows)]
#[test]
fn windows_reserved_characters_in_final_names() {
    let (_dir, base) = sandbox();
    let grants = both(&base);
    let error = grants
        .write_atomic(
            &text(&base.join("a?.rsk")),
            b"x",
            WriteMode::CreateNew,
            FORMATS,
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            AccessError::PathInvalid {
                rule: "reserved_character",
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn clones_share_handles() {
    let (_dir, base) = sandbox();
    let grants = both(&base);
    let copy = grants.clone();
    assert!(Arc::ptr_eq(&grants.read[0].dir, &copy.read[0].dir));
    assert!(Arc::ptr_eq(&grants.write[0].dir, &copy.write[0].dir));
}
