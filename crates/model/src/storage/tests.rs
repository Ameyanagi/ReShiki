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
