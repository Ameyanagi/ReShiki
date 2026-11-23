use super::*;
use crate::canvas::Edit;
use reshiki::equipment::builtin;
use reshiki::equipment::solve::hose::Grip;
use reshiki::graphics::GraphicKind;

/// The key that arms a built-in piece.
fn k(id: &str) -> String {
    placed::key(&builtin::find(id).unwrap())
}

#[test]
fn tube_furnace_pointer_drags_carry_the_casing_chain_but_slide_the_bare_tube() {
    use reshiki::equipment::eval::Value;
    let source = Document::from_json(include_bytes!(
        "../../../tests/fixtures/tube-furnace/connected-exterior.rsk"
    ))
    .unwrap();
    assert_eq!((source.graphics.len(), source.seats.len()), (8, 7));
    for view in ["exterior", "section"] {
        for (name, clicked, local) in [
            ("casing", 1, Point::new(100., -36.)),
            ("base", 1, Point::new(100., 88.)),
            ("bare tube", 2, Point::new(220., 0.)),
        ] {
            let (mut app, _) = App::new();
            app.tab.busy = false;
            app.tool = Tool::Select;
            app.tab.doc = source.clone();
            app.tab.selected.clear();
            let all: Vec<_> = source.graphics.iter().map(|g| g.id).collect();
            // Keep each actual hit and its release inside the real pointer
            // helper's viewport; the connected fixture itself stays intact.
            app.tab.doc.translate(&all, -120., 0.);
            for id in &all {
                solve::adjust::set(&mut app.tab.doc, *id, "view", Value::State(view.into()))
                    .unwrap();
            }
            let before = app.tab.doc.clone();
            let graphic = before.graphics.iter().find(|g| g.id == clicked).unwrap();
            let from = graphic.origin.offset(
                graphic.axis_x.x * local.x + graphic.axis_y.x * local.y,
                graphic.axis_x.y * local.x + graphic.axis_y.y * local.y,
            );
            let to = from.offset(24., -18.);
            // This sends CursorMoved, press, motion and release through the
            // production canvas input path, with no preselected assembly.
            let edits = crate::canvas::select_drag(
                &before,
                &[],
                from,
                to,
                iced::keyboard::Modifiers::empty(),
            );
            let (moving, dx, dy) = edits
                .iter()
                .find_map(|edit| match edit {
                    Edit::Move(ids, dx, dy) => Some((ids, *dx, *dy)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{name} in {view} must publish a move: {edits:?}"));
            let expected: Vec<_> = all
                .iter()
                .copied()
                .filter(|id| clicked == 1 || *id != 1)
                .collect();
            assert_eq!(moving.len(), expected.len(), "{name} in {view}");
            assert!(expected.iter().all(|id| moving.contains(id)));
            let expected_dy = if clicked == 1 { -18. } else { 0. };
            assert!((dx - 24.).abs() < 1e-3 && (dy - expected_dy).abs() < 1e-3);
            for edit in edits {
                let _ = app.update(Message::Canvas(edit));
            }
            assert!(!app.error, "{name} in {view}: {}", app.status);
            assert_eq!(app.tab.doc.seats, before.seats);
            for old in &before.graphics {
                let after = app
                    .tab
                    .doc
                    .graphics
                    .iter()
                    .find(|g| g.id == old.id)
                    .unwrap();
                let target = if expected.contains(&old.id) {
                    old.origin.offset(24., expected_dy)
                } else {
                    old.origin
                };
                assert!(
                    after.origin.distance(target) < 1e-3,
                    "{name} in {view}: {}",
                    old.id
                );
                assert_eq!((after.axis_x, after.axis_y), (old.axis_x, old.axis_y));
            }
            let moved = app.tab.doc.clone();
            let reopened = Document::from_json(&moved.file_json().unwrap()).unwrap();
            assert_eq!(reopened.seats, before.seats);
            assert_eq!(reopened.graphics.len(), 8);
            assert_eq!(app.tab.history.undo_frames().len(), 1);
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, moved);
        }
    }
}

#[test]
fn either_tube_end_pointer_drag_matches_preview_and_undo_with_a_layout_group() {
    use reshiki::equipment::{eval::Value, solve::adjust};
    for end in ["a", "b"] {
        for (id, layout) in [1, 2]
            .into_iter()
            .flat_map(|id| (0..3).map(move |layout| (id, layout)))
        {
            let (mut app, _) = App::new();
            app.tab.busy = false;
            app.tool = Tool::Select;
            let tube = builtin::find("reshiki/compression-tube-60-1-4").unwrap();
            let mm = placed::world_per_mm(&app.tab.doc, placed::DEFAULT_PT_PER_MM);
            app.tab
                .doc
                .graphics
                .push(placed::graphic(1, tube.clone(), Point::default(), mm));
            app.tab
                .doc
                .graphics
                .push(placed::graphic(2, tube.clone(), Point::new(100., 0.), mm));
            app.tab
                .doc
                .graphics
                .push(placed::graphic(3, tube, Point::new(200., 0.), mm));
            if layout > 0 {
                reshiki::layouts::arrange(
                    &mut app.tab.doc,
                    &[1, 2],
                    reshiki::layouts::Flow::Row,
                    12.,
                )
                .unwrap();
                if layout == 2 {
                    reshiki::layouts::arrange(
                        &mut app.tab.doc,
                        &[1, 2, 3],
                        reshiki::layouts::Flow::Row,
                        12.,
                    )
                    .unwrap();
                }
            } else {
                app.tab.doc.group_selection(&[1, 2]).unwrap();
            }
            app.tab.doc.piece_definitions = placed::used(&app.tab.doc);
            app.tab.selected = vec![id];
            let before = app.tab.doc.clone();
            let endpoint = |doc: &Document, end: &str| {
                solve::ports(doc.graphics.iter().find(|g| g.id == id).unwrap())
                    .into_iter()
                    .find(|p| p.at.joint == end)
                    .unwrap()
                    .point
            };
            let opposite = if end == "a" { "b" } else { "a" };
            let pinned = endpoint(&before, opposite);
            let g = before.graphics.iter().find(|g| g.id == id).unwrap();
            let handle = adjust::handles(g)
                .into_iter()
                .find(|h| h.tube_end.as_deref() == Some(end))
                .unwrap();
            let direction = solve::ports(g)
                .into_iter()
                .find(|p| p.at.joint == end)
                .unwrap()
                .dir;
            let from = Point::new(handle.at[0] as f32, handle.at[1] as f32);
            let to = from.offset(
                24. * mm * direction[0] as f32,
                24. * mm * direction[1] as f32,
            );
            let preview =
                crate::canvas::MoleculeCanvas::piece_handle_drag_candidate(&before, id, handle, to);
            let edits = crate::canvas::select_drag(
                &before,
                &[id],
                from,
                to,
                iced::keyboard::Modifiers::empty(),
            );
            assert!(edits.iter().any(|edit| matches!(edit, Edit::TubeEnd { id: actual, end: e, value: Value::Number(n), .. } if *actual == id && e == end && *n == 84.)), "{edits:?}");
            for edit in edits {
                let _ = app.update(Message::Canvas(edit));
            }
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.tab.doc, preview, "{end} with layout={layout}");
            let fixed = endpoint(&app.tab.doc, opposite);
            assert!(
                (fixed[0] - pinned[0]).hypot(fixed[1] - pinned[1]) < 1e-4,
                "piece {id} {end} in layout {layout}"
            );
            let moved = endpoint(&app.tab.doc, end);
            assert!((moved[0] - f64::from(to.x)).hypot(moved[1] - f64::from(to.y)) < 1e-4);
            assert_eq!(app.tab.doc.groups, before.groups);
            assert_eq!(app.tab.history.undo_frames().len(), 1);
            let committed = app.tab.doc.clone();
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, committed);
            // The Properties field still uses its original pivot and exact
            // numeric value, including in a non-first layout member.
            let _ = app.update(Message::Undo);
            let property_anchor = endpoint(&app.tab.doc, "a");
            let _ = app.update(Message::Equipment(Action::Param {
                id,
                param: "length".into(),
                value: Value::Number(84.25),
            }));
            assert!(!app.error, "{}", app.status);
            let fixed = endpoint(&app.tab.doc, "a");
            assert!((fixed[0] - property_anchor[0]).hypot(fixed[1] - property_anchor[1]) < 1e-4);
            assert_eq!(
                reshiki::equipment::parts_list::tube_row(&app.tab.doc, id)
                    .unwrap()
                    .unwrap()
                    .cut_length_mm,
                84.25
            );
        }
    }
}

#[test]
fn search_finds_pieces_by_any_name_keyword_or_size() {
    let found = |query: &str| -> Vec<&str> {
        (builtin::catalogue().iter())
            .filter(|d| matches(d, query))
            .map(|d| d.id.as_str())
            .collect()
    };
    for query in ["丸底", "round", "ナス", "Round-bottom 100"] {
        assert!(found(query).contains(&"reshiki/rbf-100-15"), "{query}");
    }
    assert_eq!(found("liebig"), ["reshiki/liebig-15", "reshiki/liebig-29"]);
    assert_eq!(found("septum 29/42"), ["reshiki/septum-29"]);
    assert_eq!(found("septum 15/25 hose"), ["reshiki/septum-hose-7"]);
    assert_eq!(
        found("straight schlenk tube"),
        [
            "reshiki/schlenk-tube-straight-15",
            "reshiki/schlenk-tube-straight-15-left",
            "reshiki/schlenk-tube-straight-29",
            "reshiki/schlenk-tube-straight-29-left",
        ]
    );
    assert!(found("burette").is_empty());
    assert_eq!(found("").len(), builtin::catalogue().len());
}

#[test]
fn names_sizes_and_the_click_hint_come_from_the_definition() {
    let flask = builtin::find("reshiki/rbf-100-15").unwrap();
    assert_eq!(title(&flask), "Round-bottom flask");
    assert_eq!(size(&flask), "100 mL · 15/25");
    assert_eq!(noun(&flask), "flask");
    let adapter = builtin::find("reshiki/red-29-15").unwrap();
    assert_eq!(size(&adapter), "29/42 · 15/25");
    assert_eq!(noun(&adapter), "adapter");
    let pipette = builtin::find("reshiki/graduated-pipette-tip-5").unwrap();
    assert_eq!(title(&pipette), "Graduated pipette");
    assert_eq!(noun(&pipette), "pipette");
    assert_eq!(size(&pipette), "5 mL · Tip graduated");
}

#[test]
fn arming_and_placing_a_piece_adds_one_graphic_in_one_undo_step() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Tool(Tool::Equipment));
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Equipment);
    assert!(!app.equipment.repeat, "Keep placing is opt-in");
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/rbf-100-15"))));
    assert_eq!(app.tool, Tool::Equipment);
    assert_eq!(app.tab.doc, before, "arming changes nothing");
    let _ = app.update(Message::ToggleInspector);
    assert!(!app.inspector_open);
    let at = Point::new(120., 80.);
    let _ = app.update(Message::Canvas(Edit::Piece(at)));
    assert!(!app.error, "{}", app.status);
    let [piece] = app.tab.doc.graphics.as_slice() else {
        panic!("one graphic: {:?}", app.tab.doc.graphics.len());
    };
    assert_eq!(piece.kind, GraphicKind::Piece);
    assert_eq!(app.tab.selected, [piece.id]);
    let (lo, hi) = piece.shape_bounds();
    let mid = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    assert!(mid.distance(at) < 0.01, "{mid:?}");
    assert_eq!(app.tab.doc.piece_definitions.len(), 1);
    assert_eq!(app.tool, Tool::Select);
    assert!(app.inspector_open, "the placed piece is ready to adjust");
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    assert!(app.armed_piece().is_none(), "the placement preview is gone");
    let placed = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Piece(at.offset(100., 0.))));
    assert_eq!(app.tab.doc, placed, "another click cannot place a copy");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, placed);
    assert_eq!(app.tool, Tool::Select);
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/rbf-100-15"))));
    let _ = app.update(Message::Canvas(Edit::Piece(at.offset(300., 0.))));
    assert_eq!(app.tab.doc.graphics.len(), 2, "arming again adds one copy");
    assert_eq!(app.tool, Tool::Select);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, placed);
}

#[test]
fn hanging_a_hose_and_bending_it_are_separate_undo_steps() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/liebig-29"))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(200., 300.))));
    let before = app.tab.doc.clone();
    let water_in = solve::ports(&before.graphics[0])
        .into_iter()
        .find(|p| p.at.joint == "water-in")
        .unwrap();
    let p = Point::new(water_in.point[0] as f32, water_in.point[1] as f32);

    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/hose-7"))));
    let _ = app.update(Message::ToggleInspector);
    assert!(!app.inspector_open);
    let _ = app.update(Message::Canvas(Edit::Hose(p, p)));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.graphics.len(), before.graphics.len() + 1);
    assert_eq!(app.tab.doc.seats.len(), before.seats.len() + 1);
    assert_eq!(app.status, "Hose on the condenser water in");
    assert_eq!(app.tool, Tool::Select);
    assert!(app.inspector_open, "the placed hose is ready to adjust");
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    let hose = app
        .tab
        .doc
        .graphics
        .iter()
        .find(|g| g.id != water_in.at.piece)
        .unwrap();
    let id = hose.id;
    assert_eq!(app.tab.selected, solve::closure(&app.tab.doc, &[id]));
    let line = hose.piece.as_ref().unwrap().centre_line.clone();
    let hung = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Hose(p, p.offset(100., 100.))));
    assert_eq!(app.tab.doc, hung, "a second gesture cannot place a hose");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, hung);

    app.set_selection(vec![id]);
    let _ = app.update(Message::Canvas(Edit::HoseGrip {
        id,
        grip: Grip::Middle,
        to: p.offset(50., 60.),
    }));
    assert!(!app.error, "{}", app.status);
    let bent = app.tab.doc.graphics.iter().find(|g| g.id == id).unwrap();
    assert_ne!(bent.piece.as_ref().unwrap().centre_line, line);
    assert_eq!(app.tab.doc.seats, hung.seats);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, hung);
}

#[test]
fn opening_the_schlenk_tube_valve_is_one_undo_step() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Equipment(Action::Arm(k(
        "reshiki/jy-schlenk-tube",
    ))));
    let camera = app.tab.camera;
    let viewport = app.viewport;
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(120., 80.))));
    let id = app.tab.doc.graphics[0].id;
    assert_eq!(app.tool, Tool::Select);
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    assert_eq!(app.tab.selected, [id]);
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(
        app.viewport, viewport,
        "an open panel needs no compensation"
    );

    let open = Value::State("open".into());
    let _ = app.update(Message::Equipment(Action::Param {
        id,
        param: "valve".into(),
        value: open.clone(),
    }));
    let tube = &app.tab.doc.graphics[0];
    assert_eq!(
        tube.piece.as_ref().unwrap().params.get("valve"),
        Some(&open)
    );
    assert_eq!(value_text(tube, "valve", None), "Valve: Open");

    let _ = app.update(Message::Undo);
    let tube = &app.tab.doc.graphics[0];
    assert_eq!(tube.id, id);
    assert!(!tube.piece.as_ref().unwrap().params.contains_key("valve"));
    assert_eq!(value_text(tube, "valve", None), "Valve: Closed");
}

#[test]
fn a_straight_schlenk_sidearm_uses_the_s_type_flow_controls_and_undoes() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Equipment(Action::Arm(k(
        "reshiki/schlenk-tube-straight-15-left",
    ))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(120., 80.))));
    let tube = &app.tab.doc.graphics[0];
    let id = tube.id;
    let before = app.tab.doc.clone();
    assert_eq!(value_text(tube, "stopcock", None), "Stopcock: Closed");
    let _ = app.update(Message::Equipment(Action::Param {
        id,
        param: "stopcock".into(),
        value: Value::State("side".into()),
    }));
    assert_eq!(
        value_text(&app.tab.doc.graphics[0], "stopcock", None),
        "Stopcock: Arm to side barb"
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn escape_disarms_and_a_click_then_places_nothing() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/liebig-15"))));
    assert_eq!(app.tool, Tool::Equipment);
    let _ = app.update(Message::Escape);
    assert_eq!(app.tool, Tool::Select);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Piece(Point::default())));
    assert_eq!(app.tab.doc, before);
}

#[test]
fn keep_placing_explicitly_repeats_pieces_and_hoses_until_switched_off() {
    for (piece, hose) in [("reshiki/stopper-15", false), ("reshiki/hose-7", true)] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let _ = app.update(Message::Tool(Tool::Equipment));
        let _ = app.update(Message::Equipment(Action::Repeat(true)));
        let _ = app.update(Message::Equipment(Action::Arm(k(piece))));
        let _ = app.update(Message::ToggleInspector);
        assert!(!app.inspector_open);
        let edit = |x| {
            let at = Point::new(x, 0.);
            Message::Canvas(if hose {
                Edit::Hose(at, at.offset(80., 40.))
            } else {
                Edit::Piece(at)
            })
        };
        for count in 1..=2 {
            let _ = app.update(edit(count as f32 * 500.));
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.tab.doc.graphics.len(), count);
            assert_eq!(app.tool, Tool::Equipment);
            assert!(app.armed_piece().is_some());
            assert!(!app.inspector_open, "Keep placing leaves the panel closed");
            assert_eq!(app.inspector_tab, InspectorTab::Equipment);
        }
        let _ = app.update(Message::Equipment(Action::Repeat(false)));
        let _ = app.update(edit(1500.));
        assert_eq!(app.tab.doc.graphics.len(), 3);
        assert_eq!(app.tool, Tool::Select);
        assert!(app.inspector_open);
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
        let placed = app.tab.doc.clone();
        let _ = app.update(edit(2000.));
        assert_eq!(app.tab.doc, placed);
    }
}

#[test]
fn rejected_placement_stays_armed_and_can_be_retried() {
    for (piece, hose) in [("reshiki/stopper-15", false), ("reshiki/hose-7", true)] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let _ = app.update(Message::Tool(Tool::Equipment));
        let _ = app.update(Message::Equipment(Action::Arm(k(piece))));
        let _ = app.update(Message::ToggleInspector);
        assert!(!app.inspector_open);
        let edit = |at| {
            Message::Canvas(if hose {
                Edit::Hose(at, Point::new(80., 40.))
            } else {
                Edit::Piece(at)
            })
        };
        let before = app.tab.doc.clone();
        let _ = app.update(edit(Point::new(f32::NAN, 0.)));
        assert!(app.error, "an invalid placement is rejected");
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        assert_eq!(app.tool, Tool::Equipment);
        assert!(
            !app.inspector_open,
            "a rejected placement keeps the panel closed"
        );
        assert_eq!(app.inspector_tab, InspectorTab::Equipment);
        assert!(app.armed_piece().is_some());
        let _ = app.update(edit(Point::default()));
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.graphics.len(), 1);
        assert_eq!(app.tool, Tool::Select);
        assert!(app.inspector_open);
    }
}

#[test]
fn reopening_properties_after_placement_preserves_visible_anchor_zoom_and_fit() {
    for hose in [false, true] {
        for fit in [false, true] {
            for (zoom, rulers) in [(0.5, false), (1., true), (2., false)] {
                for covered in [false, true] {
                    let (mut app, _) = App::new();
                    app.tab.busy = false;
                    let inspector_width = app.inspector_width();
                    let piece = if hose {
                        "reshiki/hose-7"
                    } else {
                        "reshiki/rbf-100-15"
                    };
                    let _ = app.update(Message::Equipment(Action::Arm(k(piece))));
                    let _ = app.update(Message::ToggleInspector);
                    app.viewport = iced::Size::new(1000., 600.);
                    app.tab.camera.center = Point::new(42., -20.);
                    app.tab.camera.zoom = zoom;
                    app.tab.fit_to_view = fit;
                    app.guides.rulers = rulers;
                    let before = app.guides.paper(iced::Rectangle::with_size(app.viewport));
                    // Use the physical canvas width after opening the panel,
                    // independently of the viewport the handler records.
                    let viewport = iced::Size::new(1000. - inspector_width, 600.);
                    let after = app.guides.paper(iced::Rectangle::with_size(viewport));
                    let x = if covered {
                        before.width - 20.
                    } else {
                        after.width / 2.
                    };
                    let at = app
                        .tab
                        .camera
                        .center
                        .offset((x - before.width / 2.) / zoom, 40.);
                    let screen_before = app.tab.camera.screen(at, before);
                    let edit = if hose {
                        Edit::Hose(at.offset(-80. / zoom, -40. / zoom), at)
                    } else {
                        Edit::Piece(at)
                    };
                    let _ = app.update(Message::Canvas(edit));
                    assert!(!app.error, "{}", app.status);
                    assert_eq!(app.tool, Tool::Select);
                    assert!(app.inspector_open);
                    assert_eq!(app.inspector_tab, InspectorTab::Properties);
                    assert_eq!(app.viewport, viewport);
                    assert_eq!(app.tab.camera.zoom, zoom);
                    assert_eq!(app.tab.fit_to_view, fit);
                    let [piece] = app.tab.doc.graphics.as_slice() else {
                        panic!("one placed piece");
                    };
                    let anchor = if hose {
                        let (_, end) = solve::hose::grips(piece)
                            .into_iter()
                            .find(|(grip, _)| *grip == Grip::End(1))
                            .unwrap();
                        Point::new(end[0] as f32, end[1] as f32)
                    } else {
                        placed::middle(piece)
                    };
                    let expected = iced::Point::new(
                        if covered {
                            after.width - 16.
                        } else {
                            screen_before.x
                        },
                        screen_before.y,
                    );
                    assert!(
                        app.tab.camera.screen(anchor, after).distance(expected) < 0.01,
                        "hose={hose}, fit={fit}, zoom={zoom}, rulers={rulers}, covered={covered}"
                    );
                    let camera = app.tab.camera;
                    let _ = app.update(Message::Viewport(viewport));
                    assert_eq!(app.tab.camera.center, camera.center);
                    assert_eq!(app.tab.camera.zoom, zoom, "the panel resize must not Fit");
                    assert_eq!(app.tab.fit_to_view, fit);
                    let _ = app.update(Message::Viewport(iced::Size::new(500., 400.)));
                    if fit {
                        assert_ne!(app.tab.camera.zoom, zoom, "a real resize still Fits");
                    } else {
                        assert_eq!(app.tab.camera.center, camera.center);
                        assert_eq!(app.tab.camera.zoom, zoom);
                    }
                }
            }
        }
    }
}

/// A flask placed at `at`, and where a piece's joint seats on its neck.
fn flask_neck(app: &mut App, at: Point) -> Point {
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/rbf-100-15"))));
    let _ = app.update(Message::Canvas(Edit::Piece(at)));
    let neck = solve::ports(&app.tab.doc.graphics[0]).remove(0);
    Point::new(neck.point[0] as f32, neck.point[1] as f32)
}

#[test]
fn a_stopper_placed_by_the_neck_seats_and_undo_takes_it_off_in_one_step() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let neck = flask_neck(&mut app, Point::new(200., 300.));
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/stopper-15"))));
    let before = app.tab.doc.clone();
    // A click centres the stopper: aim its shoulder a few points off the neck.
    let scale = placed::world_per_mm(&app.tab.doc, placed::DEFAULT_PT_PER_MM);
    let probe = placed::graphic(
        0,
        builtin::find("reshiki/stopper-15").unwrap(),
        Point::default(),
        scale,
    );
    let shoulder = solve::ports(&probe)[0].point;
    let click = Point::new(
        neck.x - shoulder[0] as f32 + 3.,
        neck.y - shoulder[1] as f32 - 4.,
    );
    let _ = app.update(Message::Canvas(Edit::Piece(click)));
    assert_eq!(app.tab.doc.seats.len(), 1, "{}", app.status);
    assert_eq!(app.status, "Seated the stopper bottom on the flask neck");
    let stopper = app.tab.doc.graphics[1].id;
    assert_eq!(app.tab.selected, solve::closure(&app.tab.doc, &[stopper]));
    assert_eq!(
        app.tab.selected.len(),
        2,
        "the seated assembly stays selected"
    );
    assert_eq!(app.tool, Tool::Select);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn keyboard_connect_offers_joints_in_turn_and_return_seats_the_shown_one() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    flask_neck(&mut app, Point::new(200., 300.));
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/rbf-100-29"))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(600., 300.))));
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/stopper-15"))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(260., 100.))));
    let stopper = app.tab.doc.graphics[2].id;
    let _ = app.update(Message::Tool(Tool::Select));
    app.set_selection(vec![stopper]);
    let _ = app.update(Message::ContextKey("j".into()));
    assert!(
        app.status.starts_with("Connect to the flask neck (1 of 2)"),
        "{}",
        app.status
    );
    let _ = app.update(Message::Shortcut(crate::app::shortcuts::Action::Nudge(
        0., 1.,
    )));
    assert!(app.status.contains("(2 of 2)"), "{}", app.status);
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(
        app.status.starts_with("Inserted the reducing adapter"),
        "{}",
        app.status
    );
    assert_eq!(app.tab.doc.seats.len(), 2);
    assert!(app.tab.connect.is_none());
    // Detach from Properties pulls the adapter and stopper off the neck.
    let neck = app.tab.doc.seats[0].a.clone();
    let _ = app.update(Message::Equipment(Action::Detach(neck)));
    assert_eq!(app.tab.doc.seats.len(), 1);
}

/// A lab's copy of the 100 mL flask, at `revision`, with an extra keyword so
/// two copies can differ under one ID.
fn lab_flask(revision: u32, keyword: &str) -> Arc<Definition> {
    let mut d = (*builtin::find("reshiki/rbf-100-15").unwrap()).clone();
    d.id = "lab/flask".into();
    d.revision = revision;
    d.keywords.push(keyword.into());
    Arc::new(d)
}

#[test]
fn library_pieces_join_the_browser_once_each_and_name_their_library_on_a_clash() {
    let (mut app, _) = App::new();
    let mine = lab_flask(1, "mine");
    let theirs = lab_flask(1, "theirs");
    let libraries = &mut app.equipment.libraries;
    libraries.mine.pieces = vec![mine.clone(), mine.clone()];
    libraries.labs = vec![(
        PathBuf::from("/labs/Tanaka lab"),
        reshiki::piece_library::Folder {
            pieces: vec![theirs.clone(), mine.clone()],
            problems: vec![],
        },
    )];
    libraries.index();
    let entries = &libraries.entries;
    let lab: Vec<_> = (entries.iter())
        .filter(|e| e.definition.id == "lab/flask")
        .map(|e| e.source)
        .collect();
    assert_eq!(lab, [Source::Mine, Source::Lab(0)]);
    // The lab's own list still shows the piece My pieces also holds.
    let in_lab = (entries.iter())
        .filter(|e| e.definition.id == "lab/flask" && e.in_library(Source::Lab(0)))
        .count();
    assert_eq!(in_lab, 2);
    assert_eq!(libraries.name(Source::Lab(0)), "Tanaka lab");
    assert_eq!(libraries.newer(&lab_flask(0, "old")).unwrap().0, mine);
    assert!(libraries.newer(&mine).is_none());
    // Arming by key places that exact piece.
    app.tab.busy = false;
    let _ = app.update(Message::Equipment(Action::Arm(placed::key(&theirs))));
    assert_eq!(app.equipment.armed.as_deref(), Some(&*theirs));
}

#[test]
fn updating_to_a_newer_revision_is_one_undo_step_and_keeps_the_seat() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let old = lab_flask(1, "old");
    let new = lab_flask(2, "new");
    app.equipment.libraries.mine.pieces = vec![old.clone(), new.clone()];
    app.equipment.libraries.index();
    let _ = app.update(Message::Equipment(Action::Arm(placed::key(&old))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(200., 300.))));
    let flask = app.tab.selected[0];
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/stopper-15"))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(600., 120.))));
    let stopper = app.tab.selected[0];
    let before = app.tab.doc.clone();
    let joint = |piece, joint: &str| JointRef {
        piece,
        joint: joint.into(),
    };
    solve::connect(
        &mut app.tab.doc,
        &[stopper],
        &joint(flask, "neck"),
        &joint(stopper, "bottom"),
    )
    .unwrap();
    app.changed(before);
    assert_eq!(app.tab.doc.seats.len(), 1, "{}", app.status);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Equipment(Action::Update {
        ids: vec![flask],
        key: placed::key(&new),
    }));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Updated to revision 2");
    let g = app.tab.doc.graphics.iter().find(|g| g.id == flask).unwrap();
    assert_eq!(g.piece.as_ref().unwrap().definition, placed::key(&new));
    assert_eq!(app.tab.doc.seats.len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

/// Where a click places `id` so that the middle of its hold `name` is at `at`.
fn click_for(app: &App, id: &str, name: &str, at: [f64; 2]) -> Point {
    let scale = placed::world_per_mm(&app.tab.doc, placed::DEFAULT_PT_PER_MM);
    let probe = placed::graphic(0, builtin::find(id).unwrap(), Point::default(), scale);
    let p = solve::ports(&probe)
        .into_iter()
        .find(|p| p.at.joint == name)
        .unwrap();
    let c = [
        p.point[0] + p.dir[0] * p.length / 2.,
        p.point[1] + p.dir[1] * p.length / 2.,
    ];
    Point::new((at[0] - c[0]) as f32, (at[1] - c[1]) as f32)
}

#[test]
fn a_clamp_placed_by_the_rod_mounts_on_it_in_one_undo_step() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/stand"))));
    let _ = app.update(Message::Canvas(Edit::Piece(Point::new(0., 0.))));
    let rod = solve::ports(&app.tab.doc.graphics[0]).remove(0);
    let at = [
        rod.point[0] + 3.,
        rod.point[1] + rod.dir[1] * rod.length / 2.,
    ];
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/clamp-3finger"))));
    let before = app.tab.doc.clone();
    let click = click_for(&app, "reshiki/clamp-3finger", "boss", at);
    let _ = app.update(Message::Canvas(Edit::Piece(click)));
    assert_eq!(app.tab.doc.seats.len(), 1, "{}", app.status);
    assert_eq!(app.status, "Mounted the clamp on the stand rod");
    // The rod says where along it the clamp is: half of its 560 mm.
    let doc = &app.tab.doc;
    let rod = solve::ports(&doc.graphics[0]).remove(0);
    let partner = solve::partner(doc, &rod.at).unwrap();
    assert_eq!(seat_state(doc, &rod, &partner), "Holds the clamp at 280 mm");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn add_clips_clips_the_selected_assembly_in_one_undo_step() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let neck = flask_neck(&mut app, Point::new(200., 300.));
    let _ = app.update(Message::Equipment(Action::Arm(k("reshiki/stopper-15"))));
    let scale = placed::world_per_mm(&app.tab.doc, placed::DEFAULT_PT_PER_MM);
    let probe = placed::graphic(
        0,
        builtin::find("reshiki/stopper-15").unwrap(),
        Point::default(),
        scale,
    );
    let shoulder = solve::ports(&probe)[0].point;
    let click = Point::new(neck.x - shoulder[0] as f32, neck.y - shoulder[1] as f32);
    let _ = app.update(Message::Canvas(Edit::Piece(click)));
    assert_eq!(app.tab.doc.seats.len(), 1, "{}", app.status);
    let _ = app.update(Message::Tool(Tool::Select));
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Equipment(Action::AddClips));
    assert_eq!(app.status, "Added 1 joint clip");
    let clip = app.tab.doc.seats[0].clip.expect("clipped");
    assert!(app.tab.selected.contains(&clip));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}
