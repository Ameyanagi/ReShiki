use super::*;
use crate::canvas::Edit;
use reshiki::document::{Document, Point};

fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.doc.add_atom("C", Point::default());
    let source = app.tab.doc.add_atom("C", Point::new(180., 0.));
    let end = app.tab.doc.add_atom("C", Point::new(222., 0.));
    app.tab.doc.add_bond(source, end, 1, "plain");
    app.tab.selected = vec![source];
    app
}
#[test]
fn joining_uses_the_preview_and_is_one_undo_step() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Join(Action::Begin));
    assert_eq!(app.tab.doc, before);
    let state = app.tab.joining.as_ref().unwrap();
    let (expected, _) = state
        .prepared
        .place(
            Point::default(),
            None,
            10. / app.tab.camera.zoom,
            state.anchor,
            state.mode,
        )
        .unwrap();
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, expected);
    assert_eq!(app.tab.doc.bonds.len(), 2);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, expected);
}
#[test]
fn cancel_switching_tools_and_invalid_targets_keep_the_original() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Canvas(Edit::Template(
        Point::new(500., 500.),
        None,
    )));
    assert!(app.tab.joining.is_some());
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Escape);
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Tool(Tool::Bond(2)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tool, Tool::Bond(2));
    assert_eq!(app.tab.doc, before);
}
#[test]
fn a_changed_document_cannot_be_overwritten_by_a_prepared_join() {
    let mut app = ready();
    let _ = app.update(Message::Join(Action::Begin));
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("O", Point::new(300., 100.));
    app.changed(before);
    let changed = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, changed);
    assert!(app.error);
}
#[test]
fn anchor_picker_switches_between_atom_and_bond_modes() {
    let mut app = ready();
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Bond(2, 3))));
    assert_eq!(app.tab.joining.as_ref().unwrap().mode, Connection::FuseBond);
    let _ = app.update(Message::Join(Action::Mode(Connection::ShareAtom)));
    assert!(matches!(
        app.tab.joining.as_ref().unwrap().anchor,
        Anchor::Atom(_)
    ));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(3))));
    assert_eq!(app.tab.joining.as_ref().unwrap().anchor, Anchor::Atom(3));
}

#[test]
fn direct_join_commands_share_selection_requirements_and_routes() {
    use super::super::shortcuts;
    let mut app = ready();
    for (selection, join, merge) in [
        (vec![], false, false),
        (vec![1], false, false),
        (vec![1, 2], true, false),
        (vec![3, 2, 1], true, true),
        (vec![1, 2, 999], false, false),
    ] {
        app.tab.selected = selection;
        let commands = app.selected_join_commands();
        assert_eq!(commands[0].enabled, join);
        assert_eq!(commands[1].enabled, merge);
        assert!(matches!(
            commands[0].message,
            Message::Shortcut(shortcuts::Action::Join)
        ));
        assert!(matches!(
            commands[1].message,
            Message::Shortcut(shortcuts::Action::MergeAtoms)
        ));
    }
    app.tab.doc.atom_mut(2).unwrap().centroid = vec![1, 3];
    app.tab.selected = vec![1, 2, 3];
    assert!(app.selected_join_commands().iter().all(|c| !c.enabled));
}

#[test]
fn both_atom_merge_commands_use_rendered_ink_and_exclude_unselected_objects() {
    use super::super::shortcuts;
    use reshiki::{document::Annotation, palette::Color, scene::Primitive, typography::TextStyle};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/join-three-atoms-before.rsk"
    ))
    .unwrap();
    app.tab.doc.atom_labels.hydrogens = false;
    app.tab.doc.atom_labels.carbons = reshiki::atom_labels::Carbons::All;
    let ink = [17, 71, 149];
    for (id, size_pt) in [(1, 32.), (3, 10.), (5, 24.)] {
        app.tab.doc.atom_mut(id).unwrap().text_style = Some(TextStyle {
            size_pt,
            color: Color::Custom(ink),
            ..Default::default()
        });
    }
    app.tab.doc.annotations.push(Annotation {
        id: 7,
        position: Point::new(-1000., -1500.),
        text: "Unselected caption".into(),
        format: Default::default(),
    });
    app.tab.doc.atom_mut(1).unwrap().display.number = Some(reshiki::atom_labels::Number {
        text: "999".into(),
        offset: Some(Point::new(900., 900.)),
        style: Default::default(),
    });
    let before = app.tab.doc.clone();
    // Measure the actual rendered text primitives independently of the app
    // helper. Only the three selected labels have this ink color.
    let boxes: Vec<_> = reshiki::scene::primitives(&before)
        .iter()
        .flat_map(|primitive| match primitive {
            Primitive::Text {
                position,
                text,
                size,
                color,
                style,
            } if *color == ink => reshiki::style::text_ink_boxes(text, *size, style)
                .into_iter()
                .map(|(lo, hi)| (position.offset(lo.x, lo.y), position.offset(hi.x, hi.y)))
                .collect(),
            _ => vec![],
        })
        .collect();
    assert_eq!(boxes.len(), 3);
    let lo = Point::new(
        boxes.iter().map(|(lo, _)| lo.x).reduce(f32::min).unwrap(),
        boxes.iter().map(|(lo, _)| lo.y).reduce(f32::min).unwrap(),
    );
    let hi = Point::new(
        boxes.iter().map(|(_, hi)| hi.x).reduce(f32::max).unwrap(),
        boxes.iter().map(|(_, hi)| hi.y).reduce(f32::max).unwrap(),
    );
    let expected = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    assert!(expected.distance(Point::default()) > 0.1);
    assert!(expected.distance(Point::new(0., 2.)) > 0.1);
    for action in [shortcuts::Action::Join, shortcuts::Action::MergeAtoms] {
        app.tab.doc = before.clone();
        app.tab.selected = vec![1, 3, 5];
        let _ = app.update(Message::Shortcut(action));
        assert!(!app.error, "{}", app.status);
        assert!(app.tab.doc.atom(1).unwrap().position.distance(expected) < 0.0001);
        assert_eq!(app.tab.selected, [1]);
        assert_eq!(
            app.tab.doc.atom(1).unwrap().text_style,
            before.atom(1).unwrap().text_style
        );
        assert_eq!(app.tab.doc.annotations, before.annotations);
        for id in [2, 4, 6] {
            assert_eq!(
                app.tab.doc.atom(id).unwrap().position,
                before.atom(id).unwrap().position
            );
        }
    }
}

#[test]
fn hidden_selected_labels_use_atom_bounds_without_terminal_arms() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.selected.clear();
    for (position, end) in [
        (Point::new(0., 0.), Point::new(-500., -500.)),
        (Point::new(20., 0.), Point::new(800., -500.)),
        (Point::new(0., 60.), Point::new(0., 1000.)),
    ] {
        let id = app.tab.doc.add_atom("C", position);
        let terminal = app.tab.doc.add_atom("C", end);
        app.tab.doc.add_bond(id, terminal, 1, "plain");
        app.tab.selected.push(id);
        assert!(
            reshiki::scene::atom_label_ink_bounds(app.tab.doc.atom(id).unwrap(), &app.tab.doc)
                .is_none()
        );
    }
    let (joined, _) = app.merge_selected_atoms().unwrap();
    assert_eq!(joined.atom(1).unwrap().position, Point::new(10., 30.));
}

#[test]
fn native_fixture_visual_center_uses_recomputed_labels() {
    use reshiki::atom_labels::refresh::Refresh;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/join-three-atoms-before.rsk"
    ))
    .unwrap();
    reshiki::atom_labels::clear_computed(&mut app.tab.doc);
    let refreshed = Refresh::calculate(&app.tab.doc, &Refresh::default()).unwrap();
    assert!(refreshed.notice.is_none(), "{:?}", refreshed.notice);
    refreshed.apply(&mut app.tab.doc);
    app.tab.selected = vec![1, 3, 5];
    assert_eq!(app.tab.doc.atom(1).unwrap().label_h, 2);
    assert_eq!(app.tab.doc.atom(5).unwrap().label_h, 1);
    let (geometry, extents): (Vec<_>, Vec<_>) = app
        .tab
        .selected
        .iter()
        .map(|id| {
            let atom = app.tab.doc.atom(*id).unwrap();
            let ink = reshiki::scene::atom_label_ink_bounds(atom, &app.tab.doc);
            let effective = ink.unwrap_or((atom.position, atom.position));
            (
                serde_json::json!({
                    "id": id, "element": atom.element, "position": atom.position,
                    "label_h": atom.label_h, "label_ink_bounds": ink,
                    "effective_bounds": effective,
                    "point_fallback": ink.is_none(),
                }),
                effective,
            )
        })
        .unzip();
    let (lo, hi) = extents
        .into_iter()
        .reduce(|(a, b), (lo, hi)| {
            (
                Point::new(a.x.min(lo.x), a.y.min(lo.y)),
                Point::new(b.x.max(hi.x), b.y.max(hi.y)),
            )
        })
        .unwrap();
    let union_midpoint = Point::new(
        ((f64::from(lo.x) + f64::from(hi.x)) / 2.) as f32,
        ((f64::from(lo.y) + f64::from(hi.y)) / 2.) as f32,
    );
    let (joined, ids) = app.merge_selected_atoms().unwrap();
    let center = joined.atom(1).unwrap().position;
    assert_eq!(ids, [1]);
    assert_eq!(center, union_midpoint);
    assert_eq!(
        app.join_shortcut().unwrap().0.atom(1).unwrap().position,
        center
    );
    assert_ne!(center, Point::default());
    println!(
        "JOIN_NATIVE_FIXTURE_EXPECTED_GEOMETRY={}",
        serde_json::json!({
            "fixture": "tests/fixtures/join-three-atoms-before.rsk",
            "label_state": "normal native load: clear computed, Refresh::calculate, Refresh::apply",
            "selected_ids": app.tab.selected, "selected_geometry": geometry,
            "selected_visual_union_bounds": (lo, hi),
            "selected_visual_union_midpoint": union_midpoint,
            "selected_node_arithmetic_mean": Point::default(),
            "selected_node_geometric_bounds_center": Point::new(0., 2.),
            "app_visual_bounds_center": center, "expected_survivor_id": 1,
            "expected_survivor_position": center, "expected_atom_count": 4,
            "expected_bond_count": 3,
        })
    );
}
