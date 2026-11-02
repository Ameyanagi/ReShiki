use super::*;
use crate::canvas::{Tool, tests::chain_canvas};
use reshiki::models3d::{Display, ForceField, Model3d, Placed};

fn drawing() -> (Document, Vec<u64>) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-21., 0.));
    let b = doc.add_atom("C", World::new(21., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.models.push(Model3d::new(
        &doc,
        vec![
            Placed {
                id: a,
                xyz: [-0.75, 0., 0.],
            },
            Placed {
                id: b,
                xyz: [0.75, 0., 0.],
            },
        ],
        vec![],
        ForceField::Mmff94s,
    ));
    reshiki::models3d::require_version(&mut doc);
    reshiki::models3d::set_display(&mut doc, 0, Display::ThreeD);
    (doc, vec![a, b])
}

#[test]
fn selected_shown_model_explains_gestures_before_pressing_anywhere_on_canvas() {
    let (mut doc, atoms) = drawing();
    let caption = reshiki::molecule_names::show(
        &mut doc,
        &atoms,
        "ethane".into(),
        "CC".into(),
        Default::default(),
        true,
    )
    .unwrap();
    let selected = doc.expand_groups(&atoms);
    let bounds = Rectangle::with_size(iced::Size::new(800., 600.));
    for display in [Display::ThreeD, Display::Both] {
        reshiki::models3d::set_display(&mut doc, 0, display);
        for ids in [&atoms[..], &selected[..]] {
            for tool in [Tool::Select, Tool::Lasso] {
                let canvas = MoleculeCanvas {
                    tool,
                    selected: ids,
                    ..chain_canvas(&doc, reshiki::chains::ChainMode::Straight)
                };
                for cursor in [Point::new(400., 300.), Point::new(20., 550.)] {
                    let state = State {
                        cursor: Some(cursor),
                        ..Default::default()
                    };
                    let mut draft = Draft::new(&doc);
                    canvas.preview_model(&mut draft, &state, bounds);
                    let (text, valid) = draft.template_notice.unwrap();
                    assert!(valid);
                    for instruction in ["rotate", "Shift-drag to roll", "Double-click to reset"] {
                        assert!(text.contains(instruction), "{text}");
                    }
                    assert!(
                        matches!(draft.preview, Cow::Borrowed(_)),
                        "A resting hint must not rebuild the model"
                    );
                }
            }
        }
    }
    for (tool, ids, cursor) in [
        (Tool::Select, &[][..], Some(Point::new(20., 550.))),
        (Tool::Select, &[caption][..], Some(Point::new(20., 550.))),
        (Tool::Select, &selected[..], None),
        (Tool::Select, &selected[..], Some(Point::new(900., 550.))),
        (Tool::Ring, &selected[..], Some(Point::new(20., 550.))),
        (Tool::Tilt, &selected[..], Some(Point::new(20., 550.))),
    ] {
        let canvas = MoleculeCanvas {
            tool,
            selected: ids,
            ..chain_canvas(&doc, reshiki::chains::ChainMode::Straight)
        };
        let mut draft = Draft::new(&doc);
        canvas.preview_model(
            &mut draft,
            &State {
                cursor,
                ..Default::default()
            },
            bounds,
        );
        assert!(
            draft.template_notice.is_none(),
            "{tool:?} {ids:?} {cursor:?}"
        );
    }
}

#[test]
fn resting_model_hint_excludes_other_objects_and_keeps_the_live_turn_hint() {
    let (mut doc, atoms) = drawing();
    let other = doc.add_atom("N", World::new(200., 0.));
    let mixed = [atoms.clone(), vec![other]].concat();
    let bounds = Rectangle::with_size(iced::Size::new(800., 600.));
    let cursor = Point::new(400., 300.);
    {
        let canvas = MoleculeCanvas {
            tool: Tool::Select,
            selected: &mixed,
            ..chain_canvas(&doc, reshiki::chains::ChainMode::Straight)
        };
        let mut draft = Draft::new(&doc);
        canvas.preview_model(
            &mut draft,
            &State {
                cursor: Some(cursor),
                ..Default::default()
            },
            bounds,
        );
        assert!(draft.template_notice.is_none());
    }
    reshiki::models3d::set_display(&mut doc, 0, Display::TwoD);
    {
        let canvas = MoleculeCanvas {
            tool: Tool::Select,
            selected: &atoms,
            ..chain_canvas(&doc, reshiki::chains::ChainMode::Straight)
        };
        let mut draft = Draft::new(&doc);
        canvas.preview_model(
            &mut draft,
            &State {
                cursor: Some(cursor),
                ..Default::default()
            },
            bounds,
        );
        assert!(draft.template_notice.is_none());
    }
    reshiki::models3d::set_display(&mut doc, 0, Display::ThreeD);
    let canvas = MoleculeCanvas {
        tool: Tool::Select,
        selected: &atoms,
        ..chain_canvas(&doc, reshiki::chains::ChainMode::Straight)
    };
    let (lo, hi) = reshiki::models3d::frame(&doc, &doc.models[0]).unwrap();
    let center = World::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    let mut state = State {
        cursor: Some(canvas.camera.screen(center, bounds)),
        ..Default::default()
    };
    assert!(
        canvas
            .press_turn(&mut state, center, canvas.camera.screen(center, bounds))
            .is_some()
    );
    let mut draft = Draft::new(&doc);
    canvas.preview_model(&mut draft, &state, bounds);
    assert_eq!(
        draft.template_notice,
        Some((
            "Drag to rotate · Shift-drag to roll · Double-click to reset".into(),
            true
        ))
    );
}

#[test]
fn caption_follows_held_model_resize_at_the_same_position_as_the_commit() {
    use crate::canvas::selection::{Handle, SelectionBox, TransformDrag};
    let (mut doc, atoms) = drawing();
    let caption = reshiki::molecule_names::show(
        &mut doc,
        &atoms,
        "ethane".into(),
        "CC".into(),
        Default::default(),
        true,
    )
    .unwrap();
    let ids = reshiki::molecule_names::include_annotations(&doc, &atoms);
    let bounds = Rectangle::new(Point::new(50., 80.), iced::Size::new(800., 600.));
    for display in [Display::ThreeD, Display::Both] {
        reshiki::models3d::set_display(&mut doc, 0, display);
        reshiki::molecule_names::preview_layout(&mut doc, &atoms);
        for zoom in [0.5, 1., 2.] {
            for ids in [&ids[..], &atoms[..1]] {
                let canvas = MoleculeCanvas {
                    selected: ids,
                    camera: Camera {
                        zoom,
                        ..Default::default()
                    },
                    ..chain_canvas(&doc, reshiki::chains::ChainMode::Straight)
                };
                let selection = SelectionBox::new(&doc, ids, canvas.camera, bounds).unwrap();
                let (_, hi) = reshiki::models3d::frame(&doc, &doc.models[0]).unwrap();
                let start = hi.offset(12. / zoom, 12. / zoom);
                let end = start.offset(45. / zoom, 45. / zoom);
                let drag = TransformDrag::new(selection, Handle::Resize(2), start, ids);
                let (scale, _) = drag.values(end, false);
                let pivot = drag.pivot;
                let state = State {
                    cursor: Some(
                        canvas.camera.screen(end, bounds) + iced::Vector::new(bounds.x, bounds.y),
                    ),
                    gesture: Some(super::super::Gesture::Transform(Box::new(drag))),
                    ..Default::default()
                };
                let mut draft = Draft::new(&doc);
                canvas.preview_transform(&mut draft, &state, bounds);
                let preview = draft.preview.into_owned();
                let mut committed = doc.clone();
                reshiki::models3d::resize(&mut committed, ids, pivot, scale);
                let _ = reshiki::transaction::reconcile(&mut committed, doc.clone()).unwrap();
                let caption_at = |drawing: &Document| {
                    drawing
                        .annotations
                        .iter()
                        .find(|a| a.id == caption)
                        .unwrap()
                        .position
                };
                assert!(caption_at(&preview).distance(caption_at(&committed)) < 1e-3);
                assert!(caption_at(&preview).distance(caption_at(&doc)) > 1.);
                assert_eq!(preview.molecule_names, doc.molecule_names);
                assert_eq!(preview.atoms, doc.atoms);
                assert_eq!(preview.bonds, doc.bonds);
                assert_eq!(preview.models[0].atoms, doc.models[0].atoms);
            }
        }
    }
}
