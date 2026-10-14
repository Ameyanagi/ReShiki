use super::*;

#[test]
fn migration_preserves_originals_and_current_choices_and_does_not_resurrect_drafts()
-> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let old = directory.path().join("old");
    let new = directory.path().join("new");
    std::fs::create_dir_all(old.join("recovery"))?;
    std::fs::create_dir_all(&new)?;
    std::fs::write(old.join("templates.json"), b"templates")?;
    std::fs::write(old.join("assistant-preferences.json"), b"old choice")?;
    std::fs::write(new.join("assistant-preferences.json"), b"new choice")?;
    std::fs::write(old.join("recovery/draft.json"), b"drawing")?;
    migrate_data(&old, &new).map_err(anyhow::Error::msg)?;
    assert_eq!(std::fs::read(new.join("templates.json"))?, b"templates");
    assert_eq!(
        std::fs::read(new.join("assistant-preferences.json"))?,
        b"new choice"
    );
    assert_eq!(std::fs::read(new.join("recovery/draft.json"))?, b"drawing");
    assert_eq!(std::fs::read(old.join("recovery/draft.json"))?, b"drawing");
    std::fs::remove_file(new.join("recovery/draft.json"))?;
    migrate_data(&old, &new).map_err(anyhow::Error::msg)?;
    assert!(!new.join("recovery/draft.json").exists());
    Ok(())
}

#[test]
fn native_extensions_accept_previous_drawings() {
    assert!(is_native_extension("rsk"));
    assert!(is_native_extension("RSK"));
    assert!(is_native_extension("reshiki"));
    assert!(is_native_extension("MORUNO"));
    assert!(!is_native_extension("mol"));
}

#[test]
fn structure_formats_match_known_extensions_without_a_default() {
    for extension in NATIVE_EXTENSIONS {
        assert_eq!(structure_format(extension), Some("reshiki"), "{extension}");
        assert_eq!(
            structure_format(&extension.to_ascii_uppercase()),
            Some("reshiki"),
            "{extension}"
        );
    }
    for format in ["mol", "rxn", "rsmi", "cdxml", "cdx", "inchi"] {
        assert_eq!(structure_format(format), Some(format), "{format}");
        assert_eq!(
            structure_format(&format.to_ascii_uppercase()),
            Some(format),
            "{format}"
        );
    }
    assert_eq!(structure_format("CdXmL"), Some("cdxml"));
    for extension in ["", "smi", "smiles", "txt", "svg", ".mol", "molfile"] {
        assert_eq!(structure_format(extension), None, "{extension:?}");
    }
}

#[test]
fn cdx_import_text_uses_standard_base64_only_for_lower_case_format() {
    assert_eq!(
        import_text("cdx", vec![0xff, 0, 1, 0xfa]),
        Ok("/wAB+g==".into())
    );
    assert_eq!(import_text("cdx", Vec::new()), Ok(String::new()));
    for format in ["CDX", "Cdx", "mol", "smiles", "unknown"] {
        assert_eq!(
            import_text(format, b"file text".to_vec()),
            Ok("file text".into()),
            "{format}"
        );
    }
}

#[test]
fn import_text_preserves_utf8_and_its_error_message() {
    let text = "C=C\n硫\r\n";
    assert_eq!(
        import_text("mol", text.as_bytes().to_vec()),
        Ok(text.into())
    );
    for format in ["CDX", "mol", "smiles", "unknown"] {
        assert_eq!(
            import_text(format, vec![0xff]),
            Err(
                "The file is not valid UTF-8: invalid utf-8 sequence of 1 bytes from index 0"
                    .into()
            ),
            "{format}"
        );
    }
}

#[test]
fn an_override_wins_and_the_missing_directory_error_is_unchanged() {
    let project = || Some(PathBuf::from("/platform/data"));
    let cases = [
        (
            Some(OsString::from("/override")),
            project(),
            Ok(DataLocation {
                path: PathBuf::from("/override"),
                origin: DataLocationOrigin::Override,
            }),
        ),
        (
            Some(OsString::from("/override")),
            None,
            Ok(DataLocation {
                path: PathBuf::from("/override"),
                origin: DataLocationOrigin::Override,
            }),
        ),
        (
            None,
            project(),
            Ok(DataLocation {
                path: PathBuf::from("/platform/data"),
                origin: DataLocationOrigin::Default,
            }),
        ),
        (None, None, Err("No application data directory".to_owned())),
    ];
    for (override_path, project, expected) in cases {
        let label = format!("{override_path:?} {project:?}");
        assert_eq!(
            resolve_data_location(override_path, project),
            expected,
            "{label}"
        );
    }
}

#[test]
fn only_the_platform_directory_imports_earlier_data() {
    let finish = |origin, outcome: Result<(), String>| {
        let calls = std::cell::Cell::new(0);
        let location = DataLocation {
            path: PathBuf::from("/data"),
            origin,
        };
        let result = finish_data_directory(location, |root| {
            assert_eq!(root, Path::new("/data"));
            calls.set(calls.get() + 1);
            outcome
        });
        (result, calls.get())
    };
    assert_eq!(
        finish(DataLocationOrigin::Override, Err("unused".into())),
        (Ok(PathBuf::from("/data")), 0)
    );
    assert_eq!(
        finish(DataLocationOrigin::Default, Ok(())),
        (Ok(PathBuf::from("/data")), 1)
    );
    assert_eq!(
        finish(DataLocationOrigin::Default, Err("copy failed".into())),
        (Err("copy failed".to_owned()), 1)
    );
}

#[test]
fn every_absolute_home_counts_once_in_order() {
    let root = PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" });
    let profile = root.join("Users").join("me");
    let overridden = root.join("tmp").join("home");
    let homes = distinct_homes([
        Some(profile.clone()),
        None,
        Some(PathBuf::new()),
        Some(PathBuf::from("relative")),
        Some(overridden.clone()),
        Some(profile.clone()),
    ]);
    assert_eq!(homes, [profile, overridden]);
    assert_eq!(distinct_homes([None, None]), Vec::<PathBuf>::new());
}

#[test]
fn the_environment_home_is_one_of_the_homes() {
    let homes = home_directories();
    assert!(homes.iter().all(|home| home.is_absolute()), "{homes:?}");
    if let Some(home) = std::env::home_dir().filter(|home| home.is_absolute()) {
        assert!(homes.contains(&home), "{homes:?}");
    }
}
