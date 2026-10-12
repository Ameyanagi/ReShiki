use super::*;
use crate::canvas::{ArrowOptions, Edit, GraphicOptions};
use iced::widget::canvas::Program;
use iced::{Event, Size, mouse};
use reshiki::{document::Document, editing::reference::Stretch};

fn canvas<'a>(
    doc: &'a Document,
    arrow: &'a reshiki::arrows::ArrowStyle,
    graphic: &'a reshiki::graphics::GraphicStyle,
) -> MoleculeCanvas<'a> {
    let base = crate::canvas::tests::chain_canvas(doc, crate::canvas::ChainMode::Straight);
    MoleculeCanvas {
        tool: Tool::StretchBond {
            fixed: 1,
            moving: 2,
        },
        template_connection: Default::default(),
        arrows: ArrowOptions {
            style: arrow,
            ..base.arrows
        },
        graphics: GraphicOptions {
            style: graphic,
            ..base.graphics
        },
        ..base
    }
}
fn fixture() -> Document {
    let mut doc = Document::default();
    let (s, c) = 17.3_f32.to_radians().sin_cos();
    doc.add_atom("C", World::default());
    doc.add_atom("C", World::new(42. * c, 42. * s));
    doc.add_atom("O", World::new(42. * c + 21., 42. * s - 36.373_066));
    doc.add_bond(1, 2, 1, "plain");
    doc.add_bond(2, 3, 1, "plain");
    doc
}
fn event(
    canvas: &MoleculeCanvas<'_>,
    state: &mut State,
    event: Event,
    p: Point,
    bounds: Rectangle,
) -> Option<Edit> {
    canvas
        .update(state, &event, bounds, mouse::Cursor::Available(p))
        .and_then(|a| a.into_inner().0)
}

#[test]
fn stretch_plan_cache_reuses_matching_drawings_and_refreshes_changed_content() {
    let mut doc = fixture();
    let mut state = State::default();
    let first = state.scene.get_mut().stretch(&doc, 1, 2).unwrap();
    let repeated = state.scene.get_mut().stretch(&doc.clone(), 1, 2).unwrap();
    assert!(std::rc::Rc::ptr_eq(&first, &repeated));
    let swapped = state.scene.get_mut().stretch(&doc, 2, 1).unwrap();
    assert_eq!((swapped.fixed, swapped.moving), (2, 1));
    assert!(!std::rc::Rc::ptr_eq(&first, &swapped));
    let previous = state.scene.get_mut().stretch(&doc, 1, 2).unwrap();
    doc.translate(&[2, 3], 7., 9.);
    let refreshed = state.scene.get_mut().stretch(&doc, 1, 2).unwrap();
    assert!(!std::rc::Rc::ptr_eq(&previous, &refreshed));
    assert_eq!(
        refreshed.translated(&doc, 63.).unwrap(),
        Stretch::commit(&doc, 1, 2, 63.).unwrap()
    );
    let before_ring = doc.clone();
    doc.add_bond(3, 1, 1, "plain");
    assert!(state.scene.get_mut().stretch(&doc, 1, 2).is_none());
    assert!(state.scene.get_mut().stretch(&doc, 1, 2).is_none());
    let restored = state.scene.get_mut().stretch(&before_ring, 1, 2).unwrap();
    assert!(!std::rc::Rc::ptr_eq(&refreshed, &restored));
    assert_eq!(
        restored.translated(&before_ring, 63.).unwrap(),
        Stretch::commit(&before_ring, 1, 2, 63.).unwrap()
    );
}

#[test]
fn stretch_preview_and_commit_have_identical_atom_positions_for_a_sample_drag() {
    let doc = fixture();
    let arrow = Default::default();
    let graphic = Default::default();
    let bounds = Rectangle::new(Point::new(20., 50.), Size::new(500., 350.));
    let mut canvas = canvas(&doc, &arrow, &graphic);
    canvas.camera.zoom = 1.75;
    let start = doc.atom(2).unwrap().position;
    let p = canvas.camera.screen(start, bounds) + iced::Vector::new(bounds.x, bounds.y);
    let mut state = State::default();
    event(
        &canvas,
        &mut state,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        p,
        bounds,
    );
    assert!(matches!(state.gesture, Some(Gesture::StretchBond { .. })));
    let end = canvas.camera.screen(start.offset(19.25, -6.5), bounds)
        + iced::Vector::new(bounds.x, bounds.y);
    event(
        &canvas,
        &mut state,
        Event::Mouse(mouse::Event::CursorMoved { position: end }),
        end,
        bounds,
    );
    let mut draft = Draft::new(&doc);
    canvas.preview_move(&mut draft, &state, bounds);
    let preview_positions: Vec<_> = draft
        .preview
        .atoms
        .iter()
        .map(|atom| (atom.id, atom.position))
        .collect();
    let edit = event(
        &canvas,
        &mut state,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        end,
        bounds,
    )
    .unwrap();
    let Edit::StretchBond {
        fixed,
        moving,
        length,
    } = edit
    else {
        panic!("Expected one stretch edit");
    };
    let committed = Stretch::commit(&doc, fixed, moving, length).unwrap();
    let committed_positions: Vec<_> = committed
        .atoms
        .iter()
        .map(|atom| (atom.id, atom.position))
        .collect();
    assert_eq!(preview_positions, committed_positions);
    assert_eq!(committed.atom(1), doc.atom(1));
    assert_ne!(committed.atom(2).unwrap().position, start);
}

#[test]
fn stretch_preview_and_commit_agree_after_an_undo_during_the_drag() {
    let original = fixture();
    let stretched = Stretch::commit(&original, 1, 2, 63.).unwrap();
    let arrow = Default::default();
    let graphic = Default::default();
    let bounds = Rectangle::new(Point::new(20., 50.), Size::new(500., 350.));
    let pressed = canvas(&stretched, &arrow, &graphic);
    let start = stretched.atom(2).unwrap().position;
    let p = pressed.camera.screen(start, bounds) + iced::Vector::new(bounds.x, bounds.y);
    let mut state = State::default();
    event(
        &pressed,
        &mut state,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        p,
        bounds,
    );
    assert!(matches!(state.gesture, Some(Gesture::StretchBond { .. })));
    // Undo restores the 42-unit bond while the mouse button is still held.
    let undone = canvas(&original, &arrow, &graphic);
    let (s, c) = 17.3_f32.to_radians().sin_cos();
    let end = undone.camera.screen(start.offset(21. * c, 21. * s), bounds)
        + iced::Vector::new(bounds.x, bounds.y);
    event(
        &undone,
        &mut state,
        Event::Mouse(mouse::Event::CursorMoved { position: end }),
        end,
        bounds,
    );
    let mut draft = Draft::new(&original);
    undone.preview_move(&mut draft, &state, bounds);
    let preview = draft.preview.into_owned();
    let Some(Edit::StretchBond {
        fixed,
        moving,
        length,
    }) = event(
        &undone,
        &mut state,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        end,
        bounds,
    )
    else {
        panic!("Expected one stretch edit");
    };
    assert!((length - 84.).abs() < 0.001, "{length}");
    let committed = Stretch::commit(&original, fixed, moving, length).unwrap();
    assert_eq!(preview, committed);
    let (a, b) = (committed.atom(1).unwrap(), committed.atom(2).unwrap());
    assert!((a.position.distance(b.position) - 84.).abs() < 0.001);
}

#[test]
fn stretch_gesture_preview_and_release_agree_at_multiple_zooms_without_merging() {
    let doc = fixture();
    let original = doc.clone();
    let arrow = Default::default();
    let graphic = Default::default();
    let bounds = Rectangle::new(Point::new(20., 50.), Size::new(500., 350.));
    for zoom in [0.5, 1., 3.] {
        let mut canvas = canvas(&doc, &arrow, &graphic);
        canvas.camera.zoom = zoom;
        let a = doc.atom(2).unwrap().position;
        let p = canvas.camera.screen(a, bounds) + iced::Vector::new(bounds.x, bounds.y);
        let mut state = State::default();
        event(
            &canvas,
            &mut state,
            Event::Mouse(mouse::Event::CursorMoved { position: p }),
            p,
            bounds,
        );
        event(
            &canvas,
            &mut state,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            p,
            bounds,
        );
        assert!(matches!(state.gesture, Some(Gesture::StretchBond { .. })));
        let (s, c) = 17.3_f32.to_radians().sin_cos();
        let world = a.offset(21. * c - 35. * s, 21. * s + 35. * c);
        let end = canvas.camera.screen(world, bounds) + iced::Vector::new(bounds.x, bounds.y);
        event(
            &canvas,
            &mut state,
            Event::Mouse(mouse::Event::CursorMoved { position: end }),
            end,
            bounds,
        );
        let mut draft = Draft::new(&doc);
        canvas.preview_move(&mut draft, &state, bounds);
        let preview = draft.preview.into_owned();
        let edit = event(
            &canvas,
            &mut state,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            end,
            bounds,
        )
        .unwrap();
        let Edit::StretchBond {
            fixed,
            moving,
            length,
        } = edit
        else {
            panic!("Expected one stretch edit");
        };
        assert!((length - 63.).abs() < 0.0001);
        let committed = Stretch::new(&doc, fixed, moving)
            .unwrap()
            .apply(&doc, length)
            .unwrap();
        assert_eq!(committed, preview);
        assert_eq!(doc, original);
        assert_eq!(committed.bonds, doc.bonds);
        assert!(state.gesture.is_none());
    }
}

#[test]
fn stretch_cancels_on_escape_focus_loss_outside_release_and_click_without_history_edit() {
    let doc = fixture();
    let arrow = Default::default();
    let graphic = Default::default();
    let bounds = Rectangle::with_size(Size::new(500., 350.));
    let canvas = canvas(&doc, &arrow, &graphic);
    let p = canvas.camera.screen(doc.atom(2).unwrap().position, bounds);
    for cancel in [
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
            location: iced::keyboard::Location::Standard,
            modifiers: Default::default(),
            text: None,
            repeat: false,
        }),
        Event::Window(iced::window::Event::Unfocused),
    ] {
        let mut state = State::default();
        event(
            &canvas,
            &mut state,
            Event::Mouse(mouse::Event::CursorMoved { position: p }),
            p,
            bounds,
        );
        event(
            &canvas,
            &mut state,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            p,
            bounds,
        );
        assert!(matches!(state.gesture, Some(Gesture::StretchBond { .. })));
        event(&canvas, &mut state, cancel, p, bounds);
        assert!(state.gesture.is_none());
        assert!(
            event(
                &canvas,
                &mut state,
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                p,
                bounds
            )
            .is_none()
        );
    }
    for end in [p, Point::new(-10., -10.)] {
        let mut state = State::default();
        event(
            &canvas,
            &mut state,
            Event::Mouse(mouse::Event::CursorMoved { position: p }),
            p,
            bounds,
        );
        event(
            &canvas,
            &mut state,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            p,
            bounds,
        );
        event(
            &canvas,
            &mut state,
            Event::Mouse(mouse::Event::CursorMoved { position: end }),
            end,
            bounds,
        );
        assert!(
            event(
                &canvas,
                &mut state,
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                end,
                bounds
            )
            .is_none()
        );
    }
}

#[test]
fn composed_stretch_tool_owns_a_coincident_cubic_handle_and_select_keeps_handle_editing() {
    use reshiki::arrow_anchors::{self, Pick};
    use reshiki::arrows::{ArrowStyle, Preset};
    let mut doc = fixture();
    let arrow_id = arrow_anchors::create(
        &mut doc,
        &Pick::Atom(1),
        &Pick::Atom(3),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap();
    let point = doc.atom(2).unwrap().position;
    doc.arrows.last_mut().unwrap().edit_handle(3, point);
    let original = doc.clone();
    let arrow = Default::default();
    let graphic = Default::default();
    let selected = [arrow_id];
    let mut canvas = canvas(&doc, &arrow, &graphic);
    canvas.selected = &selected;
    let bounds = Rectangle::with_size(Size::new(500., 350.));
    let p = canvas.camera.screen(point, bounds);
    for (tool, stretch) in [
        (
            Tool::StretchBond {
                fixed: 1,
                moving: 2,
            },
            true,
        ),
        (Tool::Select, false),
    ] {
        canvas.tool = tool;
        let mut state = State::default();
        event(
            &canvas,
            &mut state,
            Event::Mouse(mouse::Event::CursorMoved { position: p }),
            p,
            bounds,
        );
        event(
            &canvas,
            &mut state,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            p,
            bounds,
        );
        if stretch {
            assert!(matches!(state.gesture, Some(Gesture::StretchBond { .. })));
        } else {
            assert!(
                matches!(state.gesture, Some(Gesture::ArrowHandle { id, index: 3 }) if id == arrow_id)
            );
        }
    }
    assert_eq!(doc, original);
}
