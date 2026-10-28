use super::*;
use crate::app::{files::Prepared, tabs};

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
        ("ethanol.mol", "mol"),
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
