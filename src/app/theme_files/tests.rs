use super::*;
#[test]
fn library_reload_keeps_all_saved_themes_among_unrelated_entries() {
    let dir = tempfile::tempdir().unwrap();
    let mut theme = theme_files::bundled().unwrap().remove(0);
    for index in 0..300 {
        std::fs::write(dir.path().join(format!("notes-{index}.txt")), "notes").unwrap();
        theme.id = format!("saved-{index}");
        theme_files::save(
            &dir.path().join(format!("{}.reshiki-theme", theme.id)),
            &theme,
        )
        .unwrap();
    }
    std::fs::write(dir.path().join("broken.reshiki-theme"), "not JSON").unwrap();
    let mut state = State::default();
    state.load_from(dir.path());
    assert_eq!(state.themes().len(), 300);
    for index in 0..300 {
        assert!(
            state
                .themes()
                .iter()
                .any(|t| t.id == format!("saved-{index}"))
        );
    }
}
#[test]
fn deletion_survives_reload_and_archives_all_copies_of_only_the_selected_theme() {
    let dir = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let mut theme = theme_files::bundled().unwrap().remove(0);
    theme_files::save(&source.path().join("original.reshiki-theme"), &theme).unwrap();
    theme_files::save(&dir.path().join("manual-name.reshiki-theme"), &theme).unwrap();
    theme.name = "Latest saved name".into();
    theme_files::save(
        &dir.path().join(format!("{}.reshiki-theme", theme.id)),
        &theme,
    )
    .unwrap();
    let mut other = theme.clone();
    other.id = "other".into();
    theme_files::save(&dir.path().join("other.reshiki-theme"), &other).unwrap();
    std::fs::write(dir.path().join("broken.reshiki-theme"), b"not a theme").unwrap();
    let mut state = State::default();
    state.load_from(dir.path());
    assert_eq!(state.themes().len(), 2);
    assert_eq!(
        state.themes().iter().find(|t| t.id == theme.id).unwrap(),
        &theme
    );
    state.archive_from(dir.path(), &theme.id).unwrap();
    let mut reloaded = State::default();
    reloaded.load_from(dir.path());
    assert_eq!(reloaded.themes(), &[other]);
    assert!(source.path().join("original.reshiki-theme").exists());
    assert!(dir.path().join("broken.reshiki-theme").exists());
    let archive = std::fs::read_dir(dir.path().join(".deleted"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(std::fs::read_dir(archive).unwrap().count(), 2);
}
#[test]
fn importing_previews_until_saved_and_stale_dialogs_do_not_change_drawings() {
    let (mut app, _) = App::new();
    let original = app.tab.doc.clone();
    let _ = app.theme_generator_action(super::super::theme_generator::Action::Open);
    let serial = app.theme_library.serial;
    let theme = theme_files::bundled().unwrap().remove(0);
    let _ = app.theme_file_action(Action::Loaded(
        app.tab.file_epoch + 1,
        serial,
        Ok(Some(Box::new(theme.clone()))),
    ));
    assert_eq!(app.tab.doc, original);
    let _ = app.theme_file_action(Action::Loaded(
        app.tab.file_epoch,
        serial,
        Ok(Some(Box::new(theme.clone()))),
    ));
    assert_eq!(app.tab.doc, original);
    assert!(app.theme_library.themes().is_empty());
    let _ = app.theme_generator_action(super::super::theme_generator::Action::Apply);
    let themed = app.tab.doc.clone();
    assert!(themed.custom_theme.is_some());
    assert_eq!(themed.drawing_style, original.drawing_style);
    assert_eq!(themed.canvas_theme, original.canvas_theme);
    assert!(!super::super::same_drawing(&themed, &original));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, themed);
    let _ = app.update(Message::QuickDrawingStyle(
        super::super::document_styles::Choice::Journal(reshiki::document_styles::Preset::Nature),
    ));
    assert!(app.tab.doc.custom_theme.is_some());
    assert_eq!(app.tab.doc.version, 16);
    let _ = app.update(Message::ColorTheme(ColorTheme::Jmol));
    assert!(app.tab.doc.custom_theme.is_none());
}
