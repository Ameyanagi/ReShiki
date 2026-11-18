use super::{Leader, Target, add_callout, reconcile, remap};
use crate::{
    document::{DRAWING_ELEMENTS, Document, Point},
    graphics::GraphicKind,
};
use std::collections::HashMap;

fn ethane() -> (Document, u64, u64) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(20., 0.));
    doc.add_bond(a, b, 1, "plain");
    (doc, a, b)
}

fn leader(doc: &Document) -> &crate::graphics::Graphic {
    doc.graphics.iter().find(|g| g.leader.is_some()).unwrap()
}

#[test]
fn callout_on_a_bond_adds_a_card_and_a_leader_to_its_middle() {
    let (mut doc, a, b) = ethane();
    let text = add_callout(
        &mut doc,
        Target::Bond {
            a,
            b,
            fraction: 0.5,
        },
        "New C–C bond",
    )
    .unwrap();
    doc.validate().unwrap();
    assert!(doc.annotations.iter().any(|t| t.id == text));
    let line = leader(&doc);
    assert_eq!(line.kind, GraphicKind::Line);
    let [start, end] = line.line_ends();
    assert!(end.distance(Point::new(10., 0.)) < 1e-3);
    // The leader leaves the card from its edge, not its corner or middle.
    let (lo, hi) = crate::scene::selection_bounds(&doc, &[text]).unwrap();
    assert!(start.y > hi.y && start.x >= lo.x - 3. && start.x <= hi.x + 3.);
    assert!(line.leader_dot().is_some());
    assert_eq!(doc.file_version(), DRAWING_ELEMENTS);
}

#[test]
fn leader_follows_the_molecule_and_the_card() {
    let (mut doc, a, b) = ethane();
    let text = add_callout(&mut doc, Target::Atom { atom: b }, "Here").unwrap();
    for atom in &mut doc.atoms {
        atom.position = atom.position.offset(30., 40.);
    }
    reconcile(&mut doc);
    assert_eq!(leader(&doc).line_ends()[1], Point::new(50., 40.));
    let before = leader(&doc).line_ends()[0];
    doc.annotations
        .iter_mut()
        .find(|t| t.id == text)
        .unwrap()
        .position
        .x += 25.;
    reconcile(&mut doc);
    assert!(leader(&doc).line_ends()[0].x > before.x + 10.);
    let _ = a;
}

#[test]
fn deleting_the_card_removes_the_leader_and_a_lost_target_frees_its_end() {
    let (mut doc, _, b) = ethane();
    let text = add_callout(&mut doc, Target::Molecule { atom: b }, "Ethane").unwrap();
    doc.atoms.clear();
    doc.bonds.clear();
    reconcile(&mut doc);
    let line = leader(&doc);
    assert_eq!(line.leader.as_ref().unwrap().end, None);
    doc.annotations.retain(|t| t.id != text);
    reconcile(&mut doc);
    assert!(doc.graphics.is_empty());
}

#[test]
fn copying_a_leader_without_its_target_frees_that_end() {
    let (mut doc, _, b) = ethane();
    let text = add_callout(&mut doc, Target::Atom { atom: b }, "Here").unwrap();
    let mut line = leader(&doc).clone();
    remap(&mut line, &HashMap::from([(text, 90)]));
    assert_eq!(
        line.leader,
        Some(Leader {
            start: Some(Target::Object { id: 90 }),
            end: None,
        })
    );
}

#[test]
fn moving_only_the_leader_detaches_it() {
    let (mut doc, _, b) = ethane();
    add_callout(&mut doc, Target::Atom { atom: b }, "Here").unwrap();
    let id = leader(&doc).id;
    super::transformed(&mut doc, |object| (object == id).then_some(0));
    assert!(doc.graphics[0].leader.is_none());
}

#[test]
fn dragging_a_leader_end_releases_it_and_keeps_a_line() {
    let (mut doc, _, b) = ethane();
    add_callout(&mut doc, Target::Atom { atom: b }, "Here").unwrap();
    let line = doc
        .graphics
        .iter_mut()
        .find(|g| g.leader.is_some())
        .unwrap();
    line.edit_point(1, Point::new(60., 60.)).unwrap();
    assert_eq!(line.kind, GraphicKind::Line);
    assert_eq!(line.line_ends()[1], Point::new(60., 60.));
    assert!(line.leader.as_ref().unwrap().end.is_none());
    doc.validate().unwrap();
}

#[test]
fn a_removed_callout_leaves_no_group_behind() {
    let (mut doc, _, b) = ethane();
    let text = add_callout(&mut doc, Target::Atom { atom: b }, "Here").unwrap();
    let line = leader(&doc).id;
    let box_id = doc.next_id();
    doc.graphics.push(crate::graphics::Graphic::dragged(
        box_id,
        GraphicKind::Rectangle,
        Point::new(0., 0.),
        Point::new(5., 5.),
        Default::default(),
        Default::default(),
        false,
    ));
    doc.groups.push(crate::grouping::Group {
        id: box_id + 1,
        members: vec![line, box_id],
        integral: false,
    });
    doc.validate().unwrap();
    doc.delete(&[text]);
    doc.validate().unwrap();
    assert!(doc.graphics.iter().all(|g| g.id != line));
}

#[test]
fn a_join_moves_an_end_to_the_atom_that_remains() {
    let (mut doc, a, b) = ethane();
    add_callout(&mut doc, Target::Atom { atom: b }, "Here").unwrap();
    super::remap_join(&mut doc, &HashMap::from([(a, a), (b, a)]));
    assert_eq!(
        leader(&doc).leader.as_ref().unwrap().end,
        Some(Target::Atom { atom: a })
    );
}
