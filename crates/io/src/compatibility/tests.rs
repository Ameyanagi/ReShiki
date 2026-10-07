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
