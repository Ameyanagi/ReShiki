use super::*;
use reshiki::{
    cycles::{self, Shape},
    document::Arrow,
    graphics::{Graphic, GraphicKind},
};

fn elongated(shape: Shape) -> (App, u64, Point) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Ellipse,
        Point::new(-1500., -100.),
        Point::new(1500., 100.),
        Default::default(),
        Default::default(),
        false,
    );
    g.cycle = Some(Box::new(Cycle {
        shape,
        ..Default::default()
    }));
    let outline = cycles::outline(&g).unwrap();
    app.tab.doc.graphics.push(g);
    let mut intermediates = vec![];
    for i in 0..4 {
        let at = outline
            .point(outline.length() * (2 * i + 1) as f64 / 8.)
            .unwrap();
        let id = text(&mut app, at.x, at.y, &format!("Intermediate {i}"));
        let a = app.tab.doc.annotations.last_mut().unwrap();
        let (w, h) = a.size();
        a.position = at.offset(-w * 0.5, -h * 0.5);
        intermediates.push(vec![id]);
    }
    let c = app.tab.doc.graphics[0].cycle.as_mut().unwrap();
    c.intermediates = intermediates;
    c.steps = vec![cycles::Step::default(); 4];
    c.steps[3].name = "Retained oxidative addition".into();
    c.steps[3].lead_pt = 7.;
    c.steps[3].trail_pt = 11.;
    c.steps[3].bow_pt = 12.;
    c.steps[3].reversible = true;
    let ids = app.tab.doc.all_ids();
    app.tab.doc.group_selection(&ids).unwrap();
    app.tab.doc.validate().unwrap();
    let g = &app.tab.doc.graphics[0];
    let drawn = cycles::drawn(&app.tab.doc, g);
    let forward = drawn
        .arrows
        .iter()
        .zip(&drawn.arcs)
        .find(|(_, step)| **step == Some(3))
        .map(|(arrow, _)| arrow)
        .unwrap();
    let (center, radius) = g.cycle_circle();
    let tip = (0..=512)
        .map(|i| forward.point(i as f32 / 512.))
        .max_by(|a, b| a.distance(center).total_cmp(&b.distance(center)))
        .unwrap();
    let c = g.cycle.as_ref().unwrap();
    let old_reach = radius
        + 4. * app.tab.doc.drawing_style.bond_length_world
        + reshiki::style::DEFAULT.world(c.steps[3].bow_pt.abs());
    assert!(tip.distance(center) > old_reach + 100., "{shape:?}");
    assert!(
        forward.hit(tip, 0.6 * app.tab.doc.drawing_style.bond_length_world),
        "the test drop is on the drawn shaft"
    );
    (app, 1, tip)
}

fn named_ethane(doc: &mut Document, at: Point, two_decimals: bool) -> (Vec<u64>, [u64; 2]) {
    let a = doc.add_atom("C", at);
    let b = doc.add_atom("C", at.offset(42., 10.));
    doc.add_bond(a, b, 1, "plain");
    doc.atom_mut(a).unwrap().map_num = 11;
    doc.atom_mut(b).unwrap().map_num = 12;
    let atoms = vec![a, b];
    let name = reshiki::molecule_names::show(
        doc,
        &atoms,
        "ethane".into(),
        "CC".into(),
        Default::default(),
        false,
    )
    .unwrap();
    reshiki::linked_properties::toggle(
        doc,
        &atoms,
        reshiki::linked_properties::Property::Formula,
        Default::default(),
        two_decimals,
    )
    .unwrap();
    let formula = doc.property_captions.last().unwrap().annotation;
    (atoms, [name, formula])
}

fn middle(doc: &Document, ids: &[u64]) -> Point {
    let (lo, hi) = reshiki::scene::selection_bounds(doc, ids).unwrap();
    lo.lerp(hi, 0.5)
}

#[test]
fn moving_onto_an_elongated_painted_shape_inserts_with_captions_settings_and_one_undo() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        let (mut app, id, tip) = elongated(shape);
        let (atoms, captions) = named_ethane(
            &mut app.tab.doc,
            Point::new(4000., 2000.),
            app.appearance.two_decimal_weight,
        );
        let arrow = app.tab.doc.next_id();
        app.tab.doc.arrows.push(Arrow::new(
            arrow,
            Point::new(4200., 2000.),
            Point::new(4300., 2000.),
            Default::default(),
            Default::default(),
        ));
        reshiki::reactions::assign(
            &mut app.tab.doc,
            arrow,
            &atoms,
            reshiki::reactions::Role::Reactant,
        )
        .unwrap();
        app.tab.doc.validate().unwrap();
        let before = app.tab.doc.clone();
        let old_cycle = before.graphics[0].cycle.as_ref().unwrap();
        let from = middle(&before, &atoms);
        let delta = Point::new(tip.x - from.x, tip.y - from.y);
        let moved = reshiki::molecule_names::include_annotations(&before, &atoms);
        let frames = app.tab.history.undo_frames().len();
        let _ = app.update(Message::Canvas(Edit::Move(moved, delta.x, delta.y)));
        assert!(!app.error, "{shape:?}: {}", app.status);
        assert_eq!(
            cycles::role_of(&app.tab.doc, atoms[0]),
            Some((id, Role::Intermediate(4)))
        );
        let c = app.tab.doc.graphics[0].cycle.as_ref().unwrap();
        assert_eq!(c.shape, shape);
        assert_eq!(&c.intermediates[..4], old_cycle.intermediates.as_slice());
        assert_eq!(c.intermediates[4], atoms);
        let mut expected = old_cycle.steps.clone();
        let trail = expected[3].trail_pt;
        expected[3].trail_pt = 0.;
        expected.push(cycles::Step {
            trail_pt: trail,
            ..Default::default()
        });
        assert_eq!(c.steps, expected);
        assert_eq!(c.split_runs.len(), 1);
        assert!(
            app.tab.doc.groups.iter().any(|g| {
                g.members.contains(&id) && atoms.iter().all(|a| g.members.contains(a))
            })
        );
        for reaction in &app.tab.doc.reactions {
            for role in reshiki::reactions::Role::ALL {
                assert!(
                    reaction
                        .participants(role)
                        .iter()
                        .all(|p| { p.atoms.iter().all(|a| !atoms.contains(a)) })
                );
            }
        }
        assert_eq!(app.tab.doc.bonds, before.bonds);
        for atom in &atoms {
            let old = before.atom(*atom).unwrap();
            let new = app.tab.doc.atom(*atom).unwrap();
            assert!(new.position.distance(old.position.offset(delta.x, delta.y)) < 0.003);
            assert_eq!(new.map_num, old.map_num);
            assert_eq!(new.stereo, old.stereo);
        }
        for caption in captions {
            let old = before.annotations.iter().find(|a| a.id == caption).unwrap();
            let new = app
                .tab
                .doc
                .annotations
                .iter()
                .find(|a| a.id == caption)
                .unwrap();
            assert!(new.position.distance(old.position.offset(delta.x, delta.y)) < 0.003);
            assert_eq!(new.text, old.text);
            assert_eq!(new.format, old.format);
        }
        assert_eq!(app.tab.doc.molecule_names, before.molecule_names);
        assert_eq!(app.tab.doc.property_captions, before.property_captions);
        assert!(app.status.contains("left its reaction"), "{}", app.status);
        assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
        let applied = app.tab.doc.clone();
        let saved = applied.file_json().unwrap();
        assert_eq!(
            Document::from_json(&saved).unwrap().file_json().unwrap(),
            saved
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, applied);
    }
}

#[test]
fn native_paste_on_a_shaped_arc_stays_loose_until_a_later_drag() {
    let (mut app, id, tip) = elongated(Shape::Rectangle);
    let mut part = Document::default();
    let (source_atoms, _) = named_ethane(
        &mut part,
        Point::default(),
        app.appearance.two_decimal_weight,
    );
    let part_center = reshiki::editing::center(&part, &part.all_ids());
    let body_center = middle(&part, &source_atoms);
    app.viewport = iced::Size::new(1400., 1000.);
    app.tab.camera.zoom = 1.;
    // append_pasted places a small fragment at camera.center+(24,24),
    // measuring the whole fragment including linked caption ink.
    app.tab.camera.center = Point::new(
        tip.x - body_center.x + part_center.x - 24.,
        tip.y - body_center.y + part_center.y - 24.,
    );
    let before = app.tab.doc.clone();
    let frames = app.tab.history.undo_frames().len();
    let _ = app.update(Message::ClipboardRead {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        result: Box::new(Ok(reshiki::clipboard::PasteOutcome::native(part))),
    });
    assert!(!app.error, "{}", app.status);
    let atoms: Vec<_> = app
        .tab
        .selected
        .iter()
        .copied()
        .filter(|id| app.tab.doc.atom(*id).is_some())
        .collect();
    assert_eq!(atoms.len(), 2);
    assert!(middle(&app.tab.doc, &atoms).distance(tip) < 0.003);
    assert!(
        atoms
            .iter()
            .all(|a| cycles::role_of(&app.tab.doc, *a).is_none())
    );
    assert_eq!(app.tab.doc.graphics[0], before.graphics[0]);
    assert_eq!(app.tab.doc.groups, before.groups);
    assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
    let pasted = app.tab.doc.clone();
    let moved = reshiki::molecule_names::include_annotations(&pasted, &atoms);
    let _ = app.update(Message::Canvas(Edit::Move(moved, 0.25, -0.1)));
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        cycles::role_of(&app.tab.doc, atoms[0]),
        Some((id, Role::Intermediate(4)))
    );
    assert_eq!(app.tab.history.undo_frames().len(), frames + 2);
    let inserted = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, pasted, "undo the drag retains the loose paste");
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, inserted);
    let _ = app.update(Message::Undo);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}
