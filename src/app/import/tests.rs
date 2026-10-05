use super::*;
use crate::app::InspectorTab;
use crate::canvas::Edit;

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}
fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app
}

#[test]
fn background_file_insertion_keeps_its_status_guards_and_undo() {
    use crate::app::tabs::{Action as Tabs, tests::Front};
    for stale in [false, true] {
        let mut app = ready();
        let id = app.tab.id;
        let ticket = Ticket {
            epoch: app.tab.file_epoch,
            revision: app.tab.revision,
        };
        app.tab.busy = true;
        if stale {
            let before = app.tab.doc.clone();
            app.tab.doc.add_atom("N", Point::default());
            app.changed(before);
        }
        let before = app.tab.doc.clone();
        let front = Front::new(&mut app);
        let mut drawing = Document::default();
        drawing.add_atom("O", Point::default());
        let batch = Batch {
            label: "caffeine.mol".into(),
            drawings: vec![drawing],
            warnings: vec![],
        };
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::Imports(Action::Loaded(
                ticket,
                Box::new(Ok(batch)),
            ))),
        ));
        front.assert_unchanged(&app);
        assert!(!app.tabs.background[0].busy);
        let _ = app.update(Message::Tabs(Tabs::Select(id)));
        if stale {
            assert_eq!(app.tab.doc, before);
            assert!(app.status.contains("changed"));
        } else {
            assert!(app.status.starts_with("Inserted caffeine.mol"));
            assert_eq!(app.tab.doc.atoms.len(), before.atoms.len() + 1);
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[test]
fn files_route_by_extension_and_label_the_drop() {
    let insert = paths(&[
        "caffeine.MOL",
        "b.rxn",
        "c.cdxml",
        "d.cdx",
        "e.smi",
        "f.smiles",
    ]);
    assert_eq!(plan(&insert), Plan::Insert(insert.clone()));
    assert_eq!(plan(&insert).label(), "Drop to insert · 6 files");
    let one = paths(&["caffeine.mol"]);
    assert_eq!(plan(&one).label(), "Drop to insert · caffeine.mol");
    let pictures = paths(&["a.png", "b.JPG", "c.jpeg", "d.tif", "e.tiff", "f.webp"]);
    assert_eq!(plan(&pictures), Plan::Insert(pictures.clone()));
    for native in ["x.rsk", "x.reshiki", "x.moruno"] {
        let path = paths(&[native]);
        assert_eq!(plan(&path), Plan::Open(path.clone()));
    }
    assert_eq!(plan(&paths(&["x.rsk"])).label(), "Drop to open · x.rsk");
    assert_eq!(
        plan(&paths(&["a.rsk", "b.rsk"])).label(),
        "Drop to open · 2 files"
    );
    for (names, reason) in [
        (&["notes.docx"][..], "Can't import .docx"),
        (&["a.mol", "notes.docx"][..], "Can't import .docx"),
        (&["README"][..], "Can't import this file"),
        (
            &["a.rsk", "b.mol"][..],
            "Drop drawings apart from structures and pictures",
        ),
    ] {
        assert_eq!(plan(&paths(names)).label(), reason);
    }
}

#[test]
fn typed_text_reports_its_detected_format() {
    let mut app = ready();
    assert!(app.imports.is_blank());
    let paste = |text: &str| {
        Message::Imports(Action::Edit(text_editor::Action::Edit(
            text_editor::Edit::Paste(text.to_owned().into()),
        )))
    };
    let _ = app.update(paste("InChI=1S/CH4/h1H4"));
    assert_eq!(app.imports.format.map(format_name), Some("InChI"));
    let _ = app.update(Message::Imports(Action::Edit(
        text_editor::Action::SelectAll,
    )));
    let _ = app.update(paste("  "));
    assert!(app.imports.is_blank());
    app.imports.set_text("CCO>>CC=O");
    assert_eq!(app.imports.format.map(format_name), Some("Reaction SMILES"));
}

#[test]
fn structure_files_reject_invalid_text_and_preserve_binary_cdx() {
    for format in ["mol", "rxn", "cdxml", "smiles"] {
        assert!(
            contents(format, vec![b'C', 0xff])
                .unwrap_err()
                .contains("UTF-8")
        );
        assert_eq!(
            contents(format, "Label α".as_bytes().to_vec()).unwrap(),
            "Label α"
        );
    }
    assert_eq!(contents("cdx", vec![0xff, 0x00]).unwrap(), "/wA=");
}

#[test]
fn mixed_empty_batch_is_rejected_without_inserting_any_files() {
    let mut app = ready();
    app.tab.doc.add_atom("N", Point::new(-80., 0.));
    let before = app.tab.doc.clone();
    let mut visible = Document::default();
    visible.add_atom("O", Point::default());
    for drawings in [
        vec![visible.clone(), Document::default()],
        vec![Document::default(), visible],
    ] {
        app.insert_batch(Batch {
            label: "2 files".into(),
            drawings,
            warnings: vec![],
        });
        assert!(app.error && app.status.contains("contains no drawing"));
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn stale_import_does_not_release_a_new_documents_operation() {
    let mut app = ready();
    let ticket = Ticket {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
    };
    assert!(
        app.update(Message::Imports(Action::Files(paths(&["old.mol"]))))
            .units()
            > 0
    );
    app.reset_tab();
    assert_ne!(app.tab.file_epoch, ticket.epoch);
    assert!(app.update(Message::Analyze).units() > 0);
    assert!(app.tab.busy);
    let status = app.status.clone();
    let _ = app.update(Message::Imports(Action::Loaded(
        ticket,
        Box::new(Err("Old error".into())),
    )));
    assert!(app.tab.busy && !app.error);
    assert_eq!(app.status, status);
    assert!(app.tab.doc.all_ids().is_empty());
    assert_eq!(app.update(Message::Analyze).units(), 0);
}

#[test]
fn the_import_tab_focuses_its_box_and_its_menu_closes_like_other_menus() {
    use iced::keyboard::{Key, Modifiers};
    let mut app = ready();
    app.inspector_open = false;
    app.help_open = true;
    let command = Key::Character("i".into());
    let Some(show) = super::super::shortcuts::key_message(&command, &command, Modifiers::COMMAND)
    else {
        panic!("Cmd/Ctrl+I opens Import");
    };
    assert!(matches!(show, Message::Inspector(InspectorTab::Import)));
    assert!(app.update(show).units() > 0, "The text box takes focus");
    assert!(app.inspector_open && !app.help_open);
    assert_eq!(app.inspector_tab, InspectorTab::Import);
    let tool = app.tool;
    let _ = app.update(Message::Imports(Action::Menu(true)));
    let _ = app.update(Message::Escape);
    assert!(!app.imports.menu);
    assert_eq!(app.tool, tool, "Escape only closes the menu");
    let _ = app.update(Message::Imports(Action::Menu(true)));
    let _ = app.update(Message::Canvas(Edit::Hover(Some(Point::default()))));
    assert!(app.imports.menu, "Passive events keep it open");
    let _ = app.update(Message::Imports(Action::Edit(
        text_editor::Action::SelectAll,
    )));
    assert!(!app.imports.menu);
}

#[test]
fn hovered_files_drop_together_and_unsupported_files_change_nothing() {
    let mut app = ready();
    for name in ["a.mol", "b.png", "a.mol"] {
        let _ = app.update(Message::Imports(Action::Hovered(name.into())));
    }
    assert_eq!(app.imports.hovered, paths(&["a.mol", "b.png"]));
    let _ = app.update(Message::Imports(Action::Dropped("a.mol".into())));
    assert!(!app.tab.busy, "Waits for the second file");
    assert_eq!(app.imports.hovered.len(), 2);
    assert!(
        app.update(Message::Imports(Action::Dropped("b.png".into())))
            .units()
            > 0
    );
    assert!(app.tab.busy && app.imports.hovered.is_empty() && app.imports.dropped.is_empty());
    assert_eq!(app.status, "Importing…");

    let mut app = ready();
    app.tab.doc.add_atom("C", Point::default());
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Imports(Action::Hovered("notes.docx".into())));
    let _ = app.update(Message::Imports(Action::Left));
    assert!(app.imports.hovered.is_empty());
    let _ = app.update(Message::Imports(Action::Hovered("notes.docx".into())));
    let _ = app.update(Message::Imports(Action::Dropped("notes.docx".into())));
    assert_eq!(app.tab.doc, before);
    assert!(app.error && !app.tab.busy);
    assert_eq!(app.status, "Can't import .docx");
}

#[tokio::test]
async fn dropped_drawings_open_in_tabs_without_a_save_dialog() {
    let folder = tempfile::tempdir().unwrap();
    let mut app = ready();
    app.tab.doc.add_atom("O", Point::default());
    let mut dropped = vec![];
    for (name, element) in [("first.rsk", "N"), ("second.rsk", "S")] {
        let mut doc = Document::default();
        doc.add_atom(element, Point::default());
        let path = folder.path().join(name);
        std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        dropped.push(path);
    }
    let task = app.update(Message::Imports(Action::Files(dropped.clone())));
    assert!(task.units() > 0 && app.pending.is_none());
    for path in &dropped {
        let opened = super::super::files::read(path.clone()).await;
        let _ = app.update(Message::FilePrepared(opened));
    }
    let paths: Vec<_> = app.strip().map(|tab| tab.path.clone()).collect();
    assert_eq!(
        paths,
        [None, Some(dropped[0].clone()), Some(dropped[1].clone())]
    );
    assert_eq!(app.tab.doc.atoms[0].element, "S");
}

#[tokio::test]
async fn files_insert_side_by_side_at_the_pointer_as_one_undo_step() -> Result<(), String> {
    let folder = tempfile::tempdir().map_err(|e| e.to_string())?;
    let png = folder.path().join("blot.png");
    image::save_buffer(&png, &[0; 4 * 30 * 20], 30, 20, image::ColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let files = vec![
        fixtures.join("ethanol.mol"),
        fixtures.join("native-ethyl-clipboard.cdx"),
        png,
    ];
    let batch = load(LocalEngine::default(), files).await?;
    assert_eq!(batch.label, "3 files");
    assert_eq!(batch.drawings.len(), 3);
    assert_eq!(batch.drawings[0].atoms.len(), 3);
    assert!(!batch.drawings[1].atoms.is_empty(), "Binary CDX");
    assert_eq!(batch.drawings[2].graphics.len(), 1);

    let mut app = ready();
    app.viewport = iced::Size::new(900., 600.);
    app.tab.doc.add_atom("N", Point::new(-300., 0.));
    let at = Point::new(40., 25.);
    let _ = app.update(Message::Canvas(Edit::Hover(Some(at))));
    let before = app.tab.doc.clone();
    let ticket = Ticket {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
    };
    app.tab.busy = true;
    let loaded = |ticket, batch: &Batch| {
        Message::Imports(Action::Loaded(ticket, Box::new(Ok(batch.clone()))))
    };
    let _ = app.update(loaded(ticket, &batch));
    assert!(!app.tab.busy && !app.error, "{}", app.status);
    let after = app.tab.doc.clone();
    assert!(before.atoms.iter().all(|a| after.atom(a.id) == Some(a)));
    assert_eq!(after.graphics.len(), 1);
    let (lo, hi) = reshiki::scene::selection_bounds(&after, &app.tab.selected).ok_or("bounds")?;
    assert!(((lo.x + hi.x) / 2. - at.x).abs() < 1. && ((lo.y + hi.y) / 2. - at.y).abs() < 1.);
    // Left to right in file order.
    let x = |ids: &[u64]| reshiki::scene::selection_bounds(&after, ids).map(|(lo, _)| lo.x);
    let mol: Vec<_> = app.tab.selected.iter().copied().take(3).collect();
    let picture = [after.graphics[0].id];
    assert!(x(&mol) < x(&picture));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);

    // A newer drawing or file wins over a late result.
    for new_file in [false, true] {
        let ticket = Ticket {
            epoch: app.tab.file_epoch,
            revision: app.tab.revision,
        };
        if new_file {
            app.tab.file_epoch += 1;
        } else {
            app.tab.revision += 1;
        }
        let _ = app.update(loaded(ticket, &batch));
        assert_eq!(app.tab.doc, after);
    }
    // A pointer that left the canvas, as it does to reach Finder or the
    // Import tab, leaves files at the view center.
    let mut app = ready();
    let _ = app.update(Message::Canvas(Edit::Hover(Some(at))));
    assert_eq!(app.drop_point(0., 0.), at);
    let _ = app.update(Message::Canvas(Edit::Hover(None)));
    assert_eq!(app.drop_point(0., 0.), app.tab.camera.center);
    // Drawings dropped near an edge move inward rather than scroll the view.
    let edge = app.tab.camera.center.x + app.viewport.width / 2. / app.tab.camera.zoom;
    let _ = app.update(Message::Canvas(Edit::Hover(Some(Point::new(
        edge - 1.,
        at.y,
    )))));
    let x = app.drop_point(200., 0.).x;
    assert!(x + 100. < edge && x > app.tab.camera.center.x, "{x}");
    Ok(())
}

#[tokio::test]
async fn open_reads_binary_cdx() -> Result<(), String> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native-ethyl-clipboard.cdx");
    let mut app = ready();
    let opened = super::super::files::read(path).await;
    let Some((_, Ok(super::super::files::Prepared::Import { format, contents }))) = &opened else {
        return Err(format!("{opened:?}"));
    };
    assert_eq!(*format, "cdx");
    let response = LocalEngine::default()
        .request(Request::import(format, contents))
        .await?;
    assert!(app.update(Message::FilePrepared(opened.clone())).units() > 0);
    assert!(app.tab.busy, "{}", app.status);
    assert!(!response.document.ok_or("drawing")?.atoms.is_empty());
    Ok(())
}
