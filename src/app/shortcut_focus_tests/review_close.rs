//! Add a case here whenever a review, preview or proposal command is added:
//! closing it must preserve selection and return keyboard input to the drawing.
use super::*;
use reshiki::document::Point;

mod assistant;
mod proposals;

fn drawing_app() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 13.));
    let c = app.tab.doc.add_atom("O", Point::new(70., -20.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.add_bond(b, c, 1, "plain");
    for (id, hydrogens) in [(a, 3), (b, 2), (c, 1)] {
        app.tab.doc.atom_mut(id).unwrap().label_h = hydrogens;
    }
    app.tab.selected = vec![a, b, c];
    app.tab.saved = app.tab.doc.clone();
    app.sync_keyboard_drawing();
    app
}

async fn assert_canvas_delete(ui: &mut Ui, app: &mut App) {
    ui.assert_unfocused(app);
    let drawing = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    assert!(!selected.is_empty());
    let remaining = drawing
        .atoms
        .iter()
        .filter(|atom| !selected.contains(&atom.id))
        .count();
    let revision = app.tab.revision;
    ui.key(
        app,
        Key::Named(Named::Delete),
        Code::Delete,
        Modifiers::empty(),
        None,
    )
    .await;
    assert_ne!(app.tab.doc, drawing, "Delete reaches the drawing");
    assert_eq!(app.tab.doc.atoms.len(), remaining);
    assert!(
        selected
            .iter()
            .all(|id| !app.tab.doc.all_ids().contains(id))
    );
    assert!(app.tab.selected.is_empty());
    assert!(app.tab.revision > revision);
}

async fn close(ui: &mut Ui, app: &mut App, apply: bool) {
    let (key, code) = if apply {
        (Named::Enter, Code::Enter)
    } else {
        (Named::Escape, Code::Escape)
    };
    let worker = app.optimization_key_for_test();
    let (_, messages) = ui.event(
        app,
        press(Key::Named(key), code, Modifiers::empty(), None),
        mouse::Cursor::Unavailable,
    );
    for message in messages {
        let task = app.update(message);
        let task = if let Some(key) = app.optimization_key_for_test()
            && Some(key) != worker
        {
            // Complete Apply's asynchronous check without launching a worker.
            drop(task);
            app.update(Message::Optimization(
                crate::app::optimization::Action::WorkerDone(key, Ok(optimized())),
            ))
        } else {
            task
        };
        ui.run_task(app, task).await;
    }
}

async fn open_cleanup(ui: &mut Ui, app: &mut App) {
    ui.key(
        app,
        Key::Character("K".into()),
        Code::KeyK,
        Modifiers::COMMAND | Modifiers::SHIFT,
        Some("K"),
    )
    .await;
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.cleanup.is_some(), "{}", app.status);
    assert!(!app.tab.busy);
}

async fn open_optimization(ui: &mut Ui, app: &mut App) {
    use crate::app::optimization;

    let (_, messages) = ui.event(
        app,
        press(
            Key::Character("D".into()),
            Code::KeyD,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Some("D"),
        ),
        mouse::Cursor::Unavailable,
    );
    assert!(matches!(
        messages.as_slice(),
        [Message::Optimization(optimization::Action::Begin)]
    ));
    // Complete generation locally, as optimization::tests does.
    for message in messages {
        let _ = app.update(message);
    }
    let key = app.optimization_key_for_test().expect("generation started");
    let drawing = app.tab.doc.clone();
    close(ui, app, true).await;
    assert_eq!(app.optimization_key_for_test(), Some(key));
    assert_eq!(app.tab.doc, drawing, "Enter waits for a ready conformer");
    let task = app.update(Message::Optimization(optimization::Action::WorkerDone(
        key,
        Ok(optimized()),
    )));
    ui.run_task(app, task).await;
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.optimization.is_some());
    assert!(app.optimization_key_for_test().is_none());
    assert!(
        ui.semantics(app)
            .nodes
            .iter()
            .any(|node| node.id == "optimization.apply" && node.enabled)
    );
}

fn optimized() -> std::sync::Arc<reshiki::geometry::Optimized> {
    use reshiki::geometry::{Conformer, ForceField, Optimized, Point3};

    std::sync::Arc::new(Optimized {
        conformer: Conformer {
            positions: vec![
                Point3::default(),
                Point3 {
                    x: 1.5,
                    y: 0.,
                    z: 0.4,
                },
                Point3 {
                    x: 2.1,
                    y: 1.2,
                    z: 0.6,
                },
            ],
            original_atom_count: 3,
            hydrogen_parents: vec![],
        },
        initial_energy: 2.,
        energy: 1.,
        gradient: None,
        converged: true,
        iterations: 20,
        force_field: ForceField::Mmff94,
        diagnostics: vec![],
    })
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_cleanup() {
    let mut ui = Ui::new().await;
    for apply in [true, false] {
        let mut app = drawing_app();
        let drawing = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        open_cleanup(&mut ui, &mut app).await;
        let preview = app.tab.cleanup.as_ref().unwrap().document.clone();
        assert_eq!(app.tab.doc, drawing);
        close(&mut ui, &mut app, apply).await;
        assert!(app.tab.cleanup.is_none());
        assert_eq!(app.tab.doc, if apply { preview } else { drawing });
        assert_eq!(app.tab.selected, selected);
        assert_canvas_delete(&mut ui, &mut app).await;
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_optimization() {
    let mut ui = Ui::new().await;
    for apply in [true, false] {
        let mut app = drawing_app();
        let drawing = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        open_optimization(&mut ui, &mut app).await;
        assert_eq!(app.tab.doc, drawing);
        close(&mut ui, &mut app, apply).await;
        assert!(!app.error, "{}", app.status);
        assert!(app.tab.optimization.is_none(), "{}", app.status);
        if apply {
            assert_ne!(app.tab.doc, drawing);
            assert!(app.tab.history.can_undo());
        } else {
            assert_eq!(app.tab.doc, drawing);
        }
        assert_eq!(app.tab.selected, selected);
        assert_canvas_delete(&mut ui, &mut app).await;
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_preview_buttons() {
    let mut ui = Ui::new().await;
    for optimization in [false, true] {
        for apply in [true, false] {
            let mut app = drawing_app();
            let selected = app.tab.selected.clone();
            let drawing = app.tab.doc.clone();
            let (cancel, accept) = if optimization {
                open_optimization(&mut ui, &mut app).await;
                ("optimization.cancel", "optimization.apply")
            } else {
                open_cleanup(&mut ui, &mut app).await;
                ("cleanup.cancel", "cleanup.apply")
            };
            // Shift+Tab reaches Apply first; the 3D scope also holds the
            // panel's other controls before Cancel.
            for (modifiers, expected) in [
                (Modifiers::SHIFT, accept),
                (Modifiers::SHIFT, cancel),
                (Modifiers::empty(), accept),
                (Modifiers::SHIFT, cancel),
            ] {
                ui.key(&mut app, Key::Named(Named::Tab), Code::Tab, modifiers, None)
                    .await;
                let focused: Vec<_> = ui
                    .semantics(&app)
                    .nodes
                    .into_iter()
                    .filter(|node| node.focused)
                    .map(|node| node.id)
                    .collect();
                assert_eq!(focused, [expected]);
            }
            if apply {
                ui.key(
                    &mut app,
                    Key::Named(Named::Tab),
                    Code::Tab,
                    Modifiers::empty(),
                    None,
                )
                .await;
                close(&mut ui, &mut app, true).await;
                assert_ne!(app.tab.doc, drawing);
            } else {
                ui.key(
                    &mut app,
                    Key::Named(Named::Space),
                    Code::Space,
                    Modifiers::empty(),
                    Some(" "),
                )
                .await;
                // Buttons activate Space on release.
                let (_, messages) = ui.event(
                    &app,
                    Event::Keyboard(keyboard::Event::KeyReleased {
                        key: Key::Named(Named::Space),
                        modified_key: Key::Named(Named::Space),
                        physical_key: Physical::Code(Code::Space),
                        location: keyboard::Location::Standard,
                        modifiers: Modifiers::empty(),
                    }),
                    mouse::Cursor::Unavailable,
                );
                for message in messages {
                    let task = app.update(message);
                    ui.run_task(&mut app, task).await;
                }
                assert_eq!(app.tab.doc, drawing);
            }
            assert!(app.tab.cleanup.is_none() && app.tab.optimization.is_none());
            assert_eq!(app.tab.selected, selected);
            assert_canvas_delete(&mut ui, &mut app).await;
        }
    }
}

/// Applying from the preview bar also releases a field focused elsewhere.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_cleanup_apply_with_import_focused() {
    let mut ui = Ui::new().await;
    let mut app = drawing_app();
    let selected = app.tab.selected.clone();
    open_cleanup(&mut ui, &mut app).await;
    let preview = app.tab.cleanup.as_ref().unwrap().document.clone();
    open_import(&mut ui, &mut app).await;
    ui.key(
        &mut app,
        Key::Named(Named::Tab),
        Code::Tab,
        Modifiers::SHIFT,
        None,
    )
    .await;
    ui.key(
        &mut app,
        Key::Named(Named::Enter),
        Code::Enter,
        Modifiers::empty(),
        None,
    )
    .await;
    assert!(app.tab.cleanup.is_none());
    assert_eq!(app.tab.doc, preview);
    assert_eq!(app.tab.selected, selected);
    assert_canvas_delete(&mut ui, &mut app).await;
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_atom_label() {
    let mut ui = Ui::new().await;
    for apply in [true, false] {
        let mut app = drawing_app();
        app.tab.selected = vec![1];
        app.sync_keyboard_drawing();
        let drawing = app.tab.doc.clone();
        close(&mut ui, &mut app, true).await;
        assert!(app.tab.atom_text.is_some());
        assert!(ui.input(&app, "atom-text").1);
        ui.key(
            &mut app,
            Key::Character("a".into()),
            Code::KeyA,
            Modifiers::COMMAND,
            Some("a"),
        )
        .await;
        ui.key(
            &mut app,
            Key::Character("N".into()),
            Code::KeyN,
            Modifiers::SHIFT,
            Some("N"),
        )
        .await;
        close(&mut ui, &mut app, apply).await;
        assert!(app.tab.atom_text.is_none());
        assert!(!app.error, "{}", app.status);
        if apply {
            assert_eq!(app.tab.doc.atom(1).unwrap().element, "N");
        } else {
            assert_eq!(app.tab.doc, drawing);
        }
        assert_eq!(app.tab.selected, [1]);
        assert_canvas_delete(&mut ui, &mut app).await;
    }
}

async fn open_import(ui: &mut Ui, app: &mut App) {
    ui.key(
        app,
        Key::Character("i".into()),
        Code::KeyI,
        Modifiers::COMMAND,
        Some("i"),
    )
    .await;
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Import);
    assert!(ui.input(app, app.import_input_id()).1);
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_name_import() {
    let mut ui = Ui::new().await;
    for apply in [true, false] {
        let mut app = drawing_app();
        // Reopening Import restores its previously selected Name mode.
        app.imports.kind = crate::app::import::InputKind::Name;
        let drawing = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        open_import(&mut ui, &mut app).await;
        for (letter, code) in [
            ("e", Code::KeyE),
            ("t", Code::KeyT),
            ("h", Code::KeyH),
            ("a", Code::KeyA),
            ("n", Code::KeyN),
            ("o", Code::KeyO),
            ("l", Code::KeyL),
        ] {
            ui.key(
                &mut app,
                Key::Character(letter.into()),
                code,
                Modifiers::empty(),
                Some(letter),
            )
            .await;
        }
        click_running_tasks(&mut ui, &mut app, "import-name-preview").await;
        assert!(!app.error, "{}", app.status);
        assert!(!app.tab.naming.is_pending());
        assert!(
            ui.semantics(&app)
                .nodes
                .iter()
                .any(|node| node.id == "import-name-preview" && node.expanded == Some(true))
        );
        let bounds = ui.input(&app, "import-name").0;
        ui.click(&mut app, bounds.center());
        assert!(ui.input(&app, "import-name").1);
        close(&mut ui, &mut app, apply).await;
        if apply {
            assert_eq!(app.tab.doc.atoms.len(), drawing.atoms.len() + 3);
            assert_eq!(app.tab.selected.len(), 4, "inserted atoms and name caption");
            assert!(
                app.tab
                    .selected
                    .iter()
                    .all(|id| !drawing.all_ids().contains(id))
            );
        } else {
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.selected, selected);
        }
        assert_canvas_delete(&mut ui, &mut app).await;
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_tab_without_preview_keeps_drawing() {
    let mut ui = Ui::new().await;
    let mut app = drawing_app();
    let drawing = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    for modifiers in [Modifiers::empty(), Modifiers::SHIFT] {
        ui.key(&mut app, Key::Named(Named::Tab), Code::Tab, modifiers, None)
            .await;
        assert!(app.tab.cleanup.is_none() && app.tab.optimization.is_none());
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.tab.selected, selected);
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_structure_import_insert() {
    let mut ui = Ui::new().await;
    let mut app = drawing_app();
    let drawing = app.tab.doc.clone();
    open_import(&mut ui, &mut app).await;
    for (letter, code) in [("C", Code::KeyC), ("C", Code::KeyC), ("O", Code::KeyO)] {
        ui.key(
            &mut app,
            Key::Character(letter.into()),
            code,
            Modifiers::SHIFT,
            Some(letter),
        )
        .await;
    }
    assert_eq!(app.imports.input.text(), "CCO");
    ui.key(
        &mut app,
        Key::Named(Named::Enter),
        Code::Enter,
        Modifiers::COMMAND,
        None,
    )
    .await;
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.atoms.len(), drawing.atoms.len() + 3);
    assert_eq!(app.tab.selected.len(), 3);
    assert!(
        app.tab
            .selected
            .iter()
            .all(|id| !drawing.all_ids().contains(id))
    );
    ui.assert_unfocused(&app);
    ui.key(
        &mut app,
        Key::Character("z".into()),
        Code::KeyZ,
        Modifiers::COMMAND,
        Some("z"),
    )
    .await;
    assert_eq!(
        app.tab.doc, drawing,
        "Command+Z undoes the insertion without a click"
    );
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_structure_import_escape() {
    let mut ui = Ui::new().await;
    let mut app = drawing_app();
    let selected = app.tab.selected.clone();
    open_import(&mut ui, &mut app).await;
    ui.key(
        &mut app,
        Key::Character("C".into()),
        Code::KeyC,
        Modifiers::SHIFT,
        Some("C"),
    )
    .await;
    close(&mut ui, &mut app, false).await;
    assert_eq!(app.imports.input.text(), "C");
    assert_eq!(app.tab.selected, selected);
    assert_canvas_delete(&mut ui, &mut app).await;
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_failed_structure_insert_keeps_editor() {
    let mut ui = Ui::new().await;
    let mut app = drawing_app();
    let drawing = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    open_import(&mut ui, &mut app).await;
    ui.key(
        &mut app,
        Key::Character("(".into()),
        Code::Digit9,
        Modifiers::SHIFT,
        Some("("),
    )
    .await;
    ui.key(
        &mut app,
        Key::Named(Named::Enter),
        Code::Enter,
        Modifiers::COMMAND,
        None,
    )
    .await;
    assert!(app.error, "{}", app.status);
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.selected, selected);
    assert!(ui.input(&app, crate::app::import::INPUT).1);
    close(&mut ui, &mut app, false).await;
    assert_canvas_delete(&mut ui, &mut app).await;
}
