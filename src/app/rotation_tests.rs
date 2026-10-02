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
