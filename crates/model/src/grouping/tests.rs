//! Clicking into grouped objects and dragging a piece out of them.
use super::Container;
use crate::document::{Annotation, Document, Point};
use crate::graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, panel::Panel};
use crate::typography::TextFormat;

fn text(doc: &mut Document, at: Point, text: &str) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        id,
        position: at,
        text: text.into(),
        format: TextFormat {
            style: doc.drawing_style.text_style(),
            ..Default::default()
        },
    });
    id
}

fn molecule(doc: &mut Document, x: f32, y: f32) -> Vec<u64> {
    let a = doc.add_atom("C", Point::new(x, y));
    let b = doc.add_atom("C", Point::new(x + 20., y));
    doc.add_bond(a, b, 1, "plain");
    vec![a, b]
}

/// A cycle of three texts, grouped with its circle.
fn cycle(doc: &mut Document) -> (u64, [u64; 3]) {
    let a = text(doc, Point::new(0., -200.), "Pd(0)L2");
    let b = text(doc, Point::new(170., 90.), "Ar–Pd(II)L2–X");
    let c = text(doc, Point::new(-230., 90.), "Ar–Pd(II)L2–Ar′");
    let (id, _) = crate::cycles::make(doc, &[a, b, c]).unwrap();
    (id, [a, b, c])
}

/// A panel from (0, 0) to `size`, grouped with `held`.
fn panel(doc: &mut Document, size: Point, held: &[u64]) -> u64 {
    let id = doc.next_id();
    let mut g = Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        Point::new(0., 0.),
        size,
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    g.panel = Some(Panel::default());
    doc.graphics.push(g);
    let mut members = held.to_vec();
    members.push(id);
    doc.group_selection(&members).unwrap();
    id
}

fn sorted(mut ids: Vec<u64>) -> Vec<u64> {
    ids.sort_unstable();
    ids
}

fn move_by(doc: &mut Document, ids: &[u64], dx: f32, dy: f32) -> Vec<Container> {
    doc.translate(ids, dx, dy);
    doc.take_out(ids, Point::new(dx, dy))
}

#[test]
fn a_first_click_picks_the_cycle_and_a_second_the_part_under_the_pointer() {
    let mut doc = Document::default();
    let (id, [a, b, _]) = cycle(&mut doc);
    let whole = doc.click_selection(&[], &[a]);
    assert!(whole.contains(&id) && whole.contains(&b));
    assert_eq!(doc.click_selection(&whole, &[a]), vec![a]);
    // The part stays picked; another part of the same cycle is picked alone.
    assert_eq!(doc.click_selection(&[a], &[a]), vec![a]);
    assert_eq!(doc.click_selection(&[a], &[b]), vec![b]);
    // Something else selected first: the click picks the whole cycle again.
    let other = text(&mut doc, Point::new(900., 0.), "elsewhere");
    assert_eq!(sorted(doc.click_selection(&[other], &[a])), sorted(whole));
}

#[test]
fn clicks_go_one_level_in_each_time_from_a_group_to_a_molecule_to_a_bond() {
    let mut doc = Document::default();
    let m = molecule(&mut doc, 0., 0.);
    let caption = text(&mut doc, Point::new(0., 60.), "1");
    let mut members = m.clone();
    members.push(caption);
    doc.group_selection(&members).unwrap();
    let whole = doc.click_selection(&[], &m);
    assert_eq!(sorted(whole.clone()), sorted(members));
    assert_eq!(
        sorted(doc.click_selection(&whole, &[m[0]])),
        sorted(m.clone())
    );
    assert_eq!(doc.click_selection(&m, &[m[0]]), vec![m[0]]);
    // An ungrouped atom is picked as before.
    let loose = molecule(&mut doc, 300., 0.);
    assert_eq!(doc.click_selection(&loose, &[loose[1]]), vec![loose[1]]);
}

#[test]
fn a_part_moved_inside_its_cycle_stays_and_one_dropped_outside_leaves() {
    let mut doc = Document::default();
    let (id, [a, b, c]) = cycle(&mut doc);
    assert!(move_by(&mut doc, &[a], 30., 20.).is_empty());
    assert!(crate::cycles::role_of(&doc, a).is_some());
    let left = move_by(&mut doc, &[a], 0., -400.);
    assert_eq!(left, vec![Container::Cycle]);
    assert!(crate::cycles::role_of(&doc, a).is_none());
    assert!(!doc.groups.iter().any(|g| g.members.contains(&a)));
    // The rest stays a cycle, grouped with its circle.
    let group = doc.groups.iter().find(|g| g.members.contains(&id)).unwrap();
    assert!(group.members.contains(&b) && group.members.contains(&c));
    doc.validate().unwrap();
}

#[test]
fn a_molecule_dragged_out_of_its_panel_leaves_it_and_one_inside_stays() {
    let mut doc = Document::default();
    let one = molecule(&mut doc, 40., 50.);
    let two = molecule(&mut doc, 120., 50.);
    let held: Vec<u64> = one.iter().chain(&two).copied().collect();
    let frame = panel(&mut doc, Point::new(200., 100.), &held);
    assert!(move_by(&mut doc, &one, 10., 10.).is_empty());
    assert_eq!(move_by(&mut doc, &one, 0., 300.), vec![Container::Panel]);
    let group = doc
        .groups
        .iter()
        .find(|g| g.members.contains(&frame))
        .unwrap();
    assert!(one.iter().all(|id| !group.members.contains(id)));
    assert!(two.iter().all(|id| group.members.contains(id)));
    doc.validate().unwrap();
}

#[test]
fn a_piece_leaves_a_plain_group_only_once_its_middle_clears_the_groups_old_box() {
    let mut doc = Document::default();
    let m = molecule(&mut doc, 0., 0.);
    let caption = text(&mut doc, Point::new(0., 40.), "1");
    let mut members = m.clone();
    members.push(caption);
    doc.group_selection(&members).unwrap();
    assert!(move_by(&mut doc, &[caption], 5., 0.).is_empty());
    assert_eq!(
        move_by(&mut doc, &[caption], 0., 200.),
        vec![Container::Group]
    );
    assert!(!doc.groups.iter().any(|g| g.members.contains(&caption)));
    // Part of a molecule never leaves.
    let m2 = molecule(&mut doc, 500., 0.);
    let t = text(&mut doc, Point::new(500., 40.), "2");
    doc.group_selection(&[m2[0], m2[1], t]).unwrap();
    assert!(move_by(&mut doc, &[m2[0]], 0., 300.).is_empty());
}

#[test]
fn leaving_a_cycle_inside_a_panel_keeps_the_part_in_the_panel() {
    let mut doc = Document::default();
    let (id, [a, ..]) = cycle(&mut doc);
    doc.translate(&doc.expand_groups(&[id]), 600., 600.);
    let held = doc.expand_groups(&[id]);
    let frame = panel(&mut doc, Point::new(1200., 1200.), &held);
    // Out of the cycle's reach, still inside the frame.
    let (center, radius) = doc
        .graphics
        .iter()
        .find(|g| g.id == id)
        .unwrap()
        .cycle_circle();
    let ink = crate::scene::selection_bounds(&doc, &[a]).unwrap();
    let mid = ink.0.lerp(ink.1, 0.5);
    let reach = radius + 4. * doc.drawing_style.bond_length_world + 20.;
    let dy = center.y - reach - mid.y;
    assert!(center.y - reach > 30., "the test frame is too small");
    assert_eq!(move_by(&mut doc, &[a], 0., dy), vec![Container::Cycle]);
    let outer = doc
        .groups
        .iter()
        .find(|g| g.members.contains(&frame))
        .unwrap();
    assert!(outer.members.contains(&a));
    doc.validate().unwrap();
}

#[test]
fn a_second_click_picks_one_molecule_of_a_panel_and_an_assembly_whole_first() {
    let mut doc = Document::default();
    let one = molecule(&mut doc, 40., 50.);
    let two = molecule(&mut doc, 120., 50.);
    let held: Vec<u64> = one.iter().chain(&two).copied().collect();
    let frame = panel(&mut doc, Point::new(200., 100.), &held);
    let whole = doc.click_selection(&[], &[one[0]]);
    assert!(whole.contains(&frame) && whole.contains(&two[0]));
    assert_eq!(sorted(doc.click_selection(&whole, &[one[0]])), sorted(one));
}

#[test]
fn a_selection_that_cuts_across_groups_never_leaves_them() {
    let mut doc = Document::default();
    let [a, b, c, d] = [0., 100., 200., 300.].map(|x| text(&mut doc, Point::new(x, 0.), "x"));
    doc.group_selection(&[a, b]).unwrap();
    doc.group_selection(&[a, b, c, d]).unwrap();
    assert!(move_by(&mut doc, &[a, c], 0., 900.).is_empty());
    doc.validate().unwrap();
}

#[test]
fn a_part_within_its_cycle_stays_even_outside_a_panel_sharing_the_group() {
    let mut doc = Document::default();
    let (id, [a, ..]) = cycle(&mut doc);
    // A small panel in the cycle's own group, nowhere near the part.
    let frame = doc.next_id();
    let mut g = Graphic::dragged(
        frame,
        GraphicKind::Rectangle,
        Point::new(-20., -20.),
        Point::new(20., 20.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    g.panel = Some(Panel::default());
    doc.graphics.push(g);
    let group = doc
        .groups
        .iter_mut()
        .find(|g| g.members.contains(&id))
        .unwrap();
    group.members.push(frame);
    assert!(move_by(&mut doc, &[a], 10., 0.).is_empty());
    assert!(crate::cycles::role_of(&doc, a).is_some());
    assert!(doc.groups.iter().any(|g| g.members.contains(&a)));
}

#[test]
fn a_grouped_piece_dragged_with_a_loose_object_stays() {
    let mut doc = Document::default();
    let [a, b, loose] = [0., 100., 600.].map(|x| text(&mut doc, Point::new(x, 0.), "x"));
    doc.group_selection(&[a, b]).unwrap();
    assert!(move_by(&mut doc, &[a, loose], 0., 900.).is_empty());
    assert!(doc.groups.iter().any(|g| g.members.contains(&a)));
}

#[test]
fn clicks_reach_an_atom_of_a_cycle_part() {
    let mut doc = Document::default();
    let m = molecule(&mut doc, 0., -200.);
    let b = text(&mut doc, Point::new(170., 90.), "B");
    let c = text(&mut doc, Point::new(-230., 90.), "C");
    crate::cycles::make(&mut doc, &[m[0], m[1], b, c]).unwrap();
    let whole = doc.click_selection(&[], &[m[0]]);
    let part = doc.click_selection(&whole, &[m[0]]);
    assert_eq!(sorted(part.clone()), sorted(m.clone()));
    assert_eq!(doc.click_selection(&part, &[m[0]]), vec![m[0]]);
}

#[test]
fn a_cycle_circle_dragged_alone_keeps_its_parts_group() {
    let mut doc = Document::default();
    let (id, [a, ..]) = cycle(&mut doc);
    assert!(move_by(&mut doc, &[id], 0., 2000.).is_empty());
    let group = doc.groups.iter().find(|g| g.members.contains(&id)).unwrap();
    assert!(group.members.contains(&a));
}
