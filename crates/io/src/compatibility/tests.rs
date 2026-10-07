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
