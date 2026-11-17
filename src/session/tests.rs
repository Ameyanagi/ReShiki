use super::*;

#[test]
fn repeated_open_keeps_path_order_spaces_and_unicode() {
    let args = parse(
        [
            "--open",
            "first drawing.rsk",
            "--open",
            "資料/構造 β.rsk",
            "--open",
        ]
        .map(OsString::from),
    );
    assert_eq!(
        args.paths,
        [PathBuf::from("first drawing.rsk"), "資料/構造 β.rsk".into()]
    );
    assert!(!args.shortcut_examples);
}

#[test]
fn old_single_path_and_shortcut_examples_flags_are_readable() {
    let args = parse(["--unused", "--open", "old drawing.rsk"].map(OsString::from));
    assert_eq!(args.paths, [PathBuf::from("old drawing.rsk")]);
    let args = parse(["--shortcut-examples"].map(OsString::from));
    assert!(args.paths.is_empty());
    assert!(args.shortcut_examples);
    assert!(parse([]).paths.is_empty());
}

#[test]
fn microsoft_365_edit_flag_is_separate_from_legacy_office_and_libreoffice() {
    for (flag, host) in [
        ("--office-addin-edit", Host::Microsoft365),
        ("--office-edit", Host::Office),
        ("--libreoffice-edit", Host::LibreOffice),
    ] {
        let args = parse([flag, "--open", "drawing.rsk"].map(OsString::from));
        assert_eq!(args.office_host, Some(host));
        assert_eq!(args.paths, [PathBuf::from("drawing.rsk")]);
    }
}

#[cfg(unix)]
#[test]
fn paths_do_not_require_utf8() {
    use std::os::unix::ffi::OsStringExt;
    let path = OsString::from_vec(b"drawing-\xff.rsk".to_vec());
    let args = parse([OsString::from("--open"), path.clone()]);
    assert_eq!(args.paths, [PathBuf::from(path)]);
}

#[test]
fn files_in_a_moved_data_folder_open_from_its_new_place() {
    let old = Path::new("/home/me/.local/share/reshiki");
    let new = Path::new("/home/me/.reshiki");
    let mut args = parse(
        [
            "--open",
            "/home/me/.local/share/reshiki/github/octo/lab/files/ring.rsk",
            "--open",
            "/home/me/Documents/ring.rsk",
            "--open",
            "/home/me/.local/share/reshiki-other/ring.rsk",
            "--open",
            "/home/me/.local/share/reshiki/../docs/ring.rsk",
        ]
        .map(OsString::from),
    );
    args.moved(old, new);
    assert_eq!(
        args.paths,
        [
            PathBuf::from("/home/me/.reshiki/github/octo/lab/files/ring.rsk"),
            PathBuf::from("/home/me/Documents/ring.rsk"),
            PathBuf::from("/home/me/.local/share/reshiki-other/ring.rsk"),
            PathBuf::from("/home/me/.local/share/reshiki/../docs/ring.rsk"),
        ]
    );
}
