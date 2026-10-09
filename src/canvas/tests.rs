use super::*;
use iced::widget::canvas::Program;

#[test]
fn unbounded_canvas_edges_accept_input_with_or_without_crosshair() -> Result<(), String> {
    let doc = Document::default();
    let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(400., 300.));
    for crosshair in [false, true] {
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Atom;
        canvas.guides.crosshair = crosshair;
        assert_eq!(canvas.guides.paper(bounds), bounds);
        for p in [Point::new(21., 51.), Point::new(419., 349.)] {
            let mut state = State::default();
            let cursor = mouse::Cursor::Available(p);
            canvas.update(
                &mut state,
                &Event::Mouse(mouse::Event::CursorMoved { position: p }),
                bounds,
                cursor,
            );
            let action = canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    bounds,
                    cursor,
                )
                .ok_or("Edge click should reach the canvas")?;
            let Some(Edit::Click(world)) = action.into_inner().0 else {
                return Err("Expected atom placement".into());
            };
            let expected = canvas
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            assert!(world.distance(expected) < 0.001);
        }
    }
    Ok(())
}

#[test]
fn scroll_pans_both_axes_and_command_or_control_zooms_at_pointer() -> Result<(), String> {
    use iced::keyboard::Modifiers;
    let doc = Document::default();
    let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(500., 350.));
    for rulers in [false, true] {
        for zoom in [0.5, 2.] {
            let mut canvas = chain_canvas(&doc, ChainMode::Straight);
            canvas.guides.rulers = rulers;
            canvas.camera.zoom = zoom;
            let paper = canvas.guides.paper(bounds);
            let local = Point::new(123., 87.);
            let pointer = Point::new(paper.x + local.x, paper.y + local.y);
            let anchor = canvas.camera.world(local, paper);
            let cursor = mouse::Cursor::Available(pointer);
            for (delta, expected_x, expected_y) in [
                (mouse::ScrollDelta::Pixels { x: 24., y: -18. }, 24., -18.),
                (mouse::ScrollDelta::Lines { x: -2., y: 1. }, -80., 40.),
            ] {
                let mut state = State::default();
                let action = canvas
                    .update(
                        &mut state,
                        &Event::Mouse(mouse::Event::WheelScrolled { delta }),
                        bounds,
                        cursor,
                    )
                    .ok_or("Scroll action")?;
                let Some(Edit::Pan(dx, dy)) = action.into_inner().0 else {
                    return Err("Unmodified scroll must pan".into());
                };
                let mut moved = canvas.camera;
                moved.center = moved.center.offset(-dx, -dy);
                let position = moved.screen(anchor, paper);
                assert!((position.x - local.x - expected_x).abs() < 0.001);
                assert!((position.y - local.y - expected_y).abs() < 0.001);
                for modifiers in [Modifiers::CTRL, Modifiers::COMMAND] {
                    let mut state = State {
                        modifiers,
                        ..Default::default()
                    };
                    let action = canvas
                        .update(
                            &mut state,
                            &Event::Mouse(mouse::Event::WheelScrolled { delta }),
                            bounds,
                            cursor,
                        )
                        .ok_or("Zoom action")?;
                    let Some(Edit::Zoom(factor, point)) = action.into_inner().0 else {
                        return Err("Modified scroll must zoom".into());
                    };
                    assert_eq!(point, anchor);
                    assert!((factor - 1.) * expected_y > 0.);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn rulers_exclude_editing_and_pointer_coordinates_use_the_inset_paper() {
    let doc = Document::default();
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        tool: Tool::Atom,
        guides: guides::Guides {
            rulers: true,
            crosshair: true,
            ..Default::default()
        },
        ..chain_canvas(&doc, ChainMode::Straight)
    };
    let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(400., 300.));
    let mut state = State::default();
    for p in [Point::new(45., 200.), Point::new(200., 63.)] {
        let cursor = mouse::Cursor::Available(p);
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved { position: p }),
            bounds,
            cursor,
        );
        assert!(
            canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    bounds,
                    cursor
                )
                .is_none()
        );
        assert!(state.gesture.is_none());
    }
    let p = Point::new(239., 213.);
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::CursorMoved { position: p }),
        bounds,
        mouse::Cursor::Available(p),
    );
    let click = canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(p),
        )
        .expect("atom click on paper");
    assert!(matches!(
        click.into_inner().0,
        Some(Edit::Click(World { x: 0., y: 0. }))
    ));
    state.modifiers = iced::keyboard::Modifiers::COMMAND;
    let action = canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0., y: 1. },
            }),
            bounds,
            mouse::Cursor::Available(p),
        )
        .expect("zoom on drawing");
    assert!(matches!(
        action.into_inner().0,
        Some(Edit::Zoom(_, World { x: 0., y: 0. }))
    ));
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::CursorLeft),
        bounds,
        mouse::Cursor::Unavailable,
    );
    assert!(state.cursor.is_none());
}

#[test]
fn free_ring_preset_drag_keeps_its_start_as_rotation_anchor() {
    let doc = Document::default();
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        tool: Tool::RingPreset(reshiki::rings::Preset::ChairUp),
        camera: Camera {
            center: World::default(),
            zoom: 1.,
        },
        ..chain_canvas(&doc, ChainMode::Straight)
    };
    assert!(matches!(
        pointer_gesture(&canvas, Point::new(200., 150.), Point::new(200., 80.)),
        Edit::RingPreset(
            reshiki::rings::Preset::ChairUp,
            World { x: 0., y: 0. },
            Some(World { x: 0., y: -70. }),
            false,
            false
        )
    ));
    assert!(doc.atoms.is_empty());
}

#[test]
fn regular_ring_drag_preview_and_release_agree_at_different_zooms() {
    let source = reshiki::rings::Preset::Regular.document(42., false);
    let a = source.atom(source.bonds[0].a).unwrap().position;
    let b = source.atom(source.bonds[0].b).unwrap().position;
    let anchor = World::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
    let mut cases = vec![
        (source.clone(), anchor, World::default(), true),
        (
            source,
            anchor,
            World::new(anchor.x * 2., anchor.y * 2.),
            false,
        ),
    ];
    let mut phosphorus = Document::default();
    let p = phosphorus.add_atom("P", World::default());
    let carbon = phosphorus.add_atom("C", World::new(-42., 0.));
    phosphorus.add_bond(p, carbon, 1, "plain");
    cases.push((
        phosphorus.clone(),
        World::default(),
        World::new(40., 0.),
        false,
    ));
    let ligand = phosphorus.add_atom("C", World::new(-75., 28.));
    phosphorus.add_bond(carbon, ligand, 1, "plain");
    let anchor = reshiki::attachments::add(
        &mut phosphorus,
        &[carbon, ligand],
        reshiki::attachments::Kind::MultiCenter,
    )
    .unwrap();
    let metal = phosphorus.add_atom("Fe", World::new(-90., -40.));
    phosphorus.add_bond(anchor, metal, 1, "plain");
    cases.push((phosphorus, World::default(), World::new(40., 0.), false));
    for display in ["bold", "dashed"] {
        let mut styled = Document::default();
        let a = styled.add_atom("C", World::new(-30., 0.));
        let b = styled.add_atom("C", World::new(30., 0.));
        styled.add_bond(a, b, 2, display);
        cases.push((styled, World::default(), World::new(0., 40.), false));
    }
    for zoom in [0.5, 1., 2.5] {
        for (source, anchor, end, rejected) in &cases {
            let mut canvas = chain_canvas(source, ChainMode::Straight);
            canvas.tool = Tool::Ring;
            canvas.ring_size = 6;
            canvas.aromatic_ring = false;
            canvas.camera.zoom = zoom;
            canvas.camera.center = *anchor;
            let start = Point::new(200., 150.);
            let finish = Point::new(
                200. + (end.x - anchor.x) * zoom,
                150. + (end.y - anchor.y) * zoom,
            );
            let Edit::Ring(committed_anchor, committed_direction) =
                pointer_gesture(&canvas, start, finish)
            else {
                panic!("Ring drag must publish its attachment geometry");
            };
            let (preview_anchor, preview_direction) = ring_gesture(*anchor, *end, true, 10. / zoom);
            let mut preview = source.clone();
            let expected = reshiki::editing::ring_oriented(
                &mut preview,
                preview_anchor,
                6,
                false,
                10. / zoom,
                preview_direction,
            );
            let mut committed = source.clone();
            let actual = reshiki::editing::ring_oriented(
                &mut committed,
                committed_anchor,
                6,
                false,
                10. / zoom,
                committed_direction,
            );
            assert_eq!(actual, expected);
            assert_eq!(actual.is_err(), *rejected);
            assert_eq!(committed, preview);
            if *rejected {
                assert_eq!(&preview, source);
            } else {
                assert_eq!(
                    &committed.bonds[..source.bonds.len()],
                    source.bonds.as_slice()
                );
            }
        }
    }
}

#[test]
fn arrow_handle_drag_is_one_edit_and_midpoint_hit_follows_curve() {
    use reshiki::arrows::{ArrowStyle, Preset};
    let mut doc = Document::default();
    doc.arrows.push(reshiki::document::Arrow::new(
        1,
        World::new(-60., 0.),
        World::new(60., 0.),
        Preset::Forward,
        ArrowStyle::default(),
    ));
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        tool: Tool::Select,
        selected: &[1],
        camera: Camera {
            center: World::default(),
            zoom: 1.,
        },
        ..chain_canvas(&doc, ChainMode::Straight)
    };
    assert!(matches!(
        pointer_gesture(&canvas, Point::new(200., 150.), Point::new(200., 95.)),
        Edit::ArrowHandle(1, 2, World { x: 0., y: -55. })
    ));
    assert!(doc.arrows[0].control.is_none());
    doc.arrows[0].edit_handle(2, World::new(0., -55.));
    assert_eq!(hit_object(&doc, World::new(0., -55.), 4.), Some(1));
    assert_eq!(hit_object(&doc, World::new(0., 0.), 4.), None);
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        tool: Tool::Arrow,
        selected: &[1],
        camera: Camera {
            center: World::default(),
            zoom: 1.,
        },
        ..chain_canvas(&doc, ChainMode::Straight)
    };
    assert!(matches!(
        pointer_gesture(&canvas, Point::new(200., 95.), Point::new(200., 95.)),
        Edit::ArrowClick(1)
    ));
    assert!(matches!(
        pointer_gesture(&canvas, Point::new(260., 150.), Point::new(300., 150.)),
        Edit::ArrowHandle(1, 1, World { x: 100., y: 0. })
    ));
}

#[test]
fn template_anchor_preview_hit_tests_atoms_bonds_and_empty_space() {
    use reshiki::templates::Anchor;
    let mut doc = Document::default();
    let a = doc.add_atom("N", World::new(-42., 0.));
    let b = doc.add_atom("C", World::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let preview = TemplateAnchorPreview {
        document: &doc,
        anchor: Anchor::Auto,
    };
    let bounds = Rectangle::new(Point::new(20., 30.), iced::Size::new(220., 150.));
    let camera = preview.camera(bounds);
    for (point, expected) in [
        (World::new(-42., 0.), Anchor::Atom(a)),
        (World::default(), Anchor::Bond(a, b)),
    ] {
        let p = camera.screen(point, bounds) + Vector::new(bounds.x, bounds.y);
        let action = preview
            .update(
                &mut None,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(p),
            )
            .unwrap();
        assert_eq!(action.into_inner().0, Some(expected));
    }
    assert!(
        preview
            .update(
                &mut None,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(21., 31.))
            )
            .is_none()
    );
}

#[test]
fn aromatic_drag_carries_the_same_xyz_as_the_live_preview() -> Result<(), String> {
    use reshiki::projection::growth::{self, Plane};
    let mut doc = reshiki::rings::Preset::Benzene.document(42., false);
    let ids = doc.all_ids();
    reshiki::projection::tilt(&mut doc, &ids, 55., false);
    let atom = doc.atoms.get(1).ok_or("Ring atom")?;
    let plane = Plane::at(&doc, atom.id).ok_or("Plane")?;
    let cursor = plane.outward(84.).ok_or("Cursor")?.position;
    let expected = plane
        .endpoint(cursor, BondDrawing::default())
        .ok_or("Preview")?;
    for tool in [Tool::Atom, Tool::Bond(1)] {
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = tool;
        let press = Point::new(200. + atom.position.x, 150. + atom.position.y);
        let release = Point::new(200. + cursor.x, 150. + cursor.y);
        let Edit::PlaneBond(id, end) = pointer_gesture(&canvas, press, release) else {
            return Err("Expected a bond with retained depth".into());
        };
        assert_eq!(id, atom.id);
        assert_eq!(end, expected);
        assert!(end.depth.abs() > 1.);
        let (preview, added) =
            growth::place(&doc, id, end, "C", reshiki::bonds::BondPreset::Single)?;
        assert_eq!(preview.atom(added).ok_or("Added")?.depth, end.depth);
    }
    // Connecting an existing atom keeps its existing coordinates/depth.
    let (p, target) = bond_target_with(
        &doc,
        atom.position,
        doc.atoms.first().ok_or("Target")?.position,
        Some(atom.id),
        12.,
        BondDrawing::default(),
    );
    assert_eq!(target, doc.atoms.first().map(|a| a.id));
    assert_eq!(p, doc.atoms.first().ok_or("Target")?.position);
    Ok(())
}

#[test]
fn atom_click_and_drag_use_distinct_edits_and_bond_constraints() -> Result<(), String> {
    let mut doc = Document::default();
    let atom = doc.add_atom("C", World::default());
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Atom;
    canvas.element = "O";
    let start = Point::new(200., 150.);
    assert!(matches!(
        pointer_gesture(&canvas, start, start),
        Edit::Click(_)
    ));
    let Edit::Bond(a, b, Some(id), None) = pointer_gesture(&canvas, start, Point::new(255., 178.))
    else {
        return Err("Atom drag must create a bond edit".into());
    };
    assert_eq!(id, atom);
    assert!((a.distance(b) - 42.).abs() < 0.001);
    let angle = (b.y - a.y).atan2(b.x - a.x).to_degrees();
    assert!((angle - 30.).abs() < 0.001);
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
    let mut state = State {
        modifiers: iced::keyboard::Modifiers::ALT,
        ..Default::default()
    };
    let end = Point::new(255., 178.);
    for event in [
        mouse::Event::CursorMoved { position: start },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::CursorMoved { position: end },
    ] {
        canvas.update(
            &mut state,
            &Event::Mouse(event),
            bounds,
            mouse::Cursor::Available(end),
        );
    }
    let edit = canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(end),
        )
        .and_then(|a| a.into_inner().0)
        .ok_or("Missing free drag")?;
    assert!(
        matches!(edit, Edit::Bond(_, p, Some(_), None) if p.distance(World::new(55.,28.)) < 0.001),
        "{edit:?}"
    );
    Ok(())
}

#[test]
fn ring_modifier_draws_a_circle_and_does_not_change_chairs_or_haworth() -> Result<(), String> {
    use reshiki::rings::Preset;
    let modifier = if cfg!(target_os = "macos") {
        iced::keyboard::Modifiers::LOGO
    } else {
        iced::keyboard::Modifiers::CTRL
    };
    let doc = Document::default();
    for (tool, size) in [
        (Tool::Ring, 7),
        (Tool::RingPreset(Preset::Benzene), 6),
        (Tool::RingPreset(Preset::Cyclopentadiene), 5),
    ] {
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = tool;
        canvas.ring_size = 7;
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
        let p = Point::new(200., 150.);
        let mut state = State {
            modifiers: modifier,
            ..Default::default()
        };
        for event in [
            mouse::Event::CursorMoved { position: p },
            mouse::Event::ButtonPressed(mouse::Button::Left),
        ] {
            canvas.update(
                &mut state,
                &Event::Mouse(event),
                bounds,
                mouse::Cursor::Available(p),
            );
        }
        let edit = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(p),
            )
            .and_then(|a| a.into_inner().0)
            .ok_or("Missing ring gesture")?;
        assert!(matches!(edit, Edit::DelocalizedRing(_, _, n) if n == size));
    }
    for preset in [
        Preset::ChairUp,
        Preset::ChairDown,
        Preset::HaworthFive,
        Preset::HaworthSix,
    ] {
        assert_eq!(
            delocalized_ring_size(Tool::RingPreset(preset), 6, modifier),
            None
        );
    }
    assert_eq!(
        delocalized_ring_size(Tool::Ring, 6, iced::keyboard::Modifiers::empty()),
        None
    );
    Ok(())
}

pub(super) fn chain_canvas(doc: &Document, mode: ChainMode) -> MoleculeCanvas<'_> {
    static STYLE: std::sync::LazyLock<GraphicStyle> =
        std::sync::LazyLock::new(GraphicStyle::default);
    MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        doc,
        selected: &[],
        tool: Tool::Chain(mode),
        camera: Camera {
            center: World::default(),
            zoom: 1.,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &STYLE,
        bracket_sides: BracketSides::Both,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
    }
}

fn tilt_pointer(
    canvas: &MoleculeCanvas<'_>,
    state: &mut State,
    event: mouse::Event,
) -> Option<Edit> {
    let bounds = Rectangle::new(Point::new(20., 30.), iced::Size::new(400., 300.));
    canvas
        .update(
            state,
            &Event::Mouse(event),
            bounds,
            mouse::Cursor::Unavailable,
        )
        .and_then(|action| action.into_inner().0)
}

#[test]
fn tilt_drag_preserves_partial_selection_and_has_zoom_independent_snapping() -> Result<(), String> {
    let mut doc = Document::default();
    let ring = reshiki::editing::ring(&mut doc, World::default(), 6, true, 5.);
    let selected: Vec<_> = ring.iter().take(3).copied().collect();
    let atom = doc
        .atom(*selected.first().ok_or("selected atom")?)
        .ok_or("atom")?;
    let before = doc.clone();
    for zoom in [0.5, 1., 2.] {
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Tilt;
        canvas.selected = &selected;
        canvas.camera.zoom = zoom;
        let mut state = State {
            modifiers: iced::keyboard::Modifiers::SHIFT,
            ..Default::default()
        };
        let start = Point::new(220. + atom.position.x * zoom, 180. + atom.position.y * zoom);
        tilt_pointer(
            &canvas,
            &mut state,
            mouse::Event::CursorMoved { position: start },
        );
        tilt_pointer(
            &canvas,
            &mut state,
            mouse::Event::ButtonPressed(mouse::Button::Left),
        );
        assert!(matches!(&state.gesture, Some(Gesture::Tilt(drag)) if drag.ids == selected));
        let end = Point::new(start.x + 36., start.y - 26.);
        let motion = tilt_pointer(
            &canvas,
            &mut state,
            mouse::Event::CursorMoved { position: end },
        );
        assert!(motion.is_none(), "Motion must only redraw the preview");
        assert_eq!(doc, before);
        let edit = tilt_pointer(
            &canvas,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
        );
        assert!(matches!(edit, Some(Edit::Tilt { ids, x: 15., y: 15. }) if ids == selected));
        assert!(state.gesture.is_none());
    }
    Ok(())
}

#[test]
fn tilt_click_selects_a_molecule_without_rotating_and_blank_drag_selects() -> Result<(), String> {
    let mut doc = Document::default();
    let a = doc.add_atom("N", World::new(-30., 0.));
    let b = doc.add_atom("C", World::new(30., 0.));
    doc.add_bond(a, b, 1, "plain");
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Tilt;
    let mut state = State::default();
    for (start, end, expected) in [
        (Point::new(190., 180.), Point::new(191., 180.), vec![a, b]),
        (Point::new(170., 145.), Point::new(270., 210.), vec![a, b]),
        (Point::new(350., 300.), Point::new(350., 300.), vec![]),
    ] {
        tilt_pointer(
            &canvas,
            &mut state,
            mouse::Event::CursorMoved { position: start },
        );
        tilt_pointer(
            &canvas,
            &mut state,
            mouse::Event::ButtonPressed(mouse::Button::Left),
        );
        tilt_pointer(
            &canvas,
            &mut state,
            mouse::Event::CursorMoved { position: end },
        );
        let edit = tilt_pointer(
            &canvas,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
        )
        .ok_or("selection edit")?;
        let Edit::Select(mut ids) = edit else {
            return Err("A click must not rotate".into());
        };
        ids.sort_unstable();
        assert_eq!(ids, expected);
    }
    Ok(())
}

#[test]
fn aromatic_ring_interior_opens_its_menu_and_starts_tilt_without_prior_selection()
-> Result<(), String> {
    let mut doc = Document::default();
    let mut ids = reshiki::editing::ring(&mut doc, World::default(), 6, true, 5.);
    ids.sort_unstable();
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Tilt;
    let mut state = State::default();
    let center = Point::new(220., 180.);
    tilt_pointer(
        &canvas,
        &mut state,
        mouse::Event::CursorMoved { position: center },
    );
    let edit = tilt_pointer(
        &canvas,
        &mut state,
        mouse::Event::ButtonPressed(mouse::Button::Right),
    )
    .ok_or("context menu")?;
    let Edit::ContextMenu { mut selected, .. } = edit else {
        return Err("Expected context menu".into());
    };
    selected.sort_unstable();
    assert_eq!(selected, ids);
    tilt_pointer(
        &canvas,
        &mut state,
        mouse::Event::ButtonPressed(mouse::Button::Left),
    );
    let Some(Gesture::Tilt(drag)) = &state.gesture else {
        return Err("Expected tilt from ring interior".into());
    };
    let mut selected = drag.ids.clone();
    selected.sort_unstable();
    assert_eq!(selected, ids);
    Ok(())
}

#[test]
fn tilt_cancel_focus_loss_outside_release_and_tool_switch_do_not_commit() {
    let doc = reshiki::rings::Preset::Regular.document(42., false);
    let selected = doc.all_ids();
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Tilt;
    canvas.selected = &selected;
    let bounds = Rectangle::new(Point::new(20., 30.), iced::Size::new(400., 300.));
    for cancel in [
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        }),
        Event::Window(iced::window::Event::Unfocused),
    ] {
        let mut state = State {
            gesture: Some(Gesture::Tilt(tilt::TiltDrag {
                ids: selected.clone(),
                start: Point::new(200., 150.),
            })),
            cursor: Some(Point::new(250., 160.)),
            ..Default::default()
        };
        canvas.update(&mut state, &cancel, bounds, mouse::Cursor::Unavailable);
        assert!(state.gesture.is_none());
        assert!(
            tilt_pointer(
                &canvas,
                &mut state,
                mouse::Event::ButtonReleased(mouse::Button::Left)
            )
            .is_none()
        );
    }
    for (tool, position) in [
        (Tool::Tilt, Point::new(500., 150.)),
        (Tool::Select, Point::new(250., 160.)),
    ] {
        canvas.tool = tool;
        let mut state = State {
            gesture: Some(Gesture::Tilt(tilt::TiltDrag {
                ids: selected.clone(),
                start: Point::new(200., 150.),
            })),
            cursor: Some(position),
            ..Default::default()
        };
        assert!(
            tilt_pointer(
                &canvas,
                &mut state,
                mouse::Event::ButtonReleased(mouse::Button::Left)
            )
            .is_none()
        );
        assert!(state.gesture.is_none());
    }
}

#[test]
fn dragging_redraws_the_canvas_without_publishing_intermediate_application_updates() {
    let mut doc = Document::default();
    let atom = doc.add_atom("C", World::default());
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    let mut state = State::default();
    let start = Point::new(200., 150.);
    let end = Point::new(260., 180.);
    let cursor = mouse::Cursor::Available(end);
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::CursorMoved { position: start }),
        bounds,
        cursor,
    );
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        bounds,
        cursor,
    );
    for x in 201..=260 {
        let action = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(x as f32, 180.),
                }),
                bounds,
                cursor,
            )
            .unwrap();
        assert!(action.into_inner().0.is_none());
    }
    let action = canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .unwrap();
    assert!(matches!(action.into_inner().0, Some(Edit::Move(ids, 60., 30.)) if ids == vec![atom]));
    assert_eq!(doc.atom(atom).unwrap().position, World::default());
}

#[test]
fn centroid_drag_moves_ligand_and_point_edit_mode_moves_only_the_point() -> Result<(), String> {
    let mut doc = Document::default();
    let metal = doc.add_atom("Fe", World::default());
    let (doc, _) = reshiki::hotkeys::atom_edit(&doc, metal, "j", 42.).ok_or("Shortcut")??;
    let anchor = doc
        .atoms
        .iter()
        .find(|a| a.attachment.is_some())
        .ok_or("Point")?;
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    let start = canvas.camera.screen(anchor.position, bounds);
    let end = Point::new(start.x + 35., start.y + 25.);
    for tool in [Tool::Select, Tool::EditPoints] {
        canvas.tool = tool;
        let mut state = State {
            modifiers: iced::keyboard::Modifiers::ALT,
            ..Default::default()
        };
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(start),
        );
        let Some(Gesture::Move {
            ids: preview_ids, ..
        }) = &state.gesture
        else {
            return Err("Missing drag preview".into());
        };
        let preview_ids = preview_ids.clone();
        let result = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(end),
            )
            .and_then(|action| action.into_inner().0)
            .ok_or("Missing move")?;
        let Edit::Move(ids, dx, dy) = result else {
            return Err("Expected move".into());
        };
        assert_eq!(ids, preview_ids);
        assert!((dx - 35.).abs() < 0.001 && (dy - 25.).abs() < 0.001);
        if tool == Tool::Select {
            assert_eq!(ids.len(), 6);
            assert!(anchor.centroid.iter().all(|id| ids.contains(id)));
            assert!(!ids.contains(&metal));
        } else {
            assert_eq!(ids, vec![anchor.id]);
        }
        let mut preview = doc.clone();
        preview.translate(&preview_ids, dx, dy);
        let mut committed = doc.clone();
        committed.translate(&ids, dx, dy);
        assert_eq!(preview, committed);
        assert_eq!(committed.atom(metal), doc.atom(metal));
    }
    Ok(())
}

#[test]
fn bonded_move_release_uses_constraints_and_live_option_override() -> Result<(), String> {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::default());
    let b = doc.add_atom("O", World::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    for free in [false, true] {
        let mut state = State {
            gesture: Some(Gesture::Move {
                start: World::new(44., 1.),
                ids: vec![b],
                clicked: vec![b],
            }),
            ..Default::default()
        };
        let end = Point::new(279., 183.);
        if free {
            canvas.update(
                &mut state,
                &Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                    iced::keyboard::Modifiers::ALT,
                )),
                bounds,
                mouse::Cursor::Available(end),
            );
        }
        let result = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(end),
            )
            .and_then(|action| action.into_inner().0)
            .ok_or("Missing release edit")?;
        let Edit::Move(ids, dx, dy) = result else {
            return Err("Expected movement".into());
        };
        let requested = World::new(35., 32.);
        let preview = movement::delta(
            &doc,
            &[b],
            requested,
            canvas.bond_drawing.unconstrained(free),
        );
        assert_eq!(ids, vec![b]);
        assert_eq!(World::new(dx, dy), preview);
        if free {
            assert_eq!(preview, requested);
        } else {
            assert!((World::new(42. + dx, dy).distance(World::default()) - 42.).abs() < 0.001);
        }
    }
    assert_eq!(doc.atom(b).ok_or("atom")?.position, World::new(42., 0.));
    Ok(())
}

#[test]
fn secondary_click_selects_its_target_and_preserves_an_existing_multi_selection() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::default());
    let b = doc.add_atom("N", World::new(60., 0.));
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    let selected = [a, b];
    canvas.selected = &selected;
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    let mut state = State::default();
    for (point, expected) in [
        (Point::new(200., 150.), vec![a, b]),
        (Point::new(350., 250.), vec![]),
    ] {
        let cursor = mouse::Cursor::Available(point);
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved { position: point }),
            bounds,
            cursor,
        );
        let action = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
                bounds,
                cursor,
            )
            .unwrap();
        assert!(
            matches!(action.into_inner().0, Some(Edit::ContextMenu { selected, position }) if selected == expected && position == point)
        );
        assert!(state.gesture.is_none());
    }
}

#[test]
fn chain_click_and_fast_drag_keep_attachment_count_and_preview_geometry() {
    let mut doc = Document::default();
    let source = doc.add_atom("N", World::new(-120., 0.));
    let target = doc.add_atom("O", World::new(120., 0.));
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.chain_drawing.atoms = Some(7);
    let Edit::Chain {
        points,
        source: a,
        target: b,
    } = pointer_gesture(&canvas, Point::new(80., 150.), Point::new(320., 150.))
    else {
        panic!("chain gesture")
    };
    assert_eq!((a, b), (Some(source), Some(target)));
    assert_eq!(points.len(), 7);
    let (preview, preview_target) = canvas.chain_plan(
        (World::new(-120., 0.), World::new(-120., 0.)),
        Some(source),
        &[World::new(-120., 0.)],
        (false, true),
        World::new(120., 0.),
        iced::keyboard::Modifiers::empty(),
    );
    assert_eq!(preview, points);
    assert_eq!(preview_target, b);
    canvas.chain_drawing.atoms = Some(4);
    let Edit::Chain {
        points,
        source: a,
        target: b,
    } = pointer_gesture(&canvas, Point::new(84., 153.), Point::new(84., 153.))
    else {
        panic!("attached click")
    };
    assert_eq!(a, Some(source));
    assert_eq!(b, None);
    assert_eq!(points.len(), 4);
    assert_eq!(points[0], doc.atom(source).unwrap().position);

    let initial = chains::straight(
        World::default(),
        World::new(200., 0.),
        false,
        BondDrawing::default(),
        ChainDrawing {
            atoms: Some(6),
            ..Default::default()
        },
        false,
    );
    let (doc, ids) = chains::place(&Document::default(), &initial, None, None, 8.).unwrap();
    let canvas = chain_canvas(&doc, ChainMode::Straight);
    let last = *initial.last().unwrap();
    let (extension, _) = canvas.chain_plan(
        (last, last),
        ids.last().copied(),
        &[last],
        (false, false),
        last,
        iced::keyboard::Modifiers::empty(),
    );
    assert_eq!(extension.len(), 6);
    assert!(extension.iter().all(|p| p.y.abs() <= 21.001));
    assert!(extension.windows(2).all(|p| p[1].x > p[0].x));
}

#[test]
fn snaking_gesture_bends_with_control_and_cancels_on_focus_loss() {
    let doc = Document::default();
    let canvas = chain_canvas(&doc, ChainMode::Straight);
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(600., 500.));
    let mut state = State::default();
    let positions = [
        Point::new(100., 280.),
        Point::new(325., 280.),
        Point::new(325., 80.),
    ];
    let cursor = mouse::Cursor::Available(positions[2]);
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::CursorMoved {
            position: positions[0],
        }),
        bounds,
        cursor,
    );
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        bounds,
        cursor,
    );
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::CursorMoved {
            position: positions[1],
        }),
        bounds,
        cursor,
    );
    canvas.update(
        &mut state,
        &Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            iced::keyboard::Modifiers::CTRL,
        )),
        bounds,
        cursor,
    );
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::CursorMoved {
            position: positions[2],
        }),
        bounds,
        cursor,
    );
    let Some(Gesture::Chain {
        points, snaking, ..
    }) = &state.gesture
    else {
        panic!()
    };
    assert!(*snaking);
    assert!(points.len() > 7);
    assert!(points.last().unwrap().y < -100.);
    canvas.update(
        &mut state,
        &Event::Window(iced::window::Event::Unfocused),
        bounds,
        cursor,
    );
    assert!(
        canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor
            )
            .is_none()
    );
    assert!(doc.atoms.is_empty());
}

#[test]
fn attachment_bond_drag_reaches_beyond_the_ring_and_snaps_to_metal() -> Result<(), String> {
    let mut doc = Document::default();
    let members = reshiki::editing::ring(&mut doc, World::new(0., 0.), 6, true, 5.);
    let point =
        reshiki::attachments::add(&mut doc, &members, reshiki::attachments::Kind::MultiCenter)?;
    let start = doc.atom(point).ok_or("Missing attachment")?.position;
    let settings = BondDrawing::default();
    for degrees in (0..360).step_by(30) {
        let angle = (degrees as f32).to_radians();
        let cursor = start.offset(126. * angle.cos(), 126. * angle.sin());
        let (end, target) = bond_target_with(&doc, start, cursor, Some(point), 8., settings);
        assert!(target.is_none());
        assert!((end.distance(start) - 126.).abs() < 0.01);
    }
    let metal = doc.add_atom("Fe", start.offset(110., 91.));
    let target = doc.atom(metal).ok_or("Missing metal")?.position;
    assert_eq!(
        bond_target_with(&doc, start, target, Some(point), 8., settings),
        (target, Some(metal))
    );
    assert_eq!(
        bond_target_with(&doc, target, start, Some(metal), 8., settings),
        (start, Some(point))
    );
    assert!(settings.fixed_length);
    Ok(())
}

#[test]
fn alt_temporarily_frees_bond_constraints_without_changing_tool_preferences() {
    let doc = Document::default();
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Bond(1);
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
    let mut state = State::default();
    let start = Point::new(120., 150.);
    let end = Point::new(246., 202.);
    let cursor = mouse::Cursor::Available(end);
    canvas.update(
        &mut state,
        &Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            iced::keyboard::Modifiers::ALT,
        )),
        bounds,
        cursor,
    );
    for event in [
        mouse::Event::CursorMoved { position: start },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::CursorMoved { position: end },
    ] {
        canvas.update(&mut state, &Event::Mouse(event), bounds, cursor);
    }
    let edit = canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .unwrap()
        .into_inner()
        .0
        .unwrap();
    let Edit::Bond(a, b, None, None) = edit else {
        panic!()
    };
    assert_eq!(a, World::new(-80., 0.));
    assert_eq!(b, World::new(46., 52.));
    assert!(canvas.bond_drawing.fixed_length && canvas.bond_drawing.fixed_angles);
}

#[test]
fn freeform_selection_tracks_events_adds_subtracts_and_cancels() {
    let mut doc = Document::default();
    doc.add_atom("C", World::new(-50., 0.));
    doc.add_atom("O", World::new(50., 0.));
    let style = GraphicStyle::default();
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[2],
        tool: Tool::Lasso,
        camera: Camera {
            center: World::default(),
            zoom: 1.,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &style,
        bracket_sides: BracketSides::Both,
    };
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
    let points = [
        Point::new(100., 100.),
        Point::new(190., 100.),
        Point::new(190., 190.),
        Point::new(100., 190.),
        Point::new(100., 100.),
    ];
    for (mods, expected) in [
        (iced::keyboard::Modifiers::empty(), vec![1]),
        (iced::keyboard::Modifiers::SHIFT, vec![2, 1]),
    ] {
        let mut state = State {
            modifiers: mods,
            ..Default::default()
        };
        for (i, p) in points.iter().enumerate() {
            canvas.update(
                &mut state,
                &Event::Mouse(mouse::Event::CursorMoved { position: *p }),
                bounds,
                mouse::Cursor::Available(*p),
            );
            if i == 0 {
                canvas.update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    bounds,
                    mouse::Cursor::Available(*p),
                );
            }
        }
        let result = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(points[0]),
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        let Edit::Select(ids) = result else {
            panic!("expected region selection")
        };
        assert_eq!(ids, expected);
    }
    assert_eq!(
        region_selection(
            &doc,
            &[1, 2],
            &[
                World::new(-100., -50.),
                World::new(0., -50.),
                World::new(0., 50.),
                World::new(-100., 50.)
            ],
            iced::keyboard::Modifiers::ALT
        ),
        vec![2]
    );
    let mut state = State {
        gesture: Some(Gesture::Lasso {
            points: vec![World::default()],
        }),
        ..Default::default()
    };
    canvas.update(
        &mut state,
        &Event::Window(iced::window::Event::Unfocused),
        bounds,
        mouse::Cursor::Unavailable,
    );
    assert!(state.gesture.is_none());
    assert_eq!(doc.atoms.len(), 2);
}

#[test]
fn group_clicks_move_all_members_and_alt_selects_a_member() {
    let mut doc = Document::default();
    doc.add_atom("C", World::new(-50., 0.));
    doc.add_atom("O", World::new(50., 0.));
    doc.group_selection(&[1, 2]).unwrap();
    let style = GraphicStyle::default();
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[],
        tool: Tool::Select,
        camera: Camera {
            center: World::default(),
            zoom: 1.,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &style,
        bracket_sides: BracketSides::Both,
    };
    let result = pointer_gesture(&canvas, Point::new(150., 150.), Point::new(170., 170.));
    let Edit::Move(ids, dx, dy) = result else {
        panic!("expected group move")
    };
    assert_eq!(ids, vec![1, 2]);
    assert_eq!((dx, dy), (20., 20.));
    let mut state = State {
        modifiers: iced::keyboard::Modifiers::ALT,
        ..Default::default()
    };
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
    let cursor = mouse::Cursor::Available(Point::new(150., 150.));
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        bounds,
        cursor,
    );
    let result = canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .unwrap()
        .into_inner()
        .0
        .unwrap();
    let Edit::Select(ids) = result else {
        panic!("expected member selection")
    };
    assert_eq!(ids, vec![1]);

    // Alt-drag edits a member immediately, even when its group is selected.
    let selected = [1, 2];
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        selected: &selected,
        ..canvas
    };
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        bounds,
        cursor,
    );
    let result = canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(Point::new(175., 150.)),
        )
        .unwrap()
        .into_inner()
        .0
        .unwrap();
    let Edit::Move(ids, dx, dy) = result else {
        panic!("expected member move")
    };
    assert_eq!(ids, vec![1]);
    assert_eq!((dx, dy), (25., 0.));
}

#[test]
fn filled_background_leaves_atoms_and_bonds_selectable() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-20., 0.));
    let b = doc.add_atom("O", World::new(20., 0.));
    doc.add_bond(a, b, 1, "plain");
    let mut g = Graphic::dragged(
        3,
        reshiki::graphics::GraphicKind::Rectangle,
        World::new(-50., -50.),
        World::new(50., 50.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    g.style.fill = Some(reshiki::palette::Color::Custom([220, 239, 233]));
    doc.graphics.push(g);
    assert_eq!(hit_selection(&doc, World::new(-20., 0.), 5.), vec![a]);
    assert_eq!(hit_selection(&doc, World::default(), 5.), vec![a, b]);
    assert_eq!(hit_selection(&doc, World::new(0., 30.), 5.), vec![3]);
    doc.graphics[0].layer = 1;
    assert_eq!(hit_selection(&doc, World::default(), 5.), vec![3]);
}

#[test]
fn graphic_and_curve_point_drags_publish_one_edit_and_do_not_mutate_preview() {
    let doc = Document::default();
    let style = GraphicStyle::default();
    let mut canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[],
        tool: Tool::Graphic(reshiki::graphics::GraphicKind::Rectangle),
        camera: Camera {
            center: World::default(),
            zoom: 1.,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &style,
        bracket_sides: BracketSides::Both,
    };
    assert!(matches!(
        pointer_gesture(&canvas, Point::new(150., 100.), Point::new(250., 200.)),
        Edit::Graphic(World { x: -50., y: -50. }, World { x: 50., y: 50. }, false)
    ));
    assert!(doc.graphics.is_empty());
    let mut with_curve = doc.clone();
    with_curve.graphics.push(Graphic::dragged(
        1,
        reshiki::graphics::GraphicKind::Curve,
        World::new(-50., 0.),
        World::new(50., 0.),
        style.clone(),
        BracketSides::Both,
        false,
    ));
    canvas.doc = &with_curve;
    canvas.tool = Tool::EditPoints;
    canvas.selected = &[1];
    let original = with_curve.clone();
    assert!(matches!(
        pointer_gesture(&canvas, Point::new(183., 100.), Point::new(200., 80.)),
        Edit::GraphicPoint(1, 1, _)
    ));
    assert_eq!(with_curve, original);

    let mut with_closed_path = with_curve.clone();
    with_closed_path.graphics[0].edit_point(3, World::new(-50., 0.));
    with_closed_path.graphics[0].path.push(PathCommand::Close);
    canvas.doc = &with_closed_path;
    let original = with_closed_path.clone();
    assert!(
        matches!(
            pointer_gesture(&canvas, Point::new(150., 150.), Point::new(140., 140.)),
            Edit::GraphicPoint(1, 0, _)
        ),
        "coincident closed-path handles must retain first-point priority"
    );
    assert_eq!(with_closed_path, original);

    let mut with_arc = doc.clone();
    with_arc.graphics.push(
        Graphic::dragged(
            1,
            reshiki::graphics::GraphicKind::Arc,
            World::new(-50., -50.),
            World::new(50., 50.),
            style.clone(),
            BracketSides::Both,
            false,
        )
        .with_arc(reshiki::graphics::ArcGeometry {
            start_degrees: 180.,
            sweep_degrees: 360.,
        }),
    );
    canvas.doc = &with_arc;
    let original = with_arc.clone();
    assert!(
        matches!(
            pointer_gesture(&canvas, Point::new(150., 150.), Point::new(200., 100.)),
            Edit::GraphicPoint(1, 1, _)
        ),
        "coincident full-circle endpoints must let the end be dragged open"
    );
    assert_eq!(with_arc, original);
}

#[test]
fn double_click_edits_grouped_labels_but_drag_moves_the_group() {
    let mut doc = Document::default();
    let atom = doc.add_atom("C", World::new(-100., -80.));
    let label = doc.next_id();
    doc.annotations.push(reshiki::document::Annotation {
        id: label,
        position: World::default(),
        text: "Reaction conditions".into(),
        format: Default::default(),
    });
    doc.group_selection(&[atom, label]).unwrap();
    let style = GraphicStyle::default();
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[],
        tool: Tool::Select,
        camera: Camera {
            center: World::default(),
            zoom: 1.,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &style,
        bracket_sides: BracketSides::Both,
    };
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
    let point = Point::new(230., 160.);
    let cursor = mouse::Cursor::Available(point);
    let mut state = State::default();
    for second in [false, true] {
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        );
        let edit = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        if second {
            assert!(matches!(edit, Edit::BeginText(id) if id == label));
        } else {
            assert!(
                matches!(edit, Edit::Select(ids) if ids.contains(&label) && ids.contains(&atom))
            );
        }
    }
    assert!(
        matches!(pointer_gesture(&canvas, point, Point::new(245., 180.)), Edit::Move(ids, _, _) if ids.contains(&label) && ids.contains(&atom))
    );
}

fn pointer_gesture(canvas: &MoleculeCanvas<'_>, start: Point, end: Point) -> Edit {
    pointer_gesture_with(canvas, start, end, Default::default())
}

fn pointer_gesture_with(
    canvas: &MoleculeCanvas<'_>,
    start: Point,
    end: Point,
    modifiers: iced::keyboard::Modifiers,
) -> Edit {
    let mut state = State {
        modifiers,
        ..Default::default()
    };
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400.0, 300.0));
    let cursor = mouse::Cursor::Available(end);
    for event in [
        mouse::Event::CursorMoved { position: start },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::CursorMoved { position: end },
    ] {
        canvas.update(&mut state, &Event::Mouse(event), bounds, cursor);
    }
    canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .unwrap()
        .into_inner()
        .0
        .unwrap()
}

#[test]
fn template_drag_uses_the_target_bond_and_can_be_cancelled() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-30.0, 0.0));
    let b = doc.add_atom("C", World::new(30.0, 0.0));
    doc.add_bond(a, b, 1, "plain");
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[],
        tool: Tool::Template,
        camera: Camera {
            center: World::default(),
            zoom: 1.0,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: Some((
            &reshiki::templates::LIBRARY[0],
            reshiki::templates::Anchor::Auto,
        )),
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &GraphicStyle::default(),
        bracket_sides: BracketSides::Both,
    };
    let start = Point::new(200.0, 150.0);
    let end = Point::new(200.0, 200.0);
    let Edit::Template(anchor, direction) = pointer_gesture(&canvas, start, end) else {
        panic!("expected template placement")
    };
    assert_eq!(anchor, World::default());
    assert_eq!(direction, Some(World::new(0.0, 50.0)));
    // Snap about the actual attachment atom, including an off-center click.
    let origin = doc.atom(a).unwrap().position;
    let pressed = origin.offset(2., 1.);
    let target = origin.offset(52., 23.);
    use iced::keyboard::Modifiers;
    for modifiers in [Modifiers::SHIFT, Modifiers::CTRL] {
        let (anchor, direction) = canvas.template_gesture(pressed, target, modifiers);
        assert_eq!(anchor, pressed);
        let direction = direction.unwrap();
        let expected = BondDrawing {
            fixed_length: false,
            ..Default::default()
        }
        .endpoint(origin, target);
        assert!(direction.distance(expected) < 0.0001);
        assert!((chains::direction(origin, direction).to_degrees() - 30.).abs() < 0.0001);
        assert!(
            canvas
                .template_gesture(pressed, pressed, modifiers)
                .1
                .is_none()
        );
    }
    assert_eq!(
        canvas
            .template_gesture(pressed, target, Modifiers::empty())
            .1,
        Some(target)
    );
    assert_eq!(
        canvas
            .template_gesture(pressed, target, Modifiers::SHIFT | Modifiers::ALT)
            .1,
        Some(target)
    );
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
    for button in [mouse::Button::Left, mouse::Button::Right] {
        let mut drag = State {
            modifiers: Modifiers::CTRL,
            ..Default::default()
        };
        let cursor = mouse::Cursor::Available(Point::new(252., 173.));
        for event in [
            mouse::Event::CursorMoved {
                position: Point::new(200., 150.),
            },
            mouse::Event::ButtonPressed(button),
            mouse::Event::CursorMoved {
                position: Point::new(252., 173.),
            },
        ] {
            canvas.update(&mut drag, &Event::Mouse(event), bounds, cursor);
        }
        let result = canvas
            .update(
                &mut drag,
                &Event::Mouse(mouse::Event::ButtonReleased(button)),
                bounds,
                cursor,
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        let Edit::Template(anchor, Some(direction)) = result else {
            panic!("Expected constrained template, not pan")
        };
        assert!((chains::direction(anchor, direction).to_degrees() - 30.).abs() < 0.0001);
    }
    let mut state = State::default();
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400.0, 300.0));
    let cursor = mouse::Cursor::Available(start);
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        bounds,
        cursor,
    );
    canvas.update(
        &mut state,
        &Event::Window(iced::window::Event::Unfocused),
        bounds,
        cursor,
    );
    assert!(
        canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor
            )
            .is_none()
    );
    assert_eq!(doc.atoms.len(), 2);
}

#[test]
fn selection_handles_resize_and_rotate_without_moving_or_merging_atoms() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-20.0, -10.0));
    let b = doc.add_atom("C", World::new(20.0, 10.0));
    doc.add_bond(a, b, 1, "plain");
    let original = doc.clone();
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[a, b],
        tool: Tool::Select,
        camera: Camera {
            center: World::default(),
            zoom: 1.0,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &GraphicStyle::default(),
        bracket_sides: BracketSides::Both,
    };
    for (start, end, expected_pivot, expected_scale, expected_rotation) in [
        (
            Point::new(232.0, 172.0),
            Point::new(272.0, 192.0),
            World::new(-20.0, -10.0),
            2.0,
            0.0,
        ),
        (
            Point::new(200.0, 100.0),
            Point::new(250.0, 150.0),
            World::default(),
            1.0,
            90.0,
        ),
    ] {
        let Edit::Transform {
            ids,
            pivot,
            scale,
            rotation,
        } = pointer_gesture(&canvas, start, end)
        else {
            panic!("handle must produce a transform, not an atom move");
        };
        assert_eq!(ids, vec![a, b]);
        assert_eq!(pivot, expected_pivot);
        assert!((scale - expected_scale).abs() < 0.001);
        assert!((rotation - expected_rotation).abs() < 0.001);
    }
    assert_eq!(
        doc, original,
        "pointer previews must not mutate the document"
    );
    let mut state = State::default();
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400.0, 300.0));
    let cursor = mouse::Cursor::Available(Point::new(232.0, 172.0));
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        bounds,
        cursor,
    );
    assert!(matches!(state.gesture, Some(Gesture::Transform(_))));
    canvas.update(
        &mut state,
        &Event::Window(iced::window::Event::Unfocused),
        bounds,
        cursor,
    );
    assert!(state.gesture.is_none());
    assert!(
        canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor
            )
            .is_none()
    );
}

#[test]
fn bond_midpoints_select_and_drag_both_atoms_without_losing_atom_targets() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-21.0, 0.0));
    let b = doc.add_atom("C", World::new(21.0, 0.0));
    doc.add_bond(a, b, 1, "plain");
    assert_eq!(hit_selection(&doc, World::default(), 10.0), vec![a, b]);
    assert_eq!(hit_selection(&doc, World::new(-20.0, 0.0), 10.0), vec![a]);
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[],
        tool: Tool::Select,
        camera: Camera {
            center: World::default(),
            zoom: 1.0,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &GraphicStyle::default(),
        bracket_sides: BracketSides::Both,
    };
    assert!(
        matches!(pointer_gesture(&canvas, Point::new(200.0, 150.0), Point::new(200.0, 150.0)), Edit::Select(ids) if ids == vec![a, b])
    );
    assert!(
        matches!(pointer_gesture(&canvas, Point::new(200.0, 150.0), Point::new(230.0, 170.0)), Edit::Move(ids, 30.0, 20.0) if ids == vec![a, b])
    );
    let selected = [a, b];
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        selected: &selected,
        ..canvas
    };
    assert!(
        matches!(pointer_gesture(&canvas, Point::new(179.0, 150.0), Point::new(179.0, 150.0)), Edit::Select(ids) if ids == vec![a])
    );
    assert!(
        matches!(pointer_gesture(&canvas, Point::new(179.0, 150.0), Point::new(209.0, 170.0)), Edit::Move(ids, 30.0, 20.0) if ids == vec![a, b])
    );
}

#[test]
fn keyboard_click_preserves_world_point_and_stationary_screen_motion_does_not_handoff() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-21., 0.));
    let b = doc.add_atom("C", World::new(21., 0.));
    doc.add_bond(a, b, 1, "plain");
    let mut canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: Some(World::default()),
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[],
        tool: Tool::Select,
        camera: Camera {
            center: World::default(),
            zoom: 1.,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &GraphicStyle::default(),
        bracket_sides: BracketSides::Both,
    };
    assert!(
        matches!(pointer_gesture(&canvas, Point::new(200., 150.), Point::new(200., 150.)), Edit::SelectAt(ids, point) if ids == vec![a, b] && point == World::default())
    );
    let blank_screen = Point::new(50., 50.);
    assert!(
        matches!(pointer_gesture(&canvas, blank_screen, blank_screen), Edit::SelectAt(ids, point) if ids.is_empty() && point == World::new(-150., -100.))
    );
    let mut state = State::default();
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
    let screen = Point::new(200., 150.);
    let cursor = mouse::Cursor::Available(screen);
    let motion = Event::Mouse(mouse::Event::CursorMoved { position: screen });
    let first = canvas
        .update(&mut state, &motion, bounds, cursor)
        .unwrap()
        .into_inner()
        .0;
    assert!(matches!(first, Some(Edit::Hover(Some(point))) if point == World::default()));
    canvas.camera.center = World::new(100., 100.);
    let stationary = canvas
        .update(&mut state, &motion, bounds, cursor)
        .unwrap()
        .into_inner()
        .0;
    assert!(
        stationary.is_none(),
        "A camera change at the same physical pointer must not publish a new hotspot"
    );
    let next = Point::new(201., 150.);
    let moved = canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved { position: next }),
            bounds,
            mouse::Cursor::Available(next),
        )
        .unwrap()
        .into_inner()
        .0;
    assert!(matches!(moved, Some(Edit::Hover(Some(point))) if point == World::new(101., 100.)));
}

#[test]
fn command_drag_duplicates_and_shift_drag_locks_to_one_axis() {
    use iced::keyboard::Modifiers;
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-21.0, 0.0));
    let b = doc.add_atom("C", World::new(21.0, 0.0));
    let c = doc.add_atom("C", World::new(21.0, 42.0));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    let selected = [a, b, c];
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &selected,
        tool: Tool::Select,
        camera: Camera::default(),
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &GraphicStyle::default(),
        bracket_sides: BracketSides::Both,
    };
    let (start, end) = (Point::new(200.0, 150.0), Point::new(290.0, 170.0));
    let gesture = |modifiers| pointer_gesture_with(&canvas, start, end, modifiers);
    assert!(
        matches!(gesture(Modifiers::COMMAND), Edit::Duplicate(ids, 90.0, 20.0) if ids == selected)
    );
    assert!(matches!(gesture(Modifiers::SHIFT), Edit::Move(ids, 90.0, 0.0) if ids == selected));
    assert!(
        matches!(gesture(Modifiers::COMMAND | Modifiers::SHIFT), Edit::Duplicate(ids, 90.0, 0.0) if ids == selected)
    );
    let vertical = pointer_gesture_with(&canvas, start, Point::new(215.0, 90.0), Modifiers::SHIFT);
    assert!(matches!(vertical, Edit::Move(ids, 0.0, -60.0) if ids == selected));
    // Duplicating a bonded part of a molecule copies it free of the rest.
    let partial = [b, c];
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        selected: &partial,
        ..canvas
    };
    assert!(matches!(
        pointer_gesture_with(&canvas, Point::new(221.0, 171.0), Point::new(251.0, 201.0), Modifiers::COMMAND),
        Edit::Duplicate(ids, 30.0, 30.0) if ids == partial
    ));
    // Shift+Option/Alt moves a part still bonded to the rest freely along the axis.
    assert!(matches!(
        pointer_gesture_with(&canvas, Point::new(221.0, 171.0), Point::new(251.0, 181.0), Modifiers::SHIFT | Modifiers::ALT),
        Edit::Move(ids, 30.0, 0.0) if ids.len() == 2 && partial.iter().all(|id| ids.contains(id))
    ));
}

#[test]
fn command_drag_in_point_edit_mode_moves_the_point() -> Result<(), String> {
    let mut doc = Document::default();
    let metal = doc.add_atom("Fe", World::default());
    let (doc, _) = reshiki::hotkeys::atom_edit(&doc, metal, "j", 42.).ok_or("Shortcut")??;
    let anchor = doc
        .atoms
        .iter()
        .find(|a| a.attachment.is_some())
        .ok_or("Point")?;
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::EditPoints;
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    let start = canvas.camera.screen(anchor.position, bounds);
    let modifiers = iced::keyboard::Modifiers::COMMAND;
    let edit = pointer_gesture_with(&canvas, start, start + Vector::new(35., 25.), modifiers);
    assert!(
        matches!(&edit, Edit::Move(ids, ..) if *ids == vec![anchor.id]),
        "{edit:?}"
    );
    Ok(())
}

#[test]
fn ending_a_command_drag_releases_its_copy() {
    use iced::keyboard::{Event as Key, Location, Modifiers, key};
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-21.0, 0.0));
    let b = doc.add_atom("O", World::new(21.0, 0.0));
    doc.add_bond(a, b, 1, "plain");
    let selected = [a, b];
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    canvas.selected = &selected;
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    let start = canvas.camera.screen(World::new(-21.0, 0.0), bounds);
    let end = start + Vector::new(60., 30.);
    let cursor = mouse::Cursor::Available(end);
    let escape = key::Key::Named(key::Named::Escape);
    for end_drag in [
        Event::Keyboard(Key::KeyPressed {
            key: escape.clone(),
            modified_key: escape.clone(),
            physical_key: key::Physical::Code(key::Code::Escape),
            location: Location::Standard,
            modifiers: Modifiers::COMMAND,
            text: None,
            repeat: false,
        }),
        Event::Window(iced::window::Event::Unfocused),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ] {
        let mut state = State {
            modifiers: Modifiers::COMMAND,
            ..Default::default()
        };
        for event in [
            mouse::Event::CursorMoved { position: start },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::CursorMoved { position: end },
        ] {
            canvas.update(&mut state, &Event::Mouse(event), bounds, cursor);
        }
        assert!(matches!(state.gesture, Some(Gesture::Move { .. })));
        // The drag preview extracts the copy, as drawing a frame does.
        let copy = state.scene.borrow_mut().copy(&doc, &selected);
        assert_eq!(copy.atoms.len(), 2);
        canvas.update(&mut state, &end_drag, bounds, cursor);
        assert!(state.gesture.is_none(), "{end_drag:?}");
        assert_eq!(std::rc::Rc::strong_count(&copy), 1, "{end_drag:?}");
    }
}

#[test]
fn ring_drag_snaps_at_release_or_keeps_its_initial_attachment() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-21.0, 0.0));
    let b = doc.add_atom("C", World::new(21.0, 0.0));
    doc.add_bond(a, b, 1, "plain");
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[],
        tool: Tool::Ring,
        camera: Camera {
            center: World::default(),
            zoom: 1.0,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 5,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &GraphicStyle::default(),
        bracket_sides: BracketSides::Both,
    };
    assert!(
        matches!(pointer_gesture(&canvas, Point::new(100.0, 50.0), Point::new(200.0, 150.0)), Edit::Ring(anchor, None) if anchor == World::default())
    );
    assert!(
        matches!(pointer_gesture(&canvas, Point::new(200.0, 150.0), Point::new(200.0, 100.0)), Edit::Ring(anchor, Some(side)) if anchor == World::default() && side == World::new(0.0, -50.0))
    );
}

#[test]
fn leaving_the_canvas_requests_a_redraw_and_leaving_the_window_clears_hover() {
    let doc = Document::default();
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[],
        tool: Tool::Bond(1),
        camera: Camera::default(),
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &GraphicStyle::default(),
        bracket_sides: BracketSides::Both,
    };
    let bounds = Rectangle::new(Point::new(100.0, 100.0), iced::Size::new(400.0, 300.0));
    let mut state = State {
        cursor: Some(Point::new(200.0, 150.0)),
        ..Default::default()
    };
    assert!(
        canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(20.0, 20.0)
                }),
                bounds,
                mouse::Cursor::Unavailable
            )
            .is_some()
    );
    canvas.update(
        &mut state,
        &Event::Mouse(mouse::Event::CursorLeft),
        bounds,
        mouse::Cursor::Unavailable,
    );
    assert!(state.cursor.is_none());
}

#[test]
fn short_endpoint_drag_grows_instead_of_snapping_to_its_source() {
    let mut doc = Document::default();
    let source = doc.add_atom("C", World::default());
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[],
        tool: Tool::Bond(1),
        camera: Camera {
            center: World::default(),
            zoom: 1.0,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &GraphicStyle::default(),
        bracket_sides: BracketSides::Both,
    };
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400.0, 300.0));
    let mut state = State::default();
    let end = Point::new(206.0, 150.0);
    let cursor = mouse::Cursor::Available(end);
    for event in [
        mouse::Event::CursorMoved {
            position: Point::new(200.0, 150.0),
        },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::CursorMoved { position: end },
    ] {
        canvas.update(&mut state, &Event::Mouse(event), bounds, cursor);
    }
    let action = canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .unwrap();
    let Some(Edit::Bond(start, end, Some(id), None)) = action.into_inner().0 else {
        panic!("short drag must extend, not connect the source to itself");
    };
    assert_eq!(id, source);
    assert_eq!(start, World::default());
    assert_eq!(end, World::new(42.0, 0.0));
}

#[test]
fn drag_reuses_atoms_at_the_cursor_or_the_snapped_endpoint() {
    let mut doc = Document::default();
    let source = doc.add_atom("C", World::default());
    let target = doc.add_atom("C", World::new(42.0, 0.0));
    for cursor in [World::new(42.0, 3.0), World::new(80.0, 0.0)] {
        let (end, id) = bond_target(&doc, World::default(), cursor, Some(source), 12.0);
        assert_eq!(id, Some(target));
        assert_eq!(end, World::new(42.0, 0.0));
    }
}

#[test]
fn fast_drag_uses_each_motion_event_instead_of_final_cursor_snapshot() {
    let doc = Document::default();
    let canvas = MoleculeCanvas {
        optimizer: None,
        keyboard_target: None,
        element: "C",
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc: &doc,
        selected: &[],
        tool: Tool::Bond(1),
        camera: Camera {
            center: World::default(),
            zoom: 1.0,
        },
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: reshiki::templates::Connection::Auto,
        template: None,
        arrow_preset: Default::default(),
        arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: &GraphicStyle::default(),
        bracket_sides: BracketSides::Both,
    };
    let mut state = State::default();
    let bounds = Rectangle {
        x: 200.0,
        y: 100.0,
        width: 400.0,
        height: 300.0,
    };
    let origin = Point::new(350.0, 250.0);
    let end = Point::new(410.0, 250.0);
    let snapshot = mouse::Cursor::Available(end);
    for event in [
        mouse::Event::CursorMoved { position: origin },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::CursorMoved { position: end },
    ] {
        canvas.update(&mut state, &Event::Mouse(event), bounds, snapshot);
    }
    let action = canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            snapshot,
        )
        .unwrap();
    let Some(Edit::Bond(start, end, None, None)) = action.into_inner().0 else {
        panic!("drag must produce a bond, not a click");
    };
    assert_eq!(start, World::new(-50.0, 0.0));
    assert_eq!(end, World::new(-8.0, 0.0));
}

/// A drag's published edits, and its preview offset and guides just
/// before release, as `draw_paper` computes them.
pub(super) fn guided_drag(
    canvas: &MoleculeCanvas<'_>,
    from: World,
    to: World,
    modifiers: iced::keyboard::Modifiers,
) -> (Vec<Edit>, Option<(World, Vec<Guide>)>) {
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    let (start, end) = (
        canvas.camera.screen(from, bounds),
        canvas.camera.screen(to, bounds),
    );
    let cursor = mouse::Cursor::Available(end);
    let mut state = State {
        modifiers,
        ..Default::default()
    };
    let mut edits = Vec::new();
    let mut send = |state: &mut State, event| {
        let action = canvas.update(state, &Event::Mouse(event), bounds, cursor);
        edits.extend(action.and_then(|a| a.into_inner().0));
    };
    send(&mut state, mouse::Event::CursorMoved { position: start });
    send(&mut state, mouse::Event::ButtonPressed(mouse::Button::Left));
    send(&mut state, mouse::Event::CursorMoved { position: end });
    let p = canvas.camera.world(end, bounds);
    let preview = match &state.gesture {
        Some(Gesture::Move { start, ids, .. }) => Some(canvas.move_delta(
            &state,
            ids,
            World::new(p.x - start.x, p.y - start.y),
            bounds,
        )),
        Some(Gesture::ArrowHandle { id, index }) => canvas
            .doc
            .arrows
            .iter()
            .find(|a| a.id == *id)
            .map(|a| canvas.arrow_end(&state, a, *index, p, bounds)),
        _ => None,
    };
    send(
        &mut state,
        mouse::Event::ButtonReleased(mouse::Button::Left),
    );
    (edits, preview)
}

fn rectangle(doc: &mut Document, lo: World, hi: World) -> u64 {
    let id = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        lo,
        hi,
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ));
    id
}

#[test]
fn smart_guides_snap_whole_objects_and_the_preview_matches_the_release() {
    use iced::keyboard::Modifiers;
    let mut doc = Document::default();
    rectangle(&mut doc, World::new(-150., -100.), World::new(-90., -60.));
    let moving = rectangle(&mut doc, World::new(0., 0.), World::new(60., 40.));
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    // Drag by (-3, -97): the top edges end 3 px apart and snap together.
    let (from, to) = (World::new(0., 20.), World::new(-3., -77.));
    let released = |canvas: &MoleculeCanvas<'_>, modifiers| {
        let (edits, preview) = guided_drag(canvas, from, to, modifiers);
        let last = edits.last().cloned();
        let (d, guides) = preview.expect("A move preview");
        let (ids, x, y) = match last {
            Some(Edit::Move(ids, x, y) | Edit::Duplicate(ids, x, y)) => (ids, x, y),
            other => panic!("{other:?}"),
        };
        assert_eq!(ids, [moving]);
        assert_eq!((d.x, d.y), (x, y), "the preview agrees with the release");
        (World::new(x, y), guides)
    };
    let (snapped, guides) = released(&canvas, Modifiers::empty());
    assert_eq!(snapped, World::new(-3., -100.));
    // Same-size objects: one center guide spans both.
    assert!(
        guides
            .iter()
            .any(|g| matches!(g, Guide::Align { axis: Axis::Y, .. })),
        "{guides:?}"
    );
    // Shift keeps the vertical axis and still snaps along it; a copy snaps too.
    assert_eq!(released(&canvas, Modifiers::SHIFT).0, World::new(0., -100.));
    let copy = guided_drag(&canvas, from, to, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(matches!(copy.0.last(), Some(Edit::Duplicate(_, x, y)) if (*x, *y) == (0., -100.)));
    // Option/Alt, or the setting off, moves freely without guides.
    let (free, guides) = released(&canvas, Modifiers::ALT);
    assert_eq!((free, guides), (World::new(-3., -97.), vec![]));
    canvas.smart_guides = false;
    let (free, guides) = released(&canvas, Modifiers::empty());
    assert_eq!((free, guides), (World::new(-3., -97.), vec![]));
}

#[test]
fn smart_guides_leave_part_of_a_molecule_to_its_bond_constraints() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(0., 0.));
    let b = doc.add_atom("C", World::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    rectangle(&mut doc, World::new(-40., -120.), World::new(0., -80.));
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    let selected = [b];
    canvas.selected = &selected;
    let drag = |canvas: &MoleculeCanvas<'_>| {
        guided_drag(
            canvas,
            World::new(42., 0.),
            World::new(3., -80.),
            Default::default(),
        )
    };
    let (guided, preview) = drag(&canvas);
    canvas.smart_guides = false;
    assert_eq!(
        format!("{:?}", guided.last()),
        format!("{:?}", drag(&canvas).0.last())
    );
    assert!(preview.is_some_and(|(_, guides)| guides.is_empty()));
}

#[test]
fn arrow_ends_snap_along_their_fixed_angle_to_other_objects() {
    let mut doc = Document::default();
    let target = rectangle(&mut doc, World::new(100., -30.), World::new(160., 30.));
    let arrow = doc.next_id();
    doc.arrows.push(reshiki::document::Arrow::new(
        arrow,
        World::new(0., 0.),
        World::new(80., 0.),
        Default::default(),
        Default::default(),
    ));
    let (lo, _) = reshiki::scene::selection_bounds(&doc, &[target]).expect("bounds");
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    let selected = [arrow];
    canvas.selected = &selected;
    let to = World::new(lo.x - 4., 2.);
    let (edits, preview) = guided_drag(&canvas, World::new(80., 0.), to, Default::default());
    let Some(Edit::ArrowHandle(id, 1, end)) = edits.last().cloned() else {
        panic!("{edits:?}");
    };
    assert_eq!(id, arrow);
    assert!((end.x - lo.x).abs() < 1e-3 && end.y.abs() < 1e-3, "{end:?}");
    let (preview, guides) = preview.expect("A handle preview");
    assert_eq!(preview, end);
    // The target's left edge, and its middle line, which the arrow already sits on.
    assert!(
        matches!(
            guides.as_slice(),
            [
                Guide::Align { axis: Axis::X, at: x, .. },
                Guide::Align { axis: Axis::Y, at: y, .. },
            ] if *x == end.x && *y == 0.
        ),
        "{guides:?}"
    );
    // Option/Alt frees the end from both the angle steps and the guides.
    let (edits, _) = guided_drag(
        &canvas,
        World::new(80., 0.),
        to,
        iced::keyboard::Modifiers::ALT,
    );
    assert!(matches!(edits.last(), Some(Edit::ArrowHandle(_, 1, end)) if *end == to));
}

#[test]
fn grid_dots_sit_on_bond_length_steps_from_the_origin() {
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    let at = |zoom| Camera {
        center: World::new(13., -7.),
        zoom,
    };
    let on = |v: f32, step: f32| (v / step - (v / step).round()).abs() < 1e-3;
    // Zoom 1: whole 42-unit steps, with half steps 21 px apart between them.
    let dots = grid_dots(at(1.), bounds, 42.);
    for (p, major) in &dots {
        let w = at(1.).world(*p, bounds);
        assert!(on(w.x, 21.) && on(w.y, 21.), "{w:?}");
        assert_eq!(*major, on(w.x, 42.) && on(w.y, 42.), "{w:?}");
    }
    assert_eq!(dots.len(), 19 * 14);
    assert!(dots.iter().any(|(_, major)| !major));
    // Half steps under 12 px apart are dropped; steps under 10 px hide the grid.
    let zoomed_out = grid_dots(at(0.5), bounds, 42.);
    assert!(!zoomed_out.is_empty() && zoomed_out.iter().all(|(_, major)| *major));
    assert!(grid_dots(at(0.2), bounds, 42.).is_empty());
}

#[test]
fn large_grids_bound_dot_count_without_moving_the_world_origin() {
    for size in [iced::Size::new(3840., 2160.), iced::Size::new(7680., 4320.)] {
        let bounds = Rectangle::with_size(size);
        for step in [10., 24., 42.] {
            let camera = Camera {
                center: World::new(13., -7.),
                zoom: 1.,
            };
            let dots = grid_dots(camera, bounds, step);
            assert!(!dots.is_empty() && dots.len() <= 8192);
            for (point, major) in dots {
                let world = camera.world(point, bounds);
                assert!(major);
                for value in [world.x, world.y] {
                    assert!((value / step - (value / step).round()).abs() < 1e-3);
                }
            }
        }
    }
}

#[test]
fn readable_foreground_orbital_labels_select_the_atom_and_other_filled_graphics_keep_precedence() {
    use reshiki::{
        graphics::GraphicKind, palette::Color, scene::Primitive, scientific::OrbitalKind,
    };
    let mut doc = Document::default();
    let atom = doc.add_atom("O", World::default());
    doc.atom_mut(atom).unwrap().label_h = 1;
    let probes: Vec<_> = reshiki::scene::primitives(&doc)
        .into_iter()
        .flat_map(|primitive| {
            if let Primitive::Text {
                position,
                text,
                size,
                style,
                ..
            } = primitive
            {
                reshiki::style::text_ink_boxes(&text, size, &style)
                    .into_iter()
                    .map(|(lo, hi)| position.offset((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5))
                    .collect()
            } else {
                Vec::new()
            }
        })
        .collect();
    assert!(probes.len() >= 2, "Probe both O and its H label");
    let orbital = doc.next_id();
    let mut graphic = Graphic::dragged(
        orbital,
        GraphicKind::Orbital(OrbitalKind::S),
        World::default(),
        World::new(0., -42.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    graphic.layer = 1;
    assert!(
        probes.iter().all(|&p| graphic.hit(p, 1.)),
        "Original filled orbital geometry covers the labels"
    );
    doc.graphics.push(graphic);
    let before = doc.clone();
    for &probe in &probes {
        assert_eq!(hit::hit_object(&doc, probe, 1.), Some(atom));
    }
    assert_eq!(
        hit::hit_object(&doc, World::new(-30., 0.), 1.),
        Some(orbital)
    );
    assert_eq!(
        doc, before,
        "Hit testing cannot change chemistry or geometry"
    );
    let circle = doc.next_id();
    let mut ordinary = Graphic::dragged(
        circle,
        GraphicKind::Ellipse,
        World::new(-42., -42.),
        World::new(42., 42.),
        GraphicStyle {
            fill: Some(Color::Ink),
            ..Default::default()
        },
        BracketSides::Both,
        false,
    );
    ordinary.layer = 2;
    doc.graphics.push(ordinary);
    for probe in probes {
        assert_eq!(hit::hit_object(&doc, probe, 1.), Some(circle));
    }
}
