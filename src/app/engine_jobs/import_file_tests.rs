use super::*;
use crate::app::{
    files::Prepared,
    prompt::{Answer, Prompt},
    tabs,
};

fn ready() -> App {
    let (mut app, _) = App::new();
    app.cancel_engine_job();
    app.tab.busy = false;
    app
}

fn open(app: &mut App, name: &str, format: &'static str) -> Key {
    let path = std::path::Path::new("imports").join(name);
    let task = app.update(Message::FilePrepared(Some((
        path,
        Ok(Prepared::Import {
            format,
            contents: String::new(),
            loss: Default::default(),
        }),
    ))));
    assert!(task.units() > 0);
    app.tab.engine_job.as_ref().unwrap().key
}

fn imported(key: Key) -> Message {
    let mut document = Document::default();
    document.add_atom("O", Point::default());
    Message::EngineFinished {
        key,
        result: Box::new(Ok(Response {
            document: Some(document),
            analysis: None,
            output: None,
            engine_version: "test".into(),
            warnings: vec![],
        })),
    }
}

#[test]
fn imported_files_use_source_basenames_until_native_save_as() {
    for (name, format) in [
        ("co2.cif", "cif"),
        ("rutile.vasp", "poscar"),
        ("POSCAR", "poscar"),
        ("CONTCAR", "poscar"),
        ("urea.pdb", "pdb"),
        ("water.xyz", "xyz"),
        ("silicon.extxyz", "xyz"),
        ("日本語.CIF", "cif"),
    ] {
        let mut app = ready();
        let key = open(&mut app, name, format);
        let _ = app.update(imported(key));
        assert_eq!(app.tab.name(), name);
        assert_eq!(app.title(), format!("{name} • — ReShiki"));
        assert!(app.dirty());
        assert_eq!(
            app.drawing_save_target(false),
            (
                None,
                format!(
                    "{}.rsk",
                    std::path::Path::new(name)
                        .file_stem()
                        .unwrap()
                        .to_string_lossy()
                )
            )
        );

        // Cancelling native Save As keeps the source name without binding its path.
        assert!(app.update(Message::Save).units() > 0);
        let snapshot = Box::new(app.tab.doc.clone());
        let _ = app.update(Message::Saved(app.tab.file_epoch, snapshot, Ok(None)));
        assert!(app.tab.path.is_none());
        assert_eq!(app.tab.name(), name);
        assert!(app.dirty());

        assert!(app.update(Message::Save).units() > 0);
        let snapshot = Box::new(app.tab.doc.clone());
        let _ = app.update(Message::Saved(
            app.tab.file_epoch,
            snapshot,
            Ok(Some("converted.rsk".into())),
        ));
        assert_eq!(app.tab.untitled_name, None);
        assert_eq!(app.title(), "converted.rsk — ReShiki");
        assert_eq!(
            app.drawing_save_target(false),
            (Some("converted.rsk".into()), "converted.rsk".into())
        );
        let _ = app.update(Message::New);
        assert_eq!(app.tab.name(), "Untitled");
        assert_eq!(app.title(), "Untitled — ReShiki");
    }
}

#[test]
fn chemdraw_and_mol_files_open_unchanged_and_ask_once_how_to_save() {
    for (name, format, label) in [
        ("caffeine.cdxml", "cdxml", "caffeine · CDXML"),
        ("caffeine.CDX", "cdx", "caffeine · CDX"),
        ("aspirin.mol", "mol", "aspirin · MOL"),
    ] {
        let mut app = ready();
        let path = std::path::Path::new("imports").join(name);
        let key = open(&mut app, name, format);
        let _ = app.update(imported(key));
        assert_eq!(app.tab.path.as_ref(), Some(&path));
        assert_eq!(app.title(), format!("{name} — ReShiki"));
        assert_eq!(app.tab.label(), label);
        assert!(!app.dirty() && !app.tab.history.can_undo());

        // The first Save asks; Cancel leaves the question for the next one.
        let _ = app.update(Message::Save);
        assert!(matches!(app.prompt, Some(Prompt::KeepFormat(asked)) if asked == format));
        let _ = app.update(Message::Prompt(Answer::Cancel));
        assert!(app.prompt.is_none() && !app.file_io.saving);
        let _ = app.update(Message::Save);
        assert!(app.update(Message::Prompt(Answer::DontSave)).units() > 0);
        assert!(app.file_io.saving, "Keep as … writes the file in place");
        let snapshot = Box::new(app.tab.doc.clone());
        let _ = app.update(Message::Saved(
            app.tab.file_epoch,
            snapshot,
            Ok(Some(path.clone())),
        ));
        assert_eq!(app.tab.path.as_ref(), Some(&path));

        // The answer holds for this file.
        assert!(app.update(Message::Save).units() > 0);
        assert!(app.prompt.is_none() && app.file_io.saving);
    }
}

#[test]
fn the_format_question_is_about_the_file_its_tab_opened_and_enter_saves_as_reshiki() {
    let mut app = ready();
    let loss = reshiki::formats::ChemdrawLoss {
        schemes: 4,
        steps: 12,
        pages: 0,
        unsupported: 0,
    };
    let task = app.update(Message::FilePrepared(Some((
        std::path::Path::new("imports").join("schemes.cdxml"),
        Ok(Prepared::Import {
            format: "cdxml",
            contents: String::new(),
            loss,
        }),
    ))));
    assert!(task.units() > 0);
    let key = app.tab.engine_job.as_ref().unwrap().key;
    let _ = app.update(imported(key));
    let asked = app.tab.id;
    assert_eq!(app.tab.chemdraw_loss, loss);

    // A file opened beside it has its own answer.
    let other = open(&mut app, "caffeine.cdxml", "cdxml");
    let _ = app.update(imported(other));
    assert_eq!(app.tab.chemdraw_loss, Default::default());
    let _ = app.update(Message::Tabs(tabs::Action::Select(asked)));
    assert_eq!(app.tab.chemdraw_loss, loss);

    // Enter takes the default, Save as ReShiki, which leaves the file alone.
    let _ = app.update(Message::Save);
    assert!(matches!(app.prompt, Some(Prompt::KeepFormat("cdxml"))));
    let enter = crate::app::prompt::modal_key(
        &iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
        iced::keyboard::Modifiers::empty(),
    )
    .unwrap();
    assert!(app.update(enter).units() > 0);
    assert!(app.prompt.is_none() && app.file_io.saving && !app.tab.keep_format);
}

#[test]
fn a_file_opened_during_the_format_question_cancels_it() {
    let mut app = ready();
    let key = open(&mut app, "aspirin.mol", "mol");
    let _ = app.update(imported(key));
    let asked = app.tab.id;
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("N", Point::new(80., 0.));
    app.changed(before);
    let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
    let _ = app.update(Message::Save);
    assert!(matches!(app.prompt, Some(Prompt::KeepFormat("mol"))) && app.pending.is_some());

    // Finder opens another file in front: the answer must not save that one.
    let other = open(&mut app, "caffeine.cdxml", "cdxml");
    let _ = app.update(imported(other));
    assert_ne!(app.tab.id, asked);
    assert!(app.prompt.is_none() && app.pending.is_none());
    assert!(!app.tab.keep_format && !app.file_io.saving);
}

#[test]
fn save_as_reshiki_turns_a_chemdraw_file_into_a_drawing() {
    let mut app = ready();
    let key = open(&mut app, "caffeine.cdxml", "cdxml");
    let _ = app.update(imported(key));
    let _ = app.update(Message::Save);
    assert!(app.update(Message::Prompt(Answer::Save)).units() > 0);
    assert!(app.file_io.saving);
    let snapshot = Box::new(app.tab.doc.clone());
    let _ = app.update(Message::Saved(
        app.tab.file_epoch,
        snapshot,
        Ok(Some("caffeine.rsk".into())),
    ));
    assert_eq!(app.tab.label(), "caffeine.rsk");
    assert!(app.update(Message::Save).units() > 0);
    assert!(app.prompt.is_none());
}

#[test]
fn save_as_lists_the_formats_and_a_chosen_format_is_kept() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("O", Point::default());
    app.changed(before);
    let _ = app.update(Message::SaveAs);
    assert!(matches!(app.prompt, Some(Prompt::SaveAs)) && !app.file_io.saving);
    assert!(matches!(
        Prompt::SaveAs.answer(Answer::Save),
        Message::SaveFormat("rsk")
    ));
    assert!(matches!(
        Prompt::SaveAs.answer(Answer::Cancel),
        Message::Cancel
    ));
    assert!(app.update(Message::Prompt(Answer::Format("mol"))).units() > 0);
    assert!(app.prompt.is_none() && app.file_io.saving);
    let snapshot = Box::new(app.tab.doc.clone());
    let _ = app.update(Message::Saved(
        app.tab.file_epoch,
        snapshot,
        Ok(Some("water.mol".into())),
    ));
    assert_eq!(app.tab.label(), "water · MOL");
    assert!(!app.dirty());
    // Saving as MOL was the answer: Save does not ask again.
    assert!(app.update(Message::Save).units() > 0);
    assert!(app.prompt.is_none());
}

#[test]
fn opening_an_open_chemdraw_file_again_shows_its_tab() {
    let mut app = ready();
    let key = open(&mut app, "caffeine.cdxml", "cdxml");
    let _ = app.update(imported(key));
    let first = app.tab.id;
    let _ = app.update(Message::New);
    let _ = app.update(Message::FilePrepared(Some((
        std::path::Path::new("imports").join("caffeine.cdxml"),
        Ok(Prepared::Import {
            format: "cdxml",
            contents: String::new(),
            loss: Default::default(),
        }),
    ))));
    assert_eq!(app.tab.id, first);
    assert_eq!(app.strip().count(), 2);
    assert_eq!(app.status, "caffeine.cdxml is already open");
}

#[test]
fn separate_imports_keep_their_names_when_completed_in_the_background() {
    let mut app = ready();
    let first = open(&mut app, "co2.cif", "cif");
    let second = open(&mut app, "rutile.vasp", "poscar");
    assert_ne!(first.tab, second.tab);
    let _ = app.update(Message::Tab(first.tab, Box::new(imported(first))));
    let _ = app.update(imported(second));
    assert_eq!(
        app.strip().map(|tab| tab.name()).collect::<Vec<_>>(),
        ["co2.cif", "rutile.vasp"]
    );
    assert!(app.strip().all(|tab| tab.path.is_none() && app.edited(tab)));
    assert_eq!(app.title(), "rutile.vasp • — ReShiki");
    let _ = app.update(Message::Tabs(tabs::Action::Select(first.tab)));
    assert_eq!(app.title(), "co2.cif • — ReShiki");
    let _ = app.update(Message::New);
    assert_eq!(app.title(), "Untitled — ReShiki");
    assert_eq!(
        app.strip().map(|tab| tab.name()).collect::<Vec<_>>(),
        ["co2.cif", "rutile.vasp", "Untitled"]
    );
    assert!(app.tabs.background.iter().all(|tab| tab.edited));
}

#[test]
fn unsuccessful_imports_do_not_install_a_source_name() {
    for outcome in ["cancel", "error", "stale"] {
        let mut app = ready();
        let key = open(&mut app, "co2.cif", "cif");
        match outcome {
            "cancel" => {
                let _ = app.update(Message::CancelEngine(key));
                let _ = app.update(imported(key));
            }
            "error" => {
                let _ = app.update(Message::EngineFinished {
                    key,
                    result: Box::new(Err("Could not parse this structure".into())),
                });
            }
            "stale" => {
                let before = app.tab.doc.clone();
                app.tab.doc.add_atom("N", Point::default());
                app.changed(before);
                let _ = app.update(imported(key));
            }
            _ => unreachable!(),
        }
        assert_eq!(app.tab.name(), "Untitled");
        assert!(app.tab.path.is_none());
        if outcome != "stale" {
            assert!(app.tab.reusable());
        }
    }
}

#[test]
fn imported_files_open_on_the_remembered_canvas() {
    use reshiki::canvas_theme::CanvasTheme;
    for (name, format) in [("caffeine.cdxml", "cdxml"), ("co2.cif", "cif")] {
        let mut app = ready();
        let _ = app.update(Message::CanvasTheme(CanvasTheme::Dark));
        assert!(
            app.status
                .ends_with("Dark canvas · Exports: canvas colors · Copies: transparent"),
            "{}",
            app.status
        );
        // A light drawing in front: the import still takes the remembered canvas.
        let _ = app.update(Message::CanvasTheme(CanvasTheme::Light));
        app.appearance.canvas_theme = CanvasTheme::Dark;
        let key = open(&mut app, name, format);
        let _ = app.update(imported(key));
        assert_eq!(app.tab.doc.canvas_theme, CanvasTheme::Dark, "{name}");
        if format == "cdxml" {
            assert!(!app.dirty(), "an opened ChemDraw file stays unchanged");
        }
    }
}
