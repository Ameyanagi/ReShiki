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

/// Clean up applies at once as one Undo step.
async fn clean_up(ui: &mut Ui, app: &mut App) {
    let drawing = app.tab.doc.clone();
    ui.key(
        app,
        Key::Character("K".into()),
        Code::KeyK,
        Modifiers::COMMAND | Modifiers::SHIFT,
        Some("K"),
    )
    .await;
    assert!(!app.error, "{}", app.status);
    assert!(!app.tab.busy);
    assert_ne!(app.tab.doc, drawing, "{}", app.status);
    assert!(app.tab.history.can_undo());
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
        approximations: vec![],
    })
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_cleanup() {
    let mut ui = Ui::new().await;
    let mut app = drawing_app();
    let selected = app.tab.selected.clone();
    clean_up(&mut ui, &mut app).await;
    assert_eq!(app.tab.selected, selected);
    assert_canvas_delete(&mut ui, &mut app).await;
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
    {
        for apply in [true, false] {
            let mut app = drawing_app();
            let selected = app.tab.selected.clone();
            let drawing = app.tab.doc.clone();
            open_optimization(&mut ui, &mut app).await;
            let (cancel, accept) = ("optimization.cancel", "optimization.apply");
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
            assert!(app.tab.optimization.is_none());
            assert_eq!(app.tab.selected, selected);
            assert_canvas_delete(&mut ui, &mut app).await;
        }
    }
}

/// The toolbar's Clean up and its options menu also release a field focused
/// elsewhere.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_cleanup_toolbar_with_import_focused() {
    let mut ui = Ui::new().await;
    let mut app = drawing_app();
    let selected = app.tab.selected.clone();
    let drawing = app.tab.doc.clone();
    open_import(&mut ui, &mut app).await;
    click_running_tasks(&mut ui, &mut app, "header-cleanup-options").await;
    assert!(app.cleanup_menu);
    click_running_tasks(&mut ui, &mut app, "cleanup-free").await;
    assert!(!app.cleanup_menu);
    assert_eq!(
        app.cleanup_options.orientation(),
        reshiki::cleanup::Orientation::Free
    );
    ui.assert_unfocused(&app);
    open_import(&mut ui, &mut app).await;
    click_running_tasks(&mut ui, &mut app, "header-cleanup").await;
    assert!(!app.error, "{}", app.status);
    assert_ne!(app.tab.doc, drawing);
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
        assert!(app.tab.optimization.is_none());
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

/// Settings opens on Command+, with its search field focused and keeps every
/// key; Esc, Command+, and Done each return them to the drawing, also when
/// a field had focus before Settings opened.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn settings_close_returns_focus_to_the_drawing() {
    let mut ui = Ui::new().await;
    for closing in ["Esc", "Command+,", "Done"] {
        let mut app = drawing_app();
        let drawing = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        open_import(&mut ui, &mut app).await;
        let import = app.import_input_id();
        let comma = || Key::Character(",".into());
        ui.key(
            &mut app,
            comma(),
            Code::Comma,
            Modifiers::COMMAND,
            Some(","),
        )
        .await;
        assert!(app.settings.open, "{closing}");
        assert!(ui.input(&app, "settings-search").1, "search has focus");
        for (text, code) in [("g", Code::KeyG), ("u", Code::KeyU)] {
            ui.key(
                &mut app,
                Key::Character(text.into()),
                code,
                Modifiers::empty(),
                Some(text),
            )
            .await;
        }
        ui.key(
            &mut app,
            Key::Named(Named::Delete),
            Code::Delete,
            Modifiers::empty(),
            None,
        )
        .await;
        assert_eq!(ui.input_value(&app, "settings-search"), "gu");
        assert_eq!(app.tab.doc, drawing, "keys stay in Settings");
        match closing {
            "Esc" => {
                ui.key(
                    &mut app,
                    Key::Named(Named::Escape),
                    Code::Escape,
                    Modifiers::empty(),
                    None,
                )
                .await
            }
            "Command+," => {
                ui.key(
                    &mut app,
                    comma(),
                    Code::Comma,
                    Modifiers::COMMAND,
                    Some(","),
                )
                .await
            }
            _ => click_running_tasks(&mut ui, &mut app, "settings-done").await,
        }
        assert!(!app.settings.open, "{closing}");
        assert_eq!(app.tab.selected, selected);
        assert!(!ui.input(&app, import).1, "{closing}");
        assert_canvas_delete(&mut ui, &mut app).await;
    }
}

/// A floating NMR panel stays under Settings: its controls leave keyboard
/// traversal and a click on it reaches Settings, not the panel.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn settings_covers_a_floating_nmr_panel() {
    let mut ui = Ui::new().await;
    let mut app = drawing_app();
    let _ = app.update(Message::Nmr(crate::app::nmr::Action::Open));
    let labels = ui
        .semantics(&app)
        .nodes
        .into_iter()
        .find(|node| node.id == "nmr.labels")
        .expect("the floating NMR panel");
    let _ = app.update(Message::Settings(crate::app::settings_page::Action::Open(
        None,
    )));
    assert!(
        ui.semantics(&app)
            .nodes
            .iter()
            .all(|node| !node.id.starts_with("nmr.")),
        "NMR controls leave traversal"
    );
    let point = labels.bounds.center();
    for event in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        let (_, messages) = ui.event(&app, Event::Mouse(event), mouse::Cursor::Available(point));
        assert!(
            messages.iter().all(|m| !matches!(m, Message::Nmr(_))),
            "{messages:?}"
        );
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_check_findings() {
    use reshiki::chemistry::stereo_lint::{Kind, Note};
    let mut ui = Ui::new().await;
    let mut app = drawing_app();
    let first = app.tab.doc.atoms[0].id;
    app.tab.stereo_notes = vec![Note {
        kind: Kind::Undefined,
        atoms: vec![first],
        bonds: vec![],
        message: "1 undefined stereocenter (C1)".into(),
    }];
    click_running_tasks(&mut ui, &mut app, "header-check").await;
    assert!(app.check_results, "The counted button opens the findings");
    click_running_tasks(&mut ui, &mut app, "check-finding-0").await;
    assert!(app.check_results, "Choosing a finding keeps the list open");
    assert_eq!(app.tab.selected, vec![first]);
    close(&mut ui, &mut app, false).await;
    assert!(!app.check_results);
    assert_eq!(app.tab.selected, vec![first]);
    assert_canvas_delete(&mut ui, &mut app).await;
}

/// A list taller than the window scrolls under its header, and its fixes
/// stay in reach at the minimum window size.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn check_findings_scroll_to_the_last_one_at_the_minimum_window_size() {
    use reshiki::chemistry::stereo_lint::{Kind, Note};
    let mut ui = Ui::new().await;
    ui.viewport = Rectangle::with_size(Size::new(1040., 680.));
    let mut app = drawing_app();
    let [first, .., last] = app
        .tab
        .doc
        .atoms
        .iter()
        .map(|atom| atom.id)
        .collect::<Vec<_>>()[..]
    else {
        unreachable!()
    };
    app.tab.stereo_notes = (0..20)
        .map(|index| Note {
            kind: Kind::Reversed,
            atoms: vec![if index == 19 { last } else { first }],
            bonds: vec![],
            message: format!("Wedge {index} points the wrong way; its narrow end belongs at C1"),
        })
        .collect();
    click_running_tasks(&mut ui, &mut app, "header-check").await;
    assert!(app.check_results);
    let node = |ui: &mut Ui, app: &App, id: &str| {
        ui.semantics(app)
            .nodes
            .into_iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("{id}"))
    };
    // Wholly visible, allowing for rounding in the clip.
    let shown = |node: &reshiki::accessibility::Node| {
        node.visible_bounds.is_some_and(|visible| {
            (visible.width - node.bounds.width).abs() < 0.5
                && (visible.height - node.bounds.height).abs() < 0.5
        })
    };
    for fix in ["check-reverse", "check-redraw", "check-again"] {
        assert!(shown(&node(&mut ui, &app, fix)), "{fix} is in reach");
    }
    assert!(!shown(&node(&mut ui, &app, "check-finding-19")));
    let over = node(&mut ui, &app, "check-finding-0").bounds.center();
    for _ in 0..40 {
        if shown(&node(&mut ui, &app, "check-finding-19")) {
            break;
        }
        let wheel = mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0., y: -1. },
        };
        let _ = ui.event(&app, Event::Mouse(wheel), mouse::Cursor::Available(over));
    }
    assert!(shown(&node(&mut ui, &app, "check-finding-19")));
    click_running_tasks(&mut ui, &mut app, "check-finding-19").await;
    assert!(app.check_results);
    assert_eq!(app.tab.selected, vec![last]);
    let redraw = node(&mut ui, &app, "check-redraw").bounds.center();
    let mut sent = vec![];
    for event in [
        mouse::Event::CursorMoved { position: redraw },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        let (_, messages) = ui.event(&app, Event::Mouse(event), mouse::Cursor::Available(redraw));
        sent.extend(messages);
    }
    assert!(
        sent.iter()
            .any(|message| matches!(message, Message::Stereo(crate::app::stereo::Action::Redraw))),
        "{sent:?}"
    );
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn review_close_returns_focus_reaction_preview() {
    let mut ui = Ui::new().await;
    let mut app = drawing_app();
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(reshiki::document::Arrow::new(
        arrow,
        Point::new(120., 0.),
        Point::new(220., 0.),
        Default::default(),
        Default::default(),
    ));
    let product = app.tab.doc.add_atom("C", Point::new(300., 0.));
    let other = app.tab.doc.add_atom("O", Point::new(340., 0.));
    app.tab.doc.add_bond(product, other, 1, "plain");
    app.tab.selected = vec![arrow];
    let task = app.update(Message::Reaction(crate::app::reactions::Action::Preview));
    ui.run_task(&mut app, task).await;
    assert!(app.reaction_preview().is_some(), "{}", app.status);
    close(&mut ui, &mut app, false).await;
    assert!(app.reaction_preview().is_none());
    assert_eq!(app.tab.selected, vec![arrow]);
    assert_canvas_delete(&mut ui, &mut app).await;
}
