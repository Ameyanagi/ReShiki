use super::*;
use crate::canvas::{Camera, Edit};
use iced::widget::canvas::Program;
use iced::{Event, Size, mouse};
use reshiki::{document::Document, editing::reference::Stretch};

fn canvas<'a>(
    doc: &'a Document,
    arrow: &'a reshiki::arrows::ArrowStyle,
    graphic: &'a reshiki::graphics::GraphicStyle,
) -> MoleculeCanvas<'a> {
    MoleculeCanvas {
        nmr: None,
        optimizer: None,
        keyboard_target: None,
        joining: None,
        hidden_annotation: None,
        bond_drawing: Default::default(),
        chain_drawing: Default::default(),
        doc,
        element: "C",
        selected: &[],
        tool: Tool::StretchBond {
            fixed: 1,
            moving: 2,
        },
        camera: Camera::default(),
        grid: false,
        guides: Default::default(),
        smart_guides: true,
        ring_size: 6,
        aromatic_ring: false,
        template_connection: Default::default(),
        template: None,
        arrow_preset: Default::default(),
        arrow_style: arrow,
        arrow_source: None,
        attach_arrow_targets: true,
        orbital_phase: Default::default(),
        phase_flipped: false,
        attach_symbols: true,
        snap_orbitals: true,
        graphic_constrain: false,
        graphic_arc: Default::default(),
        graphic_style: graphic,
        graphic_point: None,
        bracket_sides: Default::default(),
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
