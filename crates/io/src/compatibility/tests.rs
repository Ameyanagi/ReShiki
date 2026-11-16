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

/// A byte-order mark hid the `$RXN` header and made InChI look like SMILES.
#[test]
fn import_text_drops_one_leading_byte_order_mark() {
    for (text, format) in [("$RXN\n", "rxn"), ("InChI=1S/CH4/h1H4", "inchi")] {
        let marked = format!("\u{feff}{text}");
        let imported = import_text("auto", marked.into_bytes()).unwrap();
        assert_eq!(imported, text);
        assert_eq!(crate::engine::text_format(&imported), format);
    }
    assert_eq!(
        import_text("smiles", "\u{feff}\u{feff}C".as_bytes().to_vec()),
        Ok("\u{feff}C".into())
    );
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

/// An old-style data folder with a recovery draft and appearance settings.
fn old_folder(root: &Path) -> PathBuf {
    let old = root.join("Application Support").join("dev.reshiki.ReShiki");
    std::fs::create_dir_all(old.join("recovery")).unwrap();
    std::fs::write(old.join("recovery/123-1-0.json"), b"draft").unwrap();
    std::fs::write(old.join("appearance.json"), b"{}").unwrap();
    old
}

fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn the_data_folder_is_dot_reshiki_in_home_on_macos_and_linux() {
    let home = directories_next::BaseDirs::new()
        .unwrap()
        .home_dir()
        .to_path_buf();
    assert_eq!(home_data_directory(), Some(home.join(".reshiki")));
    let known = known_data_directories();
    assert_eq!(known.first(), Some(&home.join(".reshiki")));
    assert!(
        known.contains(&project_data_directory().unwrap()),
        "{known:?}"
    );
}

#[cfg(windows)]
#[test]
fn the_data_folder_is_unchanged_on_windows() {
    assert_eq!(home_data_directory(), None);
    let data = project_data_directory().unwrap();
    assert!(
        data.ends_with(Path::new("reshiki").join("ReShiki").join("data")),
        "{data:?}"
    );
    let cache = cache_in(
        DataLocation {
            path: data,
            origin: DataLocationOrigin::Default,
        },
        directories_next::ProjectDirs::from("dev", "reshiki", "ReShiki")
            .map(|project| project.cache_dir().to_path_buf()),
    );
    assert!(
        cache.ends_with(Path::new("reshiki").join("ReShiki").join("cache")),
        "{cache:?}"
    );
    assert_eq!(move_data_directory(), Ok(()));
}

#[test]
fn the_new_folder_is_used_once_anything_is_there() {
    let root = tempfile::tempdir().unwrap();
    let new = root.path().join(".reshiki");
    let old = root.path().join("old");
    // Nothing anywhere: the new folder.
    assert_eq!(current_or_earlier(new.clone(), Some(old.clone())), new);
    assert_eq!(current_or_earlier(new.clone(), None), new);
    // Only the old folder: it is used where it is.
    std::fs::create_dir(&old).unwrap();
    assert_eq!(current_or_earlier(new.clone(), Some(old.clone())), old);
    // Both: the new one.
    std::fs::create_dir(&new).unwrap();
    assert_eq!(current_or_earlier(new.clone(), Some(old.clone())), new);
    // A file at the new path still counts; an old file is not a folder.
    std::fs::remove_dir(&new).unwrap();
    std::fs::write(&new, b"").unwrap();
    assert_eq!(current_or_earlier(new.clone(), Some(old.clone())), new);
    std::fs::remove_file(&new).unwrap();
    std::fs::remove_dir(&old).unwrap();
    std::fs::write(&old, b"").unwrap();
    assert_eq!(current_or_earlier(new.clone(), Some(old)), new);
}

#[test]
fn the_old_folder_moves_whole_with_its_recovery_drafts() {
    let root = tempfile::tempdir().unwrap();
    let old = old_folder(root.path());
    let new = root.path().join(".reshiki");
    assert_eq!(
        move_data(None, Some(old.clone()), Some(new.clone()), rename),
        Ok(())
    );
    assert!(!present(&old), "the old folder is gone");
    assert_eq!(
        std::fs::read(new.join("recovery/123-1-0.json")).unwrap(),
        b"draft"
    );
    assert_eq!(std::fs::read(new.join("appearance.json")).unwrap(), b"{}");
    assert_eq!(current_or_earlier(new.clone(), Some(old.clone())), new);
    // A second run finds nothing to do.
    let unused = |_: &Path, _: &Path| -> std::io::Result<()> { panic!("no second move") };
    assert_eq!(move_data(None, Some(old), Some(new), unused), Ok(()));
}

#[test]
fn with_both_folders_the_old_one_is_left_alone() {
    let root = tempfile::tempdir().unwrap();
    let old = old_folder(root.path());
    let new = root.path().join(".reshiki");
    std::fs::create_dir(&new).unwrap();
    let unused = |_: &Path, _: &Path| -> std::io::Result<()> { panic!("nothing moves") };
    assert_eq!(
        move_data(None, Some(old.clone()), Some(new.clone()), unused),
        Ok(())
    );
    assert_eq!(
        std::fs::read(old.join("recovery/123-1-0.json")).unwrap(),
        b"draft"
    );
    assert_eq!(std::fs::read_dir(&new).unwrap().count(), 0);
    assert_eq!(current_or_earlier(new.clone(), Some(old)), new);
}

#[test]
fn a_failed_move_keeps_the_old_folder_in_use_and_says_so() {
    let root = tempfile::tempdir().unwrap();
    let old = old_folder(root.path());
    let new = root.path().join(".reshiki");
    let cross_device = |_: &Path, _: &Path| -> std::io::Result<()> {
        Err(std::io::Error::other("Cross-device link"))
    };
    assert_eq!(
        move_data(None, Some(old.clone()), Some(new.clone()), cross_device),
        Err(format!(
            "Kept your data in {}: could not move it to ~/.reshiki (Cross-device link)",
            old.display()
        ))
    );
    assert!(!present(&new), "nothing is left at the new path");
    assert_eq!(
        std::fs::read(old.join("recovery/123-1-0.json")).unwrap(),
        b"draft"
    );
    assert_eq!(current_or_earlier(new, Some(old.clone())), old);
}

/// The real rename out of a folder the user cannot write.
#[cfg(unix)]
#[test]
fn a_read_only_parent_keeps_the_old_folder() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let old = old_folder(root.path());
    let parent = old.parent().unwrap().to_path_buf();
    let new = root.path().join(".reshiki");
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = move_data(None, Some(old.clone()), Some(new.clone()), rename);
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let message = result.unwrap_err();
    assert!(
        message.starts_with(&format!(
            "Kept your data in {}: could not move it to ~/.reshiki (",
            old.display()
        )),
        "{message}"
    );
    assert!(!present(&new));
    assert_eq!(std::fs::read(old.join("appearance.json")).unwrap(), b"{}");
}

#[test]
fn a_move_another_reshiki_made_first_is_no_failure() {
    let root = tempfile::tempdir().unwrap();
    let old = old_folder(root.path());
    let new = root.path().join(".reshiki");
    let raced = |from: &Path, to: &Path| -> std::io::Result<()> {
        std::fs::rename(from, to)?;
        Err(std::io::ErrorKind::NotFound.into())
    };
    assert_eq!(move_data(None, Some(old), Some(new.clone()), raced), Ok(()));
    assert!(present(&new.join("recovery/123-1-0.json")));
}

#[test]
fn nothing_moves_without_an_old_folder_or_with_an_override() {
    let root = tempfile::tempdir().unwrap();
    let new = root.path().join(".reshiki");
    let unused = |_: &Path, _: &Path| -> std::io::Result<()> { panic!("nothing moves") };
    let missing = root.path().join("missing");
    assert_eq!(
        move_data(None, Some(missing), Some(new.clone()), unused),
        Ok(())
    );
    assert_eq!(move_data(None, None, Some(new.clone()), unused), Ok(()));
    let old = old_folder(root.path());
    assert_eq!(
        move_data(
            Some(OsString::from("/override")),
            Some(old.clone()),
            Some(new.clone()),
            unused
        ),
        Ok(())
    );
    assert!(!present(&new));
    assert!(old.join("recovery/123-1-0.json").is_file());
}

#[test]
fn caches_live_in_the_data_folder_except_on_windows_by_default() {
    let at = |origin| DataLocation {
        path: PathBuf::from("/data"),
        origin,
    };
    let platform = || Some(PathBuf::from("/platform/cache"));
    assert_eq!(
        cache_in(at(DataLocationOrigin::Default), None),
        PathBuf::from("/data/cache")
    );
    assert_eq!(
        cache_in(at(DataLocationOrigin::Default), platform()),
        PathBuf::from("/platform/cache")
    );
    assert_eq!(
        cache_in(at(DataLocationOrigin::Override), platform()),
        PathBuf::from("/data/cache")
    );
}
