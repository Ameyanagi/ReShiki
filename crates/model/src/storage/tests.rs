use super::*;
#[test]
fn save_names_keep_explicit_suffixes_and_gain_missing_format_extensions() {
    for (name, extension, expected) in [
        ("Ethanol", "rsk", "Ethanol.rsk"),
        ("Ethanol.", "rsk", "Ethanol.rsk"),
        ("Ethanol.RSK", "rsk", "Ethanol.RSK"),
        ("Ethanol.RESHIKI", "rsk", "Ethanol.RESHIKI"),
        ("Ethanol.moruno", "rsk", "Ethanol.moruno"),
        ("drawing.custom", "rsk", "drawing.custom"),
        ("反応 図", "svg", "反応 図.svg"),
        ("figure", "pdf", "figure.pdf"),
        ("picture", "png", "picture.png"),
        ("structures", "smi", "structures.smi"),
        (
            "templates",
            "reshiki-templates",
            "templates.reshiki-templates",
        ),
    ] {
        let parent = Path::new("folder.with.dots");
        assert_eq!(
            with_default_extension(&parent.join(name), extension),
            parent.join(expected)
        );
    }
}
#[test]
fn replacing_a_longer_document_does_not_leave_old_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("drawing.reshiki");
    write_atomic(&path, b"previous long document").unwrap();
    write_atomic(&path, b"{}").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"{}");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
#[test]
fn desktop_data_files_are_the_file_system() {
    let directory = tempfile::tempdir().unwrap();
    let themes = directory.path().join("themes");
    create_dir_all(&themes).unwrap();
    write_atomic(&themes.join("a.reshiki-theme"), b"a").unwrap();
    rename(
        &themes.join("a.reshiki-theme"),
        &themes.join("b.reshiki-theme"),
    )
    .unwrap();
    assert_eq!(read_dir(&themes).unwrap(), [themes.join("b.reshiki-theme")]);
    assert_eq!(read(&themes.join("b.reshiki-theme")).unwrap(), b"a");
    remove_file(&themes.join("b.reshiki-theme")).unwrap();
    assert_eq!(
        read(&themes.join("b.reshiki-theme")).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    assert_eq!(browser_data_directory(), None);
}
#[test]
fn browser_keys_are_paths_under_the_browser_root() {
    let root = Path::new(BROWSER_ROOT);
    assert_eq!(browser::key(root).as_deref(), Some(""));
    assert_eq!(
        browser::key(&root.join("themes").join("a.reshiki-theme")).as_deref(),
        Some("themes/a.reshiki-theme")
    );
    assert_eq!(browser::key(&root.join("..").join("x")), None);
    assert_eq!(browser::key(Path::new("/tmp/appearance.json")), None);
    assert_eq!(browser::key(Path::new("Browser storage2/x")), None);
}
#[test]
fn browser_storage_keeps_each_change_and_lists_one_folder() {
    use std::{cell::RefCell, rc::Rc};
    // Without storage, nothing can be written.
    assert!(browser::write("appearance.json", b"{}").is_err());
    let kept = Rc::new(RefCell::new(Vec::new()));
    let sink = kept.clone();
    browser::install(
        [("themes/a.reshiki-theme".to_owned(), b"a".to_vec())],
        Box::new(move |key, bytes| {
            sink.borrow_mut()
                .push((key.to_owned(), bytes.map(<[u8]>::to_vec)))
        }),
    );
    browser::write("themes/b.reshiki-theme", b"b").unwrap();
    browser::write("themes/.deleted/1/c.reshiki-theme", b"c").unwrap();
    browser::write("appearance.json", b"{}").unwrap();
    assert_eq!(
        browser::list("themes"),
        ["a.reshiki-theme", "b.reshiki-theme"]
    );
    assert_eq!(browser::list(""), ["appearance.json"]);
    browser::rename("themes/a.reshiki-theme", "themes/d.reshiki-theme").unwrap();
    assert_eq!(browser::read("themes/d.reshiki-theme").unwrap(), b"a");
    assert_eq!(
        browser::read("themes/a.reshiki-theme").unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    // Another tab's change is remembered but not written back.
    browser::changed("templates.json", Some(b"[]".to_vec()));
    assert_eq!(browser::read("templates.json").unwrap(), b"[]");
    let keys: Vec<_> = browser::files("themes")
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(
        keys,
        [
            "themes/.deleted/1/c.reshiki-theme",
            "themes/b.reshiki-theme",
            "themes/d.reshiki-theme"
        ]
    );
    let kept = kept.borrow();
    let kept: Vec<_> = kept
        .iter()
        .map(|(key, bytes)| (key.as_str(), bytes.as_deref()))
        .collect();
    assert_eq!(
        kept,
        [
            ("themes/b.reshiki-theme", Some(&b"b"[..])),
            ("themes/.deleted/1/c.reshiki-theme", Some(&b"c"[..])),
            ("appearance.json", Some(&b"{}"[..])),
            ("themes/d.reshiki-theme", Some(&b"a"[..])),
            ("themes/a.reshiki-theme", None),
        ]
    );
}
