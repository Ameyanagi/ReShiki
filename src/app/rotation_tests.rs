//! Reproduce #96 through the actual shortcut dispatcher and app history path.
use super::{App, Document, History, Message, Point, Transform, numeric_transforms, shortcuts};
use iced::keyboard::{Key, Modifiers, key::Named};

fn fixture(degrees: f64, offset: Point) -> App {
    let (mut app, _) = App::new();
    app.tab.doc = serde_json::from_str(include_str!(
        "../../tests/fixtures/pyrrole-rotation-drift.rsk"
    ))
    .unwrap();
    let (s, c) = degrees.to_radians().sin_cos();
    for atom in &mut app.tab.doc.atoms {
        let (x, y) = (f64::from(atom.position.x), f64::from(atom.position.y));
        atom.position =
            Point::new((x * c - y * s) as f32, (x * s + y * c) as f32).offset(offset.x, offset.y);
    }
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.saved = app.tab.doc.clone();
    app.tab.history = History::default();
    app.tab.busy = false;
    app.error = false;
    app.sync_numeric_transforms();
    app
}

fn key(app: &mut App, named: Named) {
    let key = Key::Named(named);
    let message = shortcuts::key_message(&key, &key, Modifiers::ALT).unwrap();
    let _ = app.update(message);
    assert!(!app.error, "{}", app.status);
}

fn numeric(app: &mut App, degrees: f32, apply_all: bool) {
    use numeric_transforms::{Action, Field};
    let before = app.tab.doc.clone();
    let _ = app.update(Message::NumericTransform(Action::Input(
        Field::Rotation,
        degrees.to_string(),
    )));
    assert_eq!(app.tab.doc, before, "Typing must not apply a rotation");
    let _ = app.update(Message::NumericTransform(if apply_all {
        Action::ApplyAll
    } else {
        Action::Apply(Field::Rotation)
    }));
    assert!(!app.error, "{}", app.status);
}

fn assert_coordinates(actual: &Document, expected: &Document) {
    let scale = expected.atoms.iter().fold(1_f32, |max, atom| {
        max.max(atom.position.x.abs()).max(atom.position.y.abs())
    });
    let epsilon = (8. * f32::EPSILON * scale).max(0.0001);
    assert_eq!(actual.bonds, expected.bonds);
    for (a, b) in actual.atoms.iter().zip(&expected.atoms) {
        assert_eq!(a.id, b.id);
        assert!(
            a.position.distance(b.position) <= epsilon,
            "atom {} moved by {} (limit {epsilon})",
            a.id,
            a.position.distance(b.position)
        );
    }
}

#[test]
fn rotation_shortcuts_return_pyrrole_and_preserve_each_history_step() {
    for orientation in [0., 17., 90.] {
        for offset in [Point::default(), Point::new(250., -130.)] {
            let mut app = fixture(orientation, offset);
            let original = app.tab.doc.clone();
            let selected = app.tab.selected.clone();
            let mut snapshots = vec![original.clone()];
            for _ in 0..3 {
                key(&mut app, Named::ArrowDown);
                snapshots.push(app.tab.doc.clone());
                key(&mut app, Named::ArrowUp);
                snapshots.push(app.tab.doc.clone());
                assert_coordinates(&app.tab.doc, &original);
                assert_eq!(app.tab.selected, selected);
            }
            for expected in snapshots[..6].iter().rev() {
                let _ = app.update(Message::Undo);
                assert_eq!(&app.tab.doc, expected);
                assert_eq!(app.tab.selected, selected);
            }
            assert!(!app.tab.history.can_undo());
            for expected in &snapshots[1..] {
                let _ = app.update(Message::Redo);
                assert_eq!(&app.tab.doc, expected);
                assert_eq!(app.tab.selected, selected);
            }
            assert!(!app.tab.history.can_redo());
        }
    }
}

#[test]
fn rotation_shortcuts_and_numeric_enter_and_apply_agree() {
    for apply_all in [false, true] {
        let mut app = fixture(17., Point::new(250., -130.));
        let mut single = fixture(17., Point::new(250., -130.));
        for _ in 0..6 {
            key(&mut app, Named::ArrowDown);
        }
        numeric(&mut single, 90., apply_all);
        assert_coordinates(&app.tab.doc, &single.tab.doc);

        let original = single.tab.doc.clone();
        numeric(&mut single, 15., apply_all);
        let wire = serde_json::to_vec(&single.tab.doc).unwrap();
        single.tab.doc = serde_json::from_slice(&wire).unwrap();
        numeric(&mut single, -15., apply_all);
        assert_coordinates(&single.tab.doc, &original);
        key(&mut single, Named::ArrowRight);
        key(&mut single, Named::ArrowLeft);
        assert_coordinates(&single.tab.doc, &original);
    }
}

#[test]
fn rotation_menu_steps_and_toolbar_half_turn_are_consistent() {
    let mut app = fixture(0., Point::default());
    let original = app.tab.doc.clone();
    for degrees in [30., -30.] {
        let _ = app.update(Message::Transform(Transform::Rotate(degrees)));
    }
    assert_coordinates(&app.tab.doc, &original);
    let mut steps = fixture(0., Point::default());
    for _ in 0..12 {
        key(&mut steps, Named::ArrowDown);
    }
    let _ = app.update(super::object_toolbar::Command::Rotate.message());
    assert_coordinates(&app.tab.doc, &steps.tab.doc);
    let _ = app.update(super::object_toolbar::Command::Rotate.message());
    assert_coordinates(&app.tab.doc, &original);
}

#[test]
fn rotation_commands_keep_mixed_groups_and_upright_caption_anchors() {
    use reshiki::{
        document::{Annotation, Arrow},
        graphics::{Graphic, GraphicKind},
    };
    let mut app = fixture(17., Point::new(250., -130.));
    app.tab.doc.annotations.push(Annotation {
        id: app.tab.doc.next_id(),
        position: Point::new(350., 20.),
        text: "Caption".into(),
        format: Default::default(),
    });
    app.tab.doc.arrows.push(Arrow::new(
        app.tab.doc.next_id(),
        Point::new(340., -100.),
        Point::new(460., -60.),
        reshiki::arrows::Preset::Fishhook,
        Default::default(),
    ));
    app.tab.doc.graphics.push(Graphic::dragged(
        app.tab.doc.next_id(),
        GraphicKind::Arc,
        Point::new(100., -160.),
        Point::new(170., -70.),
        Default::default(),
        Default::default(),
        false,
    ));
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.doc.group_selection(&app.tab.selected).unwrap();
    app.sync_numeric_transforms();
    let original = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let mut single = fixture(0., Point::default());
    single.tab.doc = original.clone();
    single.tab.selected = selected.clone();
    single.sync_numeric_transforms();
    for _ in 0..6 {
        key(&mut app, Named::ArrowDown);
    }
    numeric(&mut single, 90., true);
    assert_coordinates(&app.tab.doc, &single.tab.doc);
    let points = |doc: &Document| {
        doc.annotations
            .iter()
            .map(|a| a.position)
            .chain(doc.arrows.iter().flat_map(|a| {
                [Some(a.start), Some(a.end), a.control_point()]
                    .into_iter()
                    .flatten()
            }))
            .chain(
                doc.graphics
                    .iter()
                    .flat_map(|g| g.commands().into_iter().flat_map(|c| c.points())),
            )
            .collect::<Vec<_>>()
    };
    for (actual, expected) in points(&app.tab.doc)
        .into_iter()
        .zip(points(&single.tab.doc))
    {
        assert!(actual.distance(expected) < 0.0005);
    }
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.doc.groups, original.groups);
    assert_eq!(
        app.tab.doc.annotations[0].format,
        original.annotations[0].format
    );
    assert_eq!(app.tab.doc.arrows[0].style, original.arrows[0].style);
    assert_eq!(app.tab.doc.graphics[0].style, original.graphics[0].style);
    for _ in 0..6 {
        let _ = app.update(Message::Undo);
    }
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.tab.selected, selected);

    let mut caption = fixture(0., Point::default());
    caption.tab.doc = Document::default();
    caption.tab.doc.annotations.push(Annotation {
        id: 1,
        position: Point::new(20., -40.),
        text: "Keep upright".into(),
        format: Default::default(),
    });
    caption.tab.selected = vec![1];
    let before = caption.tab.doc.clone();
    key(&mut caption, Named::ArrowDown);
    assert_eq!(caption.tab.doc, before);
    assert!(!caption.tab.history.can_undo());
}

#[test]
fn rotation_pointer_events_cancel_without_history_and_commit_once_per_gesture() {
    use crate::canvas::{
        Camera,
        rotation_gesture_tests::{self, Finish},
    };
    use reshiki::{
        document::{Annotation, Arrow},
        graphics::{Graphic, GraphicKind},
    };
    let points = |doc: &Document, ids: &[u64]| {
        doc.atoms
            .iter()
            .filter(|a| ids.contains(&a.id))
            .map(|a| a.position)
            .chain(doc.annotations.iter().map(|a| a.position))
            .chain(doc.arrows.iter().flat_map(|a| {
                [Some(a.start), Some(a.end), a.control_point()]
                    .into_iter()
                    .flatten()
            }))
            .chain(
                doc.graphics
                    .iter()
                    .flat_map(|g| g.commands().into_iter().flat_map(|c| c.points())),
            )
            .collect::<Vec<_>>()
    };
    let cancel = |app: &mut App, camera, pivot, moves, finish| {
        let before = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        let revision = app.tab.revision;
        let history = (app.tab.history.can_undo(), app.tab.history.can_redo());
        let events =
            rotation_gesture_tests::rotation_drag(&before, &selected, camera, pivot, moves, finish);
        if moves > 0 {
            assert_ne!(
                events.preview, before,
                "Cancel must discard an actual rotation preview"
            );
        }
        assert!(events.release.is_none());
        for edit in events.before_release {
            let _ = app.update(Message::Canvas(edit));
        }
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, selected);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(
            (app.tab.history.can_undo(), app.tab.history.can_redo()),
            history
        );
    };
    for mixed in [false, true] {
        let (mut app, _) = App::new();
        let mut source = Document::default();
        let a = source.add_atom("C", Point::new(-20., -10.));
        let b = source.add_atom("C", Point::new(40., 0.));
        let c = source.add_atom("N", Point::new(0., 50.));
        source.add_bond(a, b, 1, "wedge");
        source.add_bond(b, c, 1, "plain");
        source.atom_mut(a).unwrap().depth = 7.;
        let pivot = if mixed {
            source.annotations.push(Annotation {
                id: source.next_id(),
                position: Point::new(120., 60.),
                text: "Upright caption".into(),
                format: Default::default(),
            });
            source.arrows.push(Arrow::new(
                source.next_id(),
                Point::new(80., -60.),
                Point::new(160., -20.),
                reshiki::arrows::Preset::Fishhook,
                Default::default(),
            ));
            source.graphics.push(Graphic::dragged(
                source.next_id(),
                GraphicKind::Rectangle,
                Point::new(-140., -40.),
                Point::new(-80., 60.),
                Default::default(),
                Default::default(),
                false,
            ));
            // Three atoms, caption insertion, arrow midpoint and rectangle center.
            Point::new(25., 70. / 6.)
        } else {
            Point::new(20. / 3., 40. / 3.)
        };
        let selected = source.all_ids();
        source.group_selection(&selected).unwrap();
        let stationary = source.add_atom("O", Point::new(300., 180.));
        let initial_points = points(&source, &selected);
        for zoom in [0.5, 1., 3.] {
            let camera = Camera {
                center: Point::new(30., -15.),
                zoom,
            };
            for moves in [1, 7, 31] {
                app.tab.doc = source.clone();
                app.tab.selected = selected.clone();
                app.tab.history = History::default();
                app.tab.busy = false;
                app.error = false;
                app.sync_numeric_transforms();
                for (count, finish) in [
                    (0, Finish::Release),
                    (19, Finish::Escape),
                    (19, Finish::FocusLost),
                ] {
                    cancel(&mut app, camera, pivot, count, finish);
                }
                let mut snapshots = vec![source.clone()];
                for step in 1..=6 {
                    let before = app.tab.doc.clone();
                    let revision = app.tab.revision;
                    let events = rotation_gesture_tests::rotation_drag(
                        &before,
                        &selected,
                        camera,
                        pivot,
                        moves,
                        Finish::Release,
                    );
                    for edit in events.before_release {
                        let _ = app.update(Message::Canvas(edit));
                        assert_eq!(app.tab.doc, before, "Pointer moves must only preview");
                        assert_eq!(app.tab.revision, revision);
                    }
                    let _ = app.update(Message::Canvas(events.release.unwrap()));
                    assert_eq!(
                        app.tab.doc, events.preview,
                        "Release must commit the preview"
                    );
                    assert_eq!(app.tab.selected, selected);
                    assert_eq!(app.tab.doc.atom(stationary), source.atom(stationary));
                    assert_eq!(app.tab.doc.bonds, source.bonds);
                    assert_eq!(app.tab.doc.groups, source.groups);
                    assert_eq!(app.tab.doc.atom(a).unwrap().depth, 7.);
                    let (s, c) = f64::from(step * 15).to_radians().sin_cos();
                    let expected: Vec<_> = initial_points
                        .iter()
                        .map(|p| {
                            let x = f64::from(p.x) - f64::from(pivot.x);
                            let y = f64::from(p.y) - f64::from(pivot.y);
                            Point::new(
                                (f64::from(pivot.x) + x * c - y * s) as f32,
                                (f64::from(pivot.y) + x * s + y * c) as f32,
                            )
                        })
                        .collect();
                    let scale = initial_points
                        .iter()
                        .chain(&expected)
                        .fold(1_f32, |m, p| m.max(p.x.abs()).max(p.y.abs()));
                    let epsilon = (8. * f32::EPSILON * scale).max(0.0001);
                    let actual = points(&app.tab.doc, &selected);
                    assert_eq!(actual.len(), expected.len());
                    for (actual, expected) in actual.into_iter().zip(expected) {
                        assert!(
                            actual.distance(expected) <= epsilon,
                            "{actual:?} != {expected:?}"
                        );
                    }
                    snapshots.push(app.tab.doc.clone());
                    if step == 3 {
                        cancel(&mut app, camera, pivot, 19, Finish::Escape);
                        cancel(&mut app, camera, pivot, 19, Finish::FocusLost);
                    }
                }
                for expected in snapshots[..6].iter().rev() {
                    let _ = app.update(Message::Undo);
                    assert_eq!(&app.tab.doc, expected);
                    assert_eq!(app.tab.selected, selected);
                }
                assert!(
                    !app.tab.history.can_undo(),
                    "Six releases must be exactly six edits"
                );
                cancel(&mut app, camera, pivot, 19, Finish::Escape);
                cancel(&mut app, camera, pivot, 19, Finish::FocusLost);
                for expected in &snapshots[1..] {
                    let _ = app.update(Message::Redo);
                    assert_eq!(
                        &app.tab.doc, expected,
                        "Cancel must preserve every Redo snapshot"
                    );
                    assert_eq!(app.tab.selected, selected);
                }
                assert!(!app.tab.history.can_redo());
            }
        }
    }
}
