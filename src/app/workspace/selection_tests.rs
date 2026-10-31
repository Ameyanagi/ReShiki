use super::*;
use reshiki::document::{Annotation, Point};

#[test]
fn bonded_movement_membership_preserves_expanded_selection_and_tracks_edits() {
    let (mut app, _) = App::new();
    app.tool = Tool::Select;
    app.tab.doc = Default::default();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    let c = app.tab.doc.add_atom("O", Point::new(84., 0.));
    let isolated = app.tab.doc.add_atom("N", Point::new(150., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.add_bond(b, c, 1, "plain");
    app.tab.doc.contract(&[b, c], "Et", "Et").unwrap();

    // The visible abbreviation anchor selects both of its atoms. Selecting
    // only the external atom leaves a constrained bond across the boundary;
    // selecting it together with the anchor does not.
    for (selected, expected) in [
        (vec![], false),
        (vec![isolated], false),
        (vec![a], true),
        (vec![b], true),
        (vec![a, b], false),
        (vec![a, b, b, isolated, u64::MAX], false),
    ] {
        app.tab.selected = selected;
        assert_eq!(app.moving_bonded_selection(), expected);
    }

    // Compare every subset, including an object/nonexistent ID and duplicate
    // membership, with the original ordered selection query.
    for tool in [Tool::Select, Tool::Lasso, Tool::Atom] {
        app.tool = tool;
        for mask in 0..32 {
            app.tab.selected = [a, b, c, isolated, u64::MAX]
                .into_iter()
                .enumerate()
                .filter_map(|(i, id)| (mask & (1 << i) != 0).then_some(id))
                .collect();
            app.tab.selected.extend(app.tab.selected.clone());
            let expanded = app.tab.doc.expand_abbreviation_selection(&app.tab.selected);
            let expected = tool.selects()
                && app
                    .tab
                    .doc
                    .bonds
                    .iter()
                    .any(|bond| expanded.contains(&bond.a) != expanded.contains(&bond.b));
            assert_eq!(app.moving_bonded_selection(), expected, "{tool:?}, {mask}");
        }
    }

    // This is a per-query index: edits and abbreviation expansion are always
    // read from the current document without a revision/cache dependency.
    app.tool = Tool::Select;
    app.tab.selected = vec![b];
    assert!(app.moving_bonded_selection());
    app.tab.doc.bonds.retain(|bond| bond.a != a && bond.b != a);
    assert!(!app.moving_bonded_selection());
    app.tab.doc.abbreviations.clear();
    assert!(app.moving_bonded_selection());
}

#[test]
fn the_atoms_palette_offers_h_first_and_each_button_picks_its_element() {
    assert_eq!(PALETTE_ELEMENTS[..2], ["H", "C"]);
    assert_eq!(super::super::palettes::element_hint("H"), "H · h");
    let (mut app, _) = App::new();
    for symbol in PALETTE_ELEMENTS {
        assert!(symbol == "*" || reshiki::editing::ELEMENTS.contains(&symbol));
        let _ = app.update(Message::Element(symbol.into()));
        assert_eq!((app.tool, app.element.as_str()), (Tool::Atom, symbol));
    }
}

#[test]
fn short_rows_fold_commands_then_arrange_then_summary_then_name() {
    let commands = [120., 56.];
    let row = Fold {
        name: (80., 40.),
        options: 70.,
        summary: 50.,
        commands: &commands,
        arrange: Some((227., 80.)),
    };
    let steps = |width| {
        let fit = fit(width, &row);
        (fit.folded, fit.compact, fit.summary, fit.short)
    };
    // Fixed part, two commands with gaps, and the group after one gap.
    let full = 200. + 130. + 66. + 237.;
    assert_eq!(steps(full), (0, false, true, false));
    assert_eq!(steps(full - 1.), (1, false, true, false));
    assert_eq!(steps(542.), (2, false, true, false));
    assert_eq!(steps(476.), (2, true, true, false));
    assert_eq!(steps(330.), (2, true, true, false));
    assert_eq!(steps(329.), (2, true, false, false));
    assert_eq!(steps(279.), (2, true, false, true));
    assert_eq!(steps(100.), (2, true, false, true));
    assert_eq!(fit(329., &row).fixed, 150.);
    assert_eq!(fit(279., &row).fixed, 110.);
    // Rows without an arrange group, summary or short name.
    let plain = |commands: &[f32], width| {
        let fit = fit(
            width,
            &Fold {
                name: (80., 80.),
                options: 120.,
                summary: 0.,
                commands,
                arrange: None,
            },
        );
        (fit.folded, fit.compact, fit.short)
    };
    assert_eq!(plain(&[56.], 300.), (0, false, false));
    assert_eq!(plain(&[56.], 250.), (1, false, false));
    assert_eq!(plain(&[], 100.), (0, false, false));
}

#[test]
fn row_commands_leave_clipboard_to_menus_and_shortcuts() {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    let labels: Vec<_> = app.context_commands().iter().map(|c| c.label).collect();
    assert_eq!(
        labels,
        ["3D optimize…", "Mark [", "Connect ]", "Coordinate }"]
    );
    assert!(app.context_commands().iter().all(|command| !matches!(
        command.message,
        Message::Copy(_) | Message::CopyImage | Message::CopyAs(_) | Message::Paste
    )));
    let _ = app.update(Message::KeyboardDrawing(
        super::super::keyboard_drawing::Action::Leave,
    ));
    assert!(!app.keyboard_drawing_active());
    let labels: Vec<_> = app.context_commands().iter().map(|c| c.label).collect();
    assert_eq!(labels, ["3D optimize…", "Keyboard drawing"]);
    app.tab.selected = app.tab.doc.all_ids();
    let labels: Vec<_> = app.context_commands().iter().map(|c| c.label).collect();
    assert_eq!(
        labels,
        [
            "3D optimize…",
            "Keyboard drawing",
            "Join",
            "Merge atoms",
            "Move & attach…",
            "Coordinate in place…",
            "Group"
        ]
    );
    let _ = app.update(Message::Group);
    let labels: Vec<_> = app.context_commands().iter().map(|c| c.label).collect();
    assert_eq!(
        labels,
        [
            "3D optimize…",
            "Keyboard drawing",
            "Join",
            "Merge atoms",
            "Move & attach…",
            "Coordinate in place…",
            "Group",
            "Ungroup"
        ]
    );
    assert!(
        !app.context_commands()
            .iter()
            .find(|command| matches!(command.message, Message::Group))
            .expect("Group command remains in the row")
            .enabled
    );
}

#[test]
fn select_all_keeps_a_closed_caption_inspector_closed() {
    for open in [false, true] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.inspector_open = open;
        app.inspector_tab = InspectorTab::Import;
        app.viewport = iced::Size::new(1000. - app.inspector_width(), 600.);
        app.tab.camera.zoom = 2.;
        app.tab.camera.center = Point::new(42., -20.);
        app.tab.doc.annotations.push(Annotation {
            id: 1,
            position: Point::new(-120., 30.),
            text: "Conditions".into(),
            format: Default::default(),
        });
        let camera = app.tab.camera;
        let viewport = app.viewport;
        let _ = app.update(Message::SelectAll);
        assert_eq!(app.tab.selected, [1]);
        assert_eq!(app.tool, Tool::Select);
        assert_eq!(app.tab.caption_target, Some(1));
        assert_eq!(app.inspector_open, open);
        assert_eq!(
            app.inspector_tab,
            if open {
                InspectorTab::Properties
            } else {
                InspectorTab::Import
            }
        );
        if !open {
            assert_eq!(app.tab.camera.center, camera.center);
            assert_eq!(app.tab.camera.zoom, camera.zoom);
            assert_eq!(app.viewport, viewport);
        }
    }
}

#[test]
fn caption_context_menu_keeps_a_closed_inspector_closed() {
    for open in [false, true] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.inspector_open = open;
        app.inspector_tab = InspectorTab::Import;
        app.viewport = iced::Size::new(1000. - app.inspector_width(), 600.);
        app.tab.camera.zoom = 2.;
        app.tab.camera.center = Point::new(42., -20.);
        app.tab.doc.annotations.push(Annotation {
            id: 1,
            position: Point::new(-120., 30.),
            text: "Conditions".into(),
            format: Default::default(),
        });
        let camera = app.tab.camera;
        let viewport = app.viewport;
        let position = iced::Point::new(300., 200.);
        let _ = app.update(Message::Canvas(Edit::ContextMenu {
            position,
            selected: vec![1],
            hit: vec![1],
        }));
        let menu = app.context_menu.as_ref().expect("Caption context menu");
        assert_eq!(menu.position, position);
        assert_eq!(menu.hit, Some(vec![1]));
        assert_eq!(app.tab.selected, [1]);
        assert_eq!(app.tool, Tool::Select);
        assert_eq!(app.tab.caption_target, Some(1));
        assert_eq!(app.inspector_open, open);
        assert_eq!(
            app.inspector_tab,
            if open {
                InspectorTab::Properties
            } else {
                InspectorTab::Import
            }
        );
        if !open {
            assert_eq!(app.tab.camera.center, camera.center);
            assert_eq!(app.tab.camera.zoom, camera.zoom);
            assert_eq!(app.viewport, viewport);
        }
    }
}

#[test]
fn selection_revealing_properties_keeps_targets_at_the_same_screen_position() {
    for zoom in [0.5, 1., 2.] {
        for (open, tab) in [
            (false, InspectorTab::Properties),
            (true, InspectorTab::Labels),
            (true, InspectorTab::DrawingStyle),
            (true, InspectorTab::Properties),
            (true, InspectorTab::Templates),
        ] {
            let (mut app, _) = App::new();
            app.inspector_open = open;
            app.inspector_tab = tab;
            app.viewport = iced::Size::new(1000. - app.inspector_width(), 600.);
            app.tab.camera.zoom = zoom;
            app.tab.camera.center = Point::new(42., -20.);
            app.tab.fit_to_view = true;
            let at = Point::new(-120., 30.);
            app.tab.doc.annotations.push(Annotation {
                id: 1,
                position: at,
                text: "Conditions".into(),
                format: Default::default(),
            });
            let before = app.tab.doc.clone();
            let screen = app
                .tab
                .camera
                .screen(at, iced::Rectangle::with_size(app.viewport));
            let _ = app.update(Message::Canvas(Edit::Select(vec![1])));
            let size = app.viewport;
            let _ = app.update(Message::Viewport(size));
            assert_eq!(app.tab.camera.zoom, zoom, "Selection must not refit");
            assert_eq!(
                app.tab.camera.screen(at, iced::Rectangle::with_size(size)),
                screen,
                "Inspector {open:?}/{tab:?}, zoom {zoom}"
            );
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
            assert!(
                app.tab.fit_to_view,
                "A real resize should still refit later"
            );
            let _ = app.update(Message::Viewport(iced::Size::new(500., 400.)));
            assert_ne!(app.tab.camera.zoom, zoom, "A real resize still refits");
        }
    }
}

#[test]
fn selection_preserves_manual_pan_and_zoom_then_inspector_toggle_resizes_normally() {
    let (mut app, _) = App::new();
    app.inspector_open = false;
    app.viewport = iced::Size::new(1000., 600.);
    app.tab.doc.arrows.push(reshiki::document::Arrow::new(
        1,
        Point::new(-100., 0.),
        Point::new(100., 0.),
        Default::default(),
        Default::default(),
    ));
    app.edit(Edit::Pan(60., -20.));
    app.edit(Edit::Zoom(1.5, Point::new(-40., 10.)));
    let at = app.tab.doc.arrows[0].start;
    let before = app
        .tab
        .camera
        .screen(at, iced::Rectangle::with_size(app.viewport));
    app.edit(Edit::Select(vec![1]));
    assert_eq!(
        app.tab
            .camera
            .screen(at, iced::Rectangle::with_size(app.viewport)),
        before
    );
    let camera = app.tab.camera;
    let _ = app.update(Message::Viewport(app.viewport));
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    app.edit(Edit::Pan(10., 20.));
    assert_eq!(app.tab.camera.center, camera.center.offset(-10., -20.));
    let _ = app.update(Message::Fit);
    let zoom = app.tab.camera.zoom;
    let _ = app.update(Message::ToggleInspector);
    let _ = app.update(Message::Viewport(iced::Size::new(1000., 600.)));
    assert!(
        app.tab.camera.zoom > zoom,
        "Explicit inspector toggle still refits"
    );
}
