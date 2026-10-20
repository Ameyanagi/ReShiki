use super::*;
use crate::canvas::{Edit, Gesture, Tool};
use iced::keyboard::Modifiers;
use reshiki::{graphics::GraphicKind, scientific::OrbitalKind};

#[test]
fn orbital_preview_commit_and_temporary_bypass_agree_at_every_zoom() {
    let mut doc = Document::default();
    let id = doc.add_atom("N", World::default());
    let bounds = Rectangle::with_size(iced::Size::new(600., 400.));
    for zoom in [0.5, 1., 2.] {
        for enabled in [false, true] {
            for modifiers in [
                Modifiers::empty(),
                Modifiers::ALT,
                Modifiers::SHIFT,
                Modifiers::ALT | Modifiers::SHIFT,
            ] {
                let mut canvas =
                    crate::canvas::tests::chain_canvas(&doc, reshiki::chains::ChainMode::Straight);
                canvas.tool = Tool::Graphic(GraphicKind::Orbital(OrbitalKind::P));
                canvas.camera.zoom = zoom;
                canvas.graphics.snap = enabled;
                let start = World::new(4. / zoom, 0.);
                let end = start.offset(7., -42.);
                let mut state = State {
                    gesture: Some(Gesture::Graphic { start }),
                    cursor: Some(canvas.camera.screen(end, bounds)),
                    modifiers,
                    ..Default::default()
                };
                let mut preview = Draft::new(&doc);
                canvas.preview_pointer_edits(&mut preview, &state, bounds);
                let edited = preview.preview.into_owned();
                assert_eq!(edited.atoms, doc.atoms);
                let snap = enabled && !modifiers.alt();
                assert_eq!(
                    edited.graphics[0].origin,
                    if snap {
                        doc.atom(id).unwrap().position
                    } else {
                        start
                    }
                );
                assert!(preview.template_notice.unwrap().0.contains(if snap {
                    "snapped"
                } else {
                    "Free"
                }));
                let action = canvas
                    .handle_event(
                        &mut state,
                        &iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                            iced::mouse::Button::Left,
                        )),
                        bounds,
                        iced::mouse::Cursor::Unavailable,
                    )
                    .unwrap();
                let edit = action.into_inner().0.unwrap();
                let Edit::Orbital(actual_start, actual_end, constrain, actual_snap) = edit else {
                    panic!("Expected orbital placement event")
                };
                assert_eq!(actual_snap, snap);
                let drawing = reshiki::scientific::Drawing {
                    kind: GraphicKind::Orbital(OrbitalKind::P),
                    style: canvas.graphics.style.clone(),
                    phase: canvas.graphics.phase,
                    flipped: canvas.graphics.flipped,
                    attach: canvas.graphics.attach,
                    snap_orbitals: actual_snap,
                };
                let mut committed = doc.clone();
                drawing
                    .place(
                        &mut committed,
                        actual_start,
                        actual_end,
                        constrain,
                        10. / zoom,
                    )
                    .unwrap();
                assert_eq!(
                    committed, edited,
                    "Preview and release must use identical node, axis, phase and angle snapping"
                );
            }
        }
    }
}

#[test]
fn lone_pair_click_and_drag_previews_match_the_exact_committed_marks() {
    let mut doc = Document::default();
    let id = doc.add_atom("O", World::default());
    doc.atom_mut(id).unwrap().label_h = 1;
    let bounds = Rectangle::with_size(iced::Size::new(600., 400.));
    for zoom in [0.5, 1., 2.] {
        for (end, modifiers) in [
            (World::default(), Modifiers::empty()),
            (World::new(8., -15.), Modifiers::empty()),
            (World::new(8., -15.), Modifiers::ALT),
            (World::new(8., -15.), Modifiers::SHIFT),
        ] {
            let mut canvas =
                crate::canvas::tests::chain_canvas(&doc, reshiki::chains::ChainMode::Straight);
            let kind = GraphicKind::Symbol(reshiki::scientific::SymbolKind::LonePair);
            canvas.tool = Tool::Graphic(kind);
            canvas.camera.zoom = zoom;
            let mut state = State {
                gesture: Some(Gesture::Graphic {
                    start: World::default(),
                }),
                cursor: Some(canvas.camera.screen(end, bounds)),
                modifiers,
                ..Default::default()
            };
            let mut preview = Draft::new(&doc);
            canvas.preview_pointer_edits(&mut preview, &state, bounds);
            let edited = preview.preview.into_owned();
            let edit = canvas
                .handle_event(
                    &mut state,
                    &iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                        iced::mouse::Button::Left,
                    )),
                    bounds,
                    iced::mouse::Cursor::Unavailable,
                )
                .unwrap()
                .into_inner()
                .0
                .unwrap();
            let Edit::Graphic(start, end, constrain) = edit else {
                panic!("Expected symbol placement event")
            };
            let drawing = reshiki::scientific::Drawing {
                kind,
                style: canvas.graphics.style.clone(),
                phase: canvas.graphics.phase,
                flipped: canvas.graphics.flipped,
                attach: true,
                snap_orbitals: true,
            };
            let mut committed = doc.clone();
            drawing
                .place(&mut committed, start, end, constrain, 10. / zoom)
                .unwrap();
            assert_eq!(committed, edited);
            assert_eq!(committed.atom(id).unwrap().label_h, 1);
            // A plain drag orbits in 15° steps; Option/Alt or Shift keeps the offset.
            let mark = &committed.atom(id).unwrap().marks[0];
            if end != World::default() {
                assert_eq!(constrain, !modifiers.is_empty());
                let direction = mark.offset.y.atan2(mark.offset.x).to_degrees();
                if modifiers.is_empty() {
                    assert!((direction + 60.).abs() < 1e-3, "{direction}");
                } else {
                    assert_eq!(mark.offset, end);
                }
                let turn = (mark.angle - direction - 90.).rem_euclid(360.);
                assert!(turn.min(360. - turn) < 1e-3, "{}", mark.angle);
            }
        }
    }
}

/// Methanol's O (as OH) with a lone pair above it, as a drag's preview has it.
fn methanol_pair() -> (Document, u64) {
    let mut doc = Document::default();
    let c = doc.add_atom("C", World::new(-42., 0.));
    let o = doc.add_atom("O", World::default());
    doc.add_bond(c, o, 1, "plain");
    doc.atom_mut(o).unwrap().label_h = 1;
    let atom = doc.atom_mut(o).unwrap();
    reshiki::scientific::attach(
        atom,
        reshiki::scientific::SymbolKind::LonePair,
        World::new(0., -20.),
    )
    .unwrap();
    (doc, o)
}

#[test]
fn a_mark_drag_builds_its_orbit_guide_once_and_rebuilds_it_on_real_changes() {
    let (mut doc, o) = methanol_pair();
    let cache = std::cell::RefCell::default();
    let guide = |doc: &Document| {
        let atom = doc.atom(o).unwrap();
        overlays::orbit_guide(&cache, doc, atom, &atom.marks[0])
    };
    let first = guide(&doc);
    // Moving the pointer turns and moves the pair but keeps the guide.
    for to in [World::new(30., -14.), World::new(-5., 40.)] {
        let atom = doc.atom(o).unwrap();
        let placed = reshiki::scientific::orbit_mark(&doc, atom, &atom.marks[0], to, false);
        doc.atom_mut(o).unwrap().marks[0] = placed;
        let atom = doc.atom(o).unwrap();
        let again = guide(&doc);
        assert!(std::rc::Rc::ptr_eq(&first, &again));
        // The kept guide is the one the committed placements give.
        assert_eq!(
            *again,
            reshiki::scientific::mark_orbit_guide(&doc, atom, &atom.marks[0])
        );
    }
    // Its label, bonds, size or style changing rebuilds it.
    let edits: [&dyn Fn(&mut Document); 4] = [
        &|d| d.atom_mut(o).unwrap().label_h = 0,
        &|d| d.atoms[0].position.y += 10.,
        &|d| d.atom_mut(o).unwrap().marks[0].size_pt = Some(20.),
        &|d| d.drawing_style.font_size_pt += 2.,
    ];
    for edit in edits {
        let before = guide(&doc);
        edit(&mut doc);
        let after = guide(&doc);
        assert!(!std::rc::Rc::ptr_eq(&before, &after));
        let atom = doc.atom(o).unwrap();
        assert_eq!(
            *after,
            reshiki::scientific::mark_orbit_guide(&doc, atom, &atom.marks[0])
        );
    }
}
