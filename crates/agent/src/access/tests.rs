// Tests build their layouts with std::fs and tempfile, outside every grant.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
use super::{
    config::FILE_NAME,
    path::parse,
    protected::{app_bundle, project},
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

fn write_config(dir: &Path, json: &str) -> PathBuf {
    let file = dir.join(FILE_NAME);
    fs::write(&file, json).unwrap();
    file
}

/// The configuration error, which must name the file.
fn config_error(dir: &Path) -> String {
    let file = text(&dir.join(FILE_NAME));
    match AccessConfig::load_from(dir) {
        Err(GrantError::Config(message)) => {
            assert!(message.contains(&file), "{message} does not name {file}");
            message
        }
        other => panic!("expected a configuration error, got {other:?}"),
    }
}

#[test]
fn a_valid_configuration_loads() {
    let (_dir, base) = sandbox();
    let read = folder(&base, "read");
    let write = folder(&base, "write");
    let json = serde_json::json!({ "version": 1, "read": [text(&read)], "write": [text(&write)] });
    write_config(&base, &json.to_string());
    let config = AccessConfig::load_from(&base).unwrap();
    assert_eq!(config.read, [read]);
    assert_eq!(config.write, [write]);
    // Either list may be left out.
    write_config(&base, r#"{ "version": 1 }"#);
    assert_eq!(
        AccessConfig::load_from(&base).unwrap(),
        AccessConfig::empty()
    );
}

#[test]
fn a_missing_configuration_grants_nothing() {
    let (_dir, base) = sandbox();
    assert_eq!(
        AccessConfig::load_from(&base).unwrap(),
        AccessConfig::empty()
    );
    let absent = base.join("absent");
    assert_eq!(
        AccessConfig::load_from(&absent).unwrap(),
        AccessConfig::empty()
    );
}

#[test]
fn invalid_configurations_are_errors_naming_the_file() {
    let (_dir, base) = sandbox();
    let cases = [
        ("malformed", "{ \"version\": 1,".to_owned(), "EOF"),
        (
            "unknown field",
            r#"{ "version": 1, "roots": [] }"#.to_owned(),
            "unknown field",
        ),
        (
            "missing version",
            r#"{ "read": [] }"#.to_owned(),
            "missing field",
        ),
        (
            "version 2",
            r#"{ "version": 2 }"#.to_owned(),
            "unsupported version 2",
        ),
        (
            "relative path",
            r#"{ "version": 1, "write": ["relative/folder"] }"#.to_owned(),
            "must be absolute",
        ),
        (
            "over 64 KiB",
            format!("{{ \"version\": 1 }}{}", " ".repeat(64 * 1024)),
            "larger than 65536 bytes",
        ),
    ];
    for (case, json, reason) in cases {
        write_config(&base, &json);
        let message = config_error(&base);
        assert!(message.contains(reason), "{case}: {message}");
    }
    // Exactly 64 KiB still loads.
    let json = r#"{ "version": 1 }"#;
    write_config(
        &base,
        &format!("{json}{}", " ".repeat(64 * 1024 - json.len())),
    );
    assert_eq!(
        AccessConfig::load_from(&base).unwrap(),
        AccessConfig::empty()
    );
}

#[test]
fn a_configuration_that_is_a_folder_is_an_error() {
    let (_dir, base) = sandbox();
    folder(&base, FILE_NAME);
    config_error(&base);
}

#[cfg(unix)]
#[test]
fn a_configuration_fifo_is_refused_without_blocking() {
    let (_dir, base) = sandbox();
    let status = std::process::Command::new("mkfifo")
        .arg(base.join(FILE_NAME))
        .status()
        .unwrap();
    assert!(status.success());
    let (sender, receiver) = std::sync::mpsc::channel();
    let dir = base.clone();
    std::thread::spawn(move || {
        let _ = sender.send(AccessConfig::load_from(&dir));
    });
    // Without O_NONBLOCK the open would wait for a writer forever.
    let result = receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    assert!(matches!(result, Err(GrantError::Config(_))), "{result:?}");
    assert!(config_error(&base).contains("not a regular file"));
}

#[cfg(unix)]
#[test]
fn a_symlinked_configuration_loads() {
    let (_dir, base) = sandbox();
    let read = folder(&base, "read");
    let elsewhere = folder(&base, "elsewhere");
    let target = write_config(
        &elsewhere,
        &serde_json::json!({ "version": 1, "read": [text(&read)] }).to_string(),
    );
    std::os::unix::fs::symlink(target, base.join(FILE_NAME)).unwrap();
    assert_eq!(AccessConfig::load_from(&base).unwrap().read, [read]);
}

fn unprotected() -> Protected {
    Protected::new(Vec::new(), None, None).unwrap()
}

fn sources(config: AccessConfig, read: &[PathBuf], write: &[PathBuf], cwd: &Path) -> GrantSources {
    GrantSources {
        config,
        cli_read: read.to_vec(),
        cli_write: write.to_vec(),
        cwd: cwd.to_path_buf(),
    }
}

/// Grant `path` for reading and, separately, for writing under `protected`.
fn refusals(protected: &Protected, path: &Path) -> [Option<GrantError>; 2] {
    let grant = |read: &[PathBuf], write: &[PathBuf]| {
        Grants::from_sources(
            &sources(AccessConfig::empty(), read, write, Path::new("")),
            protected,
        )
        .err()
    };
    let path = [path.to_path_buf()];
    [grant(&path, &[]), grant(&[], &path)]
}

fn assert_refused(protected: &Protected, path: &Path, expected: GrantError) {
    assert_eq!(
        refusals(protected, path),
        [Some(expected.clone()), Some(expected)],
        "{}",
        path.display()
    );
}

fn assert_allowed(protected: &Protected, path: &Path) {
    assert_eq!(
        refusals(protected, path),
        [None, None],
        "{}",
        path.display()
    );
}

#[test]
fn configuration_and_command_line_folders_are_united_once_per_folder() {
    let (_dir, base) = sandbox();
    let a = folder(&base, "a");
    let b = folder(&base, "b");
    let c = folder(&base, "c");
    let config = AccessConfig {
        version: 1,
        read: vec![a.clone(), b.clone()],
        write: vec![b.clone()],
    };
    // Relative command-line folders resolve against cwd; duplicates of a
    // canonical folder keep the first spelling.
    let read = [PathBuf::from("a"), c.clone(), base.join("c").join(".")];
    let write = [PathBuf::from("b"), PathBuf::from("c")];
    let grants =
        Grants::from_sources(&sources(config, &read, &write, &base), &unprotected()).unwrap();
    assert_eq!(
        grants.summary(),
        GrantSummary {
            read: vec![text(&a), text(&b), text(&c)],
            write: vec![text(&b), text(&base.join("c"))],
        }
    );
    // Write roots still never imply read.
    let cli_only = Grants::from_sources(
        &sources(AccessConfig::empty(), &[], slice::from_ref(&c), &base),
        &unprotected(),
    )
    .unwrap();
    assert!(cli_only.read.is_empty());
    assert_eq!(cli_only.write.len(), 1);
}

#[cfg(unix)]
#[test]
fn a_symlink_and_its_target_are_one_folder() {
    let (_dir, base) = sandbox();
    let real = folder(&base, "real");
    let alias = base.join("alias");
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let grants = Grants::from_sources(
        &sources(AccessConfig::empty(), &[alias.clone(), real], &[], &base),
        &unprotected(),
    )
    .unwrap();
    assert_eq!(grants.summary().read, [text(&alias)]);
}

#[test]
fn a_missing_command_line_folder_is_an_error() {
    let (_dir, base) = sandbox();
    let result = Grants::from_sources(
        &sources(
            AccessConfig::empty(),
            &[PathBuf::from("absent")],
            &[],
            &base,
        ),
        &unprotected(),
    );
    assert_eq!(
        result.unwrap_err(),
        GrantError::NotFound(base.join("absent"))
    );
}

#[test]
fn file_system_roots_are_too_broad() {
    let root = PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" });
    assert_refused(&unprotected(), &root, GrantError::TooBroad(root.clone()));
}

#[test]
fn home_and_its_ancestors_are_too_broad() {
    let (_dir, base) = sandbox();
    let users = folder(&base, "users");
    let home = folder(&users, "me");
    let documents = folder(&home, "documents");
    let protected = Protected::new(Vec::new(), Some(home.clone()), None).unwrap();
    for refused in [&home, &users, &base] {
        assert_refused(&protected, refused, GrantError::TooBroad(refused.clone()));
    }
    assert_allowed(&protected, &documents);
    assert_allowed(&protected, &folder(&users, "someone"));
}

#[test]
fn protected_folders_cannot_be_granted_inside_or_around() {
    let (_dir, base) = sandbox();
    let parent = folder(&base, "parent");
    let data = folder(&parent, "data");
    let inside = folder(&data, "inside");
    let sibling = folder(&parent, "sibling");
    let protected = Protected::new(vec![data.clone()], None, None).unwrap();
    for refused in [&data, &inside, &parent, &base] {
        assert_refused(&protected, refused, GrantError::Protected(refused.clone()));
    }
    assert_allowed(&protected, &sibling);
}

#[test]
fn an_absent_data_folder_stays_protected() {
    let (_dir, base) = sandbox();
    let parent = folder(&base, "parent");
    let data = parent.join("data");
    let protected = Protected::new(vec![data.clone()], None, None).unwrap();
    assert_eq!(protected.paths, slice::from_ref(&data));
    assert_refused(&protected, &parent, GrantError::Protected(parent.clone()));
    // Once the app creates it, it is protected itself.
    fs::create_dir(&data).unwrap();
    assert_refused(&protected, &data, GrantError::Protected(data.clone()));
}

#[test]
fn projection_canonicalizes_the_existing_part_and_appends_the_rest() {
    let (_dir, base) = sandbox();
    let real = folder(&base, "real");
    assert_eq!(project(&real).unwrap(), real);
    assert_eq!(
        project(&real.join("a").join("b")).unwrap(),
        real.join("a").join("b")
    );
    assert_eq!(
        project(&real.join("a").join("..").join("b")).unwrap(),
        real.join("b")
    );
    // Nothing can exist under a file, so that is absence too.
    let file = base.join("file");
    fs::write(&file, b"").unwrap();
    assert_eq!(project(&file.join("x")).unwrap(), file.join("x"));
    #[cfg(unix)]
    {
        let alias = base.join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        assert_eq!(project(&alias.join("absent")).unwrap(), real.join("absent"));
        // A data folder reached through a symlink protects the real folder.
        let protected = Protected::new(vec![alias.join("absent")], None, None).unwrap();
        assert_refused(&protected, &real, GrantError::Protected(real.clone()));
    }
}

#[cfg(unix)]
#[test]
fn projection_follows_a_symlink_reached_through_dotdot_after_an_absent_folder() {
    let (_dir, base) = sandbox();
    let outside = folder(&base, "outside");
    let inner = folder(&base, "inner");
    std::os::unix::fs::symlink(&outside, inner.join("alias")).unwrap();
    // Once the app creates `missing`, `..` leads back to `inner` and
    // `alias` leads outside, so the data lands in `outside`.
    let data = inner.join("missing").join("..").join("alias").join("data");
    let protected = Protected::new(vec![data.clone()], None, None).unwrap();
    assert_eq!(protected.paths, [outside.join("data")]);
    assert_refused(&protected, &outside, GrantError::Protected(outside.clone()));
    fs::create_dir_all(&data).unwrap();
    assert!(outside.join("data").is_dir());
}

#[test]
fn the_system_root_and_everything_inside_it_are_protected() {
    let (_dir, base) = sandbox();
    let windows = folder(&base, "Windows");
    let system32 = folder(&windows, "System32");
    let protected = Protected::new(Vec::new(), None, Some(windows.clone())).unwrap();
    for refused in [&windows, &system32] {
        assert_refused(&protected, refused, GrantError::Protected(refused.clone()));
    }
    assert_allowed(&protected, &folder(&base, "Users"));
}

#[cfg(unix)]
#[test]
fn unix_system_trees_are_protected() {
    let dev = PathBuf::from("/dev");
    assert_refused(&unprotected(), &dev, GrantError::Protected(dev.clone()));
}

#[test]
fn the_macos_app_bundle_encloses_its_executable_folder() {
    let applications = PathBuf::from(absolute("Applications"));
    let bundle = applications.join("ReShiki.app");
    let executables = bundle.join("Contents").join("MacOS");
    assert_eq!(app_bundle(&executables), Some(bundle));
    assert_eq!(app_bundle(&applications), None);
    assert_eq!(app_bundle(&applications.join("MacOS")), None);
    // An ordinary folder with that layout is not a bundle, so its siblings
    // stay grantable.
    let project = PathBuf::from(absolute("project"));
    assert_eq!(app_bundle(&project.join("Contents").join("MacOS")), None);
}

#[cfg(unix)]
#[test]
fn a_protected_location_that_cannot_be_resolved_is_an_error() {
    use std::os::unix::fs::PermissionsExt as _;
    let (_dir, base) = sandbox();
    let locked = folder(&base, "locked");
    let target = folder(&base, "target");
    std::os::unix::fs::symlink(&target, locked.join("data")).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let result = Protected::new(vec![locked.join("data")], None, None);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    // Without search permission the symlink's target is unknown: never guess
    // it from the spelling. (Root ignores the permission and finds it.)
    match result {
        Err(GrantError::Config(message)) => assert!(message.contains("data"), "{message}"),
        Ok(protected) => assert_eq!(protected.paths, [target]),
        Err(other) => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn this_process_protects_its_executable_folder_and_home() {
    let protected = Protected::current().unwrap();
    let executable = std::env::current_exe().unwrap();
    let folder = executable.parent().unwrap().to_path_buf();
    assert_refused(&protected, &folder, GrantError::Protected(folder.clone()));
    if let Some(home) = reshiki_io::compatibility::home_directory().filter(|home| home.is_dir()) {
        assert_refused(&protected, &home, GrantError::TooBroad(home.clone()));
    }
}

fn subset(choices: &[PathBuf], mask: u32) -> Vec<PathBuf> {
    choices
        .iter()
        .enumerate()
        .filter(|(index, _)| mask >> index & 1 == 1)
        .map(|(_, path)| path.clone())
        .collect()
}

fn inside_any(path: &Path, folders: &[PathBuf]) -> bool {
    folders.iter().any(|folder| path.starts_with(folder))
}

#[test]
fn narrowing_only_ever_shrinks_a_grant_set() {
    let (_dir, base) = sandbox();
    let a = folder(&base, "a");
    let b = folder(&a, "b");
    let c = folder(&b, "c");
    let d = folder(&base, "d");
    let files: Vec<PathBuf> = [&base, &a, &b, &c, &d]
        .into_iter()
        .map(|dir| {
            let file = dir.join("f.mol");
            fs::write(&file, b"mol").unwrap();
            file
        })
        .collect();
    let root_choices = [base.clone(), a.clone(), b.clone(), d.clone()];
    // Nested, overlapping, missing and not-a-folder limits.
    let limit_choices = [
        base.clone(),
        a.clone(),
        b.clone(),
        c.clone(),
        d.clone(),
        a.join("missing"),
        b.join("f.mol"),
    ];
    for root_mask in 0..1u32 << root_choices.len() {
        let roots = subset(&root_choices, root_mask);
        let grants = Grants::open(&roots, &roots).unwrap();
        for limit_mask in 0..1u32 << limit_choices.len() {
            let limits = subset(&limit_choices, limit_mask);
            let folders: Vec<PathBuf> = limits.iter().filter(|l| l.is_dir()).cloned().collect();
            let narrowed = grants.narrowed(&limits);
            let label = format!("roots {roots:?}, limits {limits:?}");
            for root in narrowed.read.iter().chain(&narrowed.write) {
                assert!(inside_any(&root.canonical, &roots), "{label}");
                assert!(inside_any(&root.canonical, &folders), "{label}");
            }
            for file in &files {
                let expected = inside_any(file, &roots) && inside_any(file, &folders);
                let request = text(file);
                assert_eq!(
                    narrowed.read(&request, 64, FORMATS).is_ok(),
                    expected,
                    "{label}: read {request}"
                );
                let located = narrowed.locate(Kind::Write, &parse(&request).unwrap());
                assert_eq!(located.is_ok(), expected, "{label}: write {request}");
            }
        }
    }
}

#[test]
fn narrowing_keeps_read_and_write_separate() {
    let (_dir, base) = sandbox();
    let a = folder(&base, "a");
    let inner = folder(&a, "inner");
    let d = folder(&base, "d");
    let grants = Grants::open(slice::from_ref(&a), slice::from_ref(&d)).unwrap();
    let narrowed = grants.narrowed(&[inner.clone(), d.clone()]);
    assert_eq!(
        narrowed.summary(),
        GrantSummary {
            read: vec![text(&inner)],
            write: vec![text(&d)],
        }
    );
    // A root kept whole shares the original handle.
    assert!(Arc::ptr_eq(&grants.write[0].dir, &narrowed.write[0].dir));
    assert!(grants.narrowed(&[]).summary().read.is_empty());
}

#[cfg(unix)]
#[test]
fn narrowing_opens_sub_roots_through_the_root_handle() {
    let (_dir, base) = sandbox();
    let root = folder(&base, "root");
    let outside = folder(&base, "outside");
    fs::write(outside.join("secret.mol"), b"secret").unwrap();
    let real = folder(&root, "real");
    fs::write(real.join("f.mol"), b"inside").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
    std::os::unix::fs::symlink("real", root.join("alias")).unwrap();
    let grants = both(&root);
    // A symlink leaving the root cannot become a sub-root.
    let escaped = grants.narrowed(&[root.join("escape")]);
    assert!(escaped.read.is_empty() && escaped.write.is_empty());
    // One that stays inside resolves through the handle.
    let alias = grants.narrowed(&[root.join("alias")]);
    let request = text(&root.join("alias").join("f.mol"));
    assert_eq!(alias.read(&request, 64, FORMATS).unwrap(), b"inside");
    let outside_request = text(&outside.join("secret.mol"));
    assert_eq!(
        alias
            .read(&outside_request, 64, FORMATS)
            .unwrap_err()
            .code(),
        "path_not_granted"
    );
}

#[test]
fn narrowing_judges_a_relative_grant_by_where_it_resolves() {
    let (_dir, base) = sandbox();
    let a = folder(&base, "a");
    let b = folder(&base, "b");
    fs::write(b.join("f.mol"), b"mol").unwrap();
    // Granted from cwd `a` as `../b`: the root is `b`, spelled `a/../b`.
    let up = [PathBuf::from("..").join("b")];
    let grants = Grants::from_sources(
        &sources(AccessConfig::empty(), &up, &up, &a),
        &unprotected(),
    )
    .unwrap();
    let request = text(&b.join("f.mol"));
    let created = text(&b.join("new.mol"));
    assert_eq!(grants.read(&request, 64, FORMATS).unwrap(), b"mol");
    // `a/../b` lies in `b`, not `a`: narrowing to `a` keeps nothing, and
    // neither does a limit spelled with `..`.
    for limits in [slice::from_ref(&a), &[a.join("..").join("b")]] {
        let narrowed = grants.narrowed(limits);
        assert!(narrowed.read.is_empty() && narrowed.write.is_empty());
        assert_eq!(
            narrowed.read(&request, 64, FORMATS).unwrap_err().code(),
            "path_not_granted"
        );
        assert_eq!(
            narrowed
                .write_atomic(&created, b"new", WriteMode::CreateNew, FORMATS)
                .unwrap_err()
                .code(),
            "path_not_granted"
        );
    }
    // Narrowing to where it really is keeps it for reading and writing.
    let kept = grants.narrowed(slice::from_ref(&b));
    assert_eq!(kept.read(&request, 64, FORMATS).unwrap(), b"mol");
    kept.write_atomic(&created, b"new", WriteMode::CreateNew, FORMATS)
        .unwrap();
    assert_eq!(fs::read(b.join("new.mol")).unwrap(), b"new");
}
