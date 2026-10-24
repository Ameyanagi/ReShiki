use super::*;
use crate::canvas::{Camera, Gesture, Handle, SelectionBox, Tool, tests::chain_canvas};
use iced::{Event, Vector, keyboard::Modifiers, mouse, widget::canvas::Program};
use reshiki::chains::ChainMode;
use reshiki::document::Document;
use reshiki::models3d::{Display, ForceField, Hydrogen, Model3d, Placed};

/// Ethanol shown as a 3D model in place, and a separate methane.
fn drawing() -> (Document, Vec<u64>) {
    let mut doc = Document::default();
    let c1 = doc.add_atom("C", World::new(300., 300.));
    let c2 = doc.add_atom("C", World::new(336., 279.));
    let o = doc.add_atom("O", World::new(372., 300.));
    doc.add_bond(c1, c2, 1, "plain");
    doc.add_bond(c2, o, 1, "plain");
    doc.add_atom("C", World::new(600., 300.));
    let placed = |id, xyz| Placed { id, xyz };
    let model = Model3d::new(
        &doc,
        vec![
            placed(c1, [-1.2, -0.2, 0.1]),
            placed(c2, [0.1, 0.5, -0.1]),
            placed(o, [1.2, -0.3, 0.2]),
        ],
        vec![Hydrogen {
            parent: c1,
            xyz: [-2.0, 0.4, 0.5],
        }],
        ForceField::Mmff94s,
    );
    doc.models.push(model);
    doc.require_version(reshiki::document::MODELS_3D);
    assert!(models3d::set_display(&mut doc, 0, Display::ThreeD));
    (doc, vec![c1, c2, o])
}

const BOUNDS: Rectangle = Rectangle {
    x: 0.,
    y: 0.,
    width: 800.,
    height: 600.,
};

fn canvas<'a>(doc: &'a Document, selected: &'a [u64]) -> MoleculeCanvas<'a> {
    MoleculeCanvas {
        tool: Tool::Select,
        selected,
        camera: Camera {
            center: World::new(400., 300.),
            zoom: 1.,
        },
        ..chain_canvas(doc, ChainMode::Straight)
    }
}

fn send(canvas: &MoleculeCanvas<'_>, state: &mut State, event: Event) -> Option<Edit> {
    canvas
        .update(state, &event, BOUNDS, mouse::Cursor::Unavailable)
        .and_then(|action| action.into_inner().0)
}

fn moved(position: Point) -> Event {
    Event::Mouse(mouse::Event::CursorMoved { position })
}
const PRESS: Event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
const RELEASE: Event = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));

fn frame_center(canvas: &MoleculeCanvas<'_>) -> Point {
    let (lo, hi) = models3d::frame(canvas.doc, &canvas.doc.models[0]).unwrap();
    canvas
        .camera
        .screen(World::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.), BOUNDS)
}

/// Press at `from`, move through `path`, release at the last point.
fn drag(
    canvas: &MoleculeCanvas<'_>,
    state: &mut State,
    from: Point,
    path: &[Point],
) -> (Option<[f32; 4]>, Option<Edit>) {
    send(canvas, state, moved(from));
    send(canvas, state, PRESS);
    let mut preview = None;
    for p in path {
        send(canvas, state, moved(*p));
        if let Some(Gesture::Turn(turn)) = &state.gesture {
            let mut doc = canvas.doc.clone();
            preview_turn(&mut doc, turn, *p, state.modifiers.shift());
            preview = Some(doc.models[0].rotation);
        }
    }
    (preview, send(canvas, state, RELEASE))
}

#[test]
fn dragging_a_selected_model_turns_it_and_the_preview_is_the_commit() {
    let (doc, atoms) = drawing();
    let canvas = canvas(&doc, &atoms);
    let start = doc.models[0].rotation;
    let c = frame_center(&canvas);
    for roll in [false, true] {
        let mut state = State {
            modifiers: if roll {
                Modifiers::SHIFT
            } else {
                Modifiers::default()
            },
            ..Default::default()
        };
        let path = [
            c + Vector::new(1., 0.),
            c + Vector::new(20., 6.),
            c + Vector::new(31., -4.),
        ];
        let (preview, edit) = drag(&canvas, &mut state, c, &path);
        let Some(Edit::ModelView {
            atoms: turned,
            rotation,
        }) = edit
        else {
            panic!("{edit:?}");
        };
        assert_eq!(turned, atoms);
        assert_eq!(preview, Some(rotation), "the preview is the commit");
        // Within the slop nothing turns; then each step turns from the last.
        let expected = models3d::turn(models3d::turn(start, 20., 6., roll), 11., -10., roll);
        assert_eq!(rotation, expected);
    }
}

#[test]
fn an_unselected_model_or_its_edge_moves_and_alt_or_copy_drags_move() {
    let (mut doc, atoms) = drawing();
    let text = doc.next_id();
    doc.annotations.push(reshiki::document::Annotation {
        id: text,
        position: World::new(100., 100.),
        text: "Note".into(),
        format: Default::default(),
    });
    let with_text = [atoms.clone(), vec![text]].concat();
    let (lo, _) = models3d::frame(&doc, &doc.models[0]).unwrap();
    let edge = canvas(&doc, &atoms)
        .camera
        .screen(lo.offset(3., 30.), BOUNDS);
    for (selected, from, modifiers) in [
        (&[][..], None, Modifiers::default()),
        // The model with another object moves the whole selection.
        (&with_text[..], None, Modifiers::default()),
        (&atoms[..], Some(edge), Modifiers::default()),
        (&atoms[..], None, Modifiers::ALT),
        (&atoms[..], None, Modifiers::COMMAND),
    ] {
        let canvas = canvas(&doc, selected);
        let from = from.unwrap_or_else(|| frame_center(&canvas));
        let mut state = State {
            modifiers,
            ..Default::default()
        };
        send(&canvas, &mut state, moved(from));
        send(&canvas, &mut state, PRESS);
        assert!(
            matches!(state.gesture, Some(Gesture::Move { .. })),
            "{selected:?} {modifiers:?}"
        );
    }
}

#[test]
fn a_double_click_restores_the_opening_view() {
    let (mut doc, atoms) = drawing();
    let opening = doc.models[0].rotation;
    doc.models[0].rotation = models3d::turn(opening, 50., 20., false);
    let canvas = canvas(&doc, &atoms);
    let c = frame_center(&canvas);
    let mut state = State::default();
    let (_, first) = drag(&canvas, &mut state, c, &[]);
    assert!(
        matches!(first, Some(Edit::Select(_) | Edit::SelectAt(..))),
        "{first:?}"
    );
    let (_, second) = drag(&canvas, &mut state, c, &[]);
    let Some(Edit::ModelView { rotation, .. }) = second else {
        panic!("{second:?}");
    };
    assert_eq!(rotation, opening);
}

/// The turned model draws quickly while dragged, and not before it turns.
#[test]
fn a_model_is_turning_only_while_the_drag_turns_it() {
    let (doc, atoms) = drawing();
    let canvas = canvas(&doc, &atoms);
    let c = frame_center(&canvas);
    let mut state = State::default();
    let turning = |state: &State| {
        canvas
            .model_preview_document(state, BOUNDS)
            .models
            .iter()
            .any(|m| m.turning)
    };
    send(&canvas, &mut state, moved(c));
    send(&canvas, &mut state, PRESS);
    assert!(!turning(&state));
    send(&canvas, &mut state, moved(c + Vector::new(20., 6.)));
    assert!(turning(&state));
    send(&canvas, &mut state, RELEASE);
    assert!(!turning(&state));
}

#[test]
fn a_short_drag_inside_a_selected_model_is_a_click() {
    let (doc, atoms) = drawing();
    let canvas = canvas(&doc, &atoms);
    let c = frame_center(&canvas);
    let mut state = State::default();
    // Under the 3 px turn threshold: the preview stays put, so must the commit.
    let (preview, edit) = drag(&canvas, &mut state, c, &[c + Vector::new(2., 0.)]);
    assert_eq!(preview, Some(doc.models[0].rotation));
    assert!(
        matches!(edit, Some(Edit::Select(_) | Edit::SelectAt(..))),
        "{edit:?}"
    );
}

#[test]
fn corner_handles_resize_the_frame_and_edges_are_hidden() {
    let (doc, atoms) = drawing();
    let canvas = canvas(&doc, &atoms);
    let selection = SelectionBox::new(&doc, &atoms, canvas.camera, BOUNDS).unwrap();
    let (lo, hi) = reshiki::scene::selection_bounds(&doc, &atoms).unwrap();
    let corner = canvas.camera.screen(hi, BOUNDS) + Vector::new(12., 12.);
    assert_eq!(selection.hit(corner), Some(Handle::Resize(2)));
    let bottom = canvas
        .camera
        .screen(World::new((lo.x + hi.x) / 2., hi.y), BOUNDS)
        + Vector::new(0., 12.);
    assert_eq!(selection.hit(bottom), None, "no edge handles");

    let mut state = State::default();
    send(&canvas, &mut state, moved(corner));
    send(&canvas, &mut state, PRESS);
    let end = corner + Vector::new(40., 40.);
    send(&canvas, &mut state, moved(end));
    let Some(Gesture::Transform(drag)) = &state.gesture else {
        panic!("{:?}", state.gesture);
    };
    let mut preview = doc.clone();
    drag.apply(&mut preview, canvas.camera.world(end, BOUNDS), false);
    let Some(Edit::ResizeModels { ids, pivot, scale }) = send(&canvas, &mut state, RELEASE) else {
        panic!("a corner drag resizes the model");
    };
    let mut committed = doc.clone();
    models3d::resize(&mut committed, &ids, pivot, scale);
    assert_eq!(preview, committed);
    assert!(scale > 1. && committed.models[0].size > doc.models[0].size);
    assert_eq!(committed.atoms, doc.atoms);

    // A shown model mixed with another object still has no resize handles.
    let mixed = [atoms.clone(), vec![doc.atoms[3].id]].concat();
    let selection = SelectionBox::new(&doc, &mixed, canvas.camera, BOUNDS).unwrap();
    let (_, hi) = reshiki::scene::selection_bounds(&doc, &mixed).unwrap();
    let corner = canvas.camera.screen(hi, BOUNDS) + Vector::new(12., 12.);
    assert_eq!(selection.hit(corner), None);

    // A model of one drawn atom (methane) resizes too.
    let mut doc = Document::default();
    let c = doc.add_atom("C", World::new(300., 300.));
    let h = |xyz| Hydrogen { parent: c, xyz };
    let hydrogens = vec![
        h([0.6, 0.6, 0.6]),
        h([-0.6, -0.6, 0.6]),
        h([-0.6, 0.6, -0.6]),
        h([0.6, -0.6, -0.6]),
    ];
    let placed = vec![Placed {
        id: c,
        xyz: [0.; 3],
    }];
    doc.models
        .push(Model3d::new(&doc, placed, hydrogens, ForceField::Mmff94s));
    doc.require_version(reshiki::document::MODELS_3D);
    assert!(models3d::set_display(&mut doc, 0, Display::ThreeD));
    let selection = SelectionBox::new(&doc, &[c], canvas.camera, BOUNDS).unwrap();
    let (_, hi) = reshiki::scene::selection_bounds(&doc, &[c]).unwrap();
    let corner = canvas.camera.screen(hi, BOUNDS) + Vector::new(12., 12.);
    assert_eq!(selection.hit(corner), Some(Handle::Resize(2)));
}

#[test]
fn a_press_above_a_selected_molecule_selects_on_the_paper() {
    // No Show as bar floats there (#422): the press starts a marquee.
    let (doc, atoms) = drawing();
    let canvas = canvas(&doc, &atoms);
    let (lo, hi) = reshiki::scene::selection_bounds(&doc, &atoms).unwrap();
    let above = World::new((lo.x + hi.x) / 2., lo.y - 70. / canvas.camera.zoom);
    let mut state = State::default();
    send(
        &canvas,
        &mut state,
        moved(canvas.camera.screen(above, BOUNDS)),
    );
    send(&canvas, &mut state, PRESS);
    assert!(
        matches!(state.gesture, Some(Gesture::Select { .. })),
        "{:?}",
        state.gesture
    );
}

#[test]
fn the_morph_redraws_until_it_ends() {
    let (doc, atoms) = drawing();
    let mut canvas = canvas(&doc, &atoms);
    let now = Instant::now();
    let redraw = |canvas: &MoleculeCanvas<'_>, at| {
        canvas
            .update(
                &mut State::default(),
                &Event::Window(iced::window::Event::RedrawRequested(at)),
                BOUNDS,
                mouse::Cursor::Unavailable,
            )
            .is_some()
    };
    assert!(!redraw(&canvas, now));
    canvas.morph = Some((atoms[0], now));
    assert!(redraw(&canvas, now + MORPH / 2));
    assert_eq!(
        canvas.morph(now + MORPH / 2).map(|(id, _)| id),
        Some(atoms[0])
    );
    assert!(!redraw(&canvas, now + MORPH));
}
