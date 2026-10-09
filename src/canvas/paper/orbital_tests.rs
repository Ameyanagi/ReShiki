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
                canvas.snap_orbitals = enabled;
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
                    style: canvas.graphic_style.clone(),
                    phase: canvas.orbital_phase,
                    flipped: canvas.phase_flipped,
                    attach: canvas.attach_symbols,
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
        for end in [World::default(), World::new(8., -15.)] {
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
                style: canvas.graphic_style.clone(),
                phase: canvas.orbital_phase,
                flipped: canvas.phase_flipped,
                attach: true,
                snap_orbitals: true,
            };
            let mut committed = doc.clone();
            drawing
                .place(&mut committed, start, end, constrain, 10. / zoom)
                .unwrap();
            assert_eq!(committed, edited);
            assert_eq!(committed.atom(id).unwrap().label_h, 1);
        }
    }
}
