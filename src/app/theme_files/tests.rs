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
#[test]
fn exported_theme_bytes_are_the_saved_file() {
    // The browser downloads these bytes instead of calling `save`.
    let theme = theme_files::bundled().unwrap().remove(0);
    let bytes = theme_files::to_bytes(&theme).unwrap();
    assert_eq!(serde_json::from_slice::<ThemeFile>(&bytes).unwrap(), theme);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exported.reshiki-theme");
    theme_files::save(&path, &theme).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn canvas_preference_follows_explicit_choices_not_document_history_or_opening() {
    use reshiki::canvas_theme::CanvasTheme::{Dark, Light};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let original = app.tab.doc.clone();
    let _ = app.update(Message::CanvasTheme(Dark));
    assert_eq!(app.tab.doc.canvas_theme, Dark);
    assert_eq!(app.appearance.canvas_theme, Dark);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.appearance.canvas_theme, Dark);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc.canvas_theme, Dark);
    let _ = app.update(Message::Undo);

    // New preserves the old tab's Undo/Redo and seeds a clean new one.
    let original_tab = app.tab.id;
    let _ = app.update(Message::New);
    assert_eq!(app.tab.doc.canvas_theme, Dark);
    assert!(!app.dirty());
    let fresh = app.tab.id;
    let _ = app.update(Message::New);
    assert_eq!(app.tab.id, fresh, "An unchanged new drawing is reused");
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Close(Some(
        original_tab,
    ))));
    // Closing the last tab also starts with the remembered brightness.
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Close(None)));
    assert_eq!(app.tab.doc.canvas_theme, Dark);
    assert!(!app.dirty());

    // Opening a light drawing keeps its canvas and does not replace the default.
    app.show_opened(original.clone(), Some("light.rsk".into()));
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.appearance.canvas_theme, Dark);
    let before = app.tab.doc.clone();
    app.tab
        .doc
        .add_atom("O", reshiki::document::Point::default());
    app.changed(before);
    let source = app.tab.id;
    let _ = app.update(Message::New);
    assert_ne!(app.tab.id, source);
    assert_eq!(app.tab.doc.canvas_theme, Dark);
    assert!(!app.dirty());
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Select(source)));
    assert_eq!(app.tab.doc.canvas_theme, Light);
    assert_eq!(app.appearance.canvas_theme, Dark);

    let _ = app.update(Message::CanvasTheme(Light));
    assert_eq!(
        app.appearance.canvas_theme, Light,
        "An explicit choice on an already-light file is remembered too"
    );
}

#[test]
fn interface_preference_remains_independent_of_remembered_canvas() {
    use crate::appearance::Mode;
    use reshiki::canvas_theme::CanvasTheme::{Dark, Light};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Appearance(Mode::Light));
    let _ = app.update(Message::CanvasTheme(Dark));
    assert_eq!(app.appearance.mode, Mode::Light);
    assert!(!crate::appearance::is_dark(&app.theme()));
    let _ = app.update(Message::New);
    assert_eq!(app.tab.doc.canvas_theme, Dark);
    assert!(!crate::appearance::is_dark(&app.theme()));
    let _ = app.update(Message::Appearance(Mode::Dark));
    let _ = app.update(Message::CanvasTheme(Light));
    assert!(crate::appearance::is_dark(&app.theme()));
    let _ = app.update(Message::Appearance(Mode::MatchCanvas));
    assert!(!crate::appearance::is_dark(&app.theme()));
    let _ = app.update(Message::CanvasTheme(Dark));
    assert!(crate::appearance::is_dark(&app.theme()));
}

#[test]
fn recovering_a_light_draft_preserves_it_without_replacing_dark_preference() {
    use reshiki::canvas_theme::CanvasTheme::{Dark, Light};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::CanvasTheme(Dark));
    let _ = app.update(Message::Undo);
    let mut recovered = reshiki::document::Document::default();
    recovered.add_atom("O", reshiki::document::Point::default());
    let dir = tempfile::tempdir().unwrap();
    app.recovered.push(reshiki::recovery::Candidate {
        path: dir.path().join("closed-draft.json"),
        snapshot: reshiki::recovery::Snapshot {
            document: recovered.clone(),
            source: None,
            saved_at: 1,
        },
    });
    app.restore_recovered();
    assert_eq!(app.tab.doc.canvas_theme, Light);
    assert_eq!(app.tab.doc.atoms, recovered.atoms);
    assert_eq!(app.appearance.canvas_theme, Dark);
    let _ = app.update(Message::New);
    assert_eq!(app.tab.doc.canvas_theme, Dark);
    assert!(!app.dirty());
}
