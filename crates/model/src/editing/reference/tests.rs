use super::*;
use crate::graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle};

fn branch(angle: f64) -> Document {
    let mut doc = Document::default();
    let (s, c) = angle.to_radians().sin_cos();
    let a = doc.add_atom("C", Point::new(120., -80.));
    let b = doc.add_atom(
        "C",
        Point::new(120. + (42. * c) as f32, -80. + (42. * s) as f32),
    );
    let p = doc.atom(b).unwrap().position;
    let o = doc.add_atom("O", p.offset(21., -36.373_066));
    doc.add_bond(a, b, 1, "wedge");
    doc.add_bond(b, o, 1, "plain");
    doc.add_atom("N", Point::new(400., 300.));
    doc
}
fn angle(doc: &Document, a: u64, b: u64) -> f64 {
    let a = doc.atom(a).unwrap().position;
    let b = doc.atom(b).unwrap().position;
    (f64::from(b.y) - f64::from(a.y))
        .atan2(f64::from(b.x) - f64::from(a.x))
        .to_degrees()
}
fn near(actual: Point, expected: Point) {
    assert!(
        actual.distance(expected) < 0.0003,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn arbitrary_alignment_is_absolute_and_preserves_wedge_order_and_depth() {
    for start in [17.3, -82.7, 143.5, 179.9] {
        for (target, directed) in [
            (0., false),
            (90., false),
            (0., true),
            (-90., true),
            (23.7, true),
        ] {
            let mut doc = branch(start);
            doc.atom_mut(2).unwrap().depth = 13.;
            let pivot = doc.atom(1).unwrap().position;
            let ids = scope(&doc, &[1, 2], true).unwrap();
            assert_eq!(ids, vec![1, 2, 3]);
            let degrees = alignment_degrees(&doc, Edge::Bond(1, 2), target, directed).unwrap();
            let (after, selected) = rotate(&doc, &ids, pivot, degrees, false).unwrap();
            assert_eq!(selected, ids);
            let period = if directed { 360. } else { 180. };
            let error =
                (angle(&after, 1, 2) - target + period / 2.).rem_euclid(period) - period / 2.;
            assert!(error.abs() < 0.0001, "{start} → {target}: {error}");
            assert_eq!(after.bonds, doc.bonds);
            assert_eq!(after.atom(2).unwrap().depth, 13.);
            assert_eq!(after.atom(1), doc.atom(1));
            assert_eq!(after.atom(4), doc.atom(4));
            assert_eq!(
                alignment_degrees(&after, Edge::Bond(1, 2), target, directed).unwrap(),
                0.
            );
        }
    }
}

#[test]
fn axis_alignment_keeps_nearest_direction_and_directed_alignment_can_reverse_the_axis() {
    let doc = branch(170.);
    assert!((alignment_degrees(&doc, Edge::Bond(1, 2), 0., false).unwrap() - 10.).abs() < 0.0001);
    assert!((alignment_degrees(&doc, Edge::Bond(1, 2), 0., true).unwrap() + 170.).abs() < 0.0001);
    assert!(alignment_degrees(&doc, Edge::Bond(2, 1), 0., false).is_err());
}

#[test]
fn a_straight_graphic_edge_aligns_without_replacing_its_parametric_shape() {
    let mut doc = Document::default();
    let mut graphic = Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        Point::new(10., 20.),
        Point::new(80., 60.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    graphic.map_positions(|p| Point::new(p.x * 0.9 - p.y * 0.3, p.x * 0.3 + p.y * 0.9));
    doc.graphics.push(graphic);
    let edge = edges(&doc, &[1])[0];
    let delta = alignment_degrees(&doc, edge, 90., false).unwrap();
    let (after, _) = rotate(&doc, &[1], Point::default(), delta, false).unwrap();
    let (a, b) = edge.endpoints(&after).unwrap();
    assert!((a.x - b.x).abs() < 0.0001);
    assert_eq!(after.graphics[0].kind, GraphicKind::Rectangle);
}

#[test]
fn pinned_quarter_turn_copies_use_the_same_center_and_leave_originals_unchanged() {
    let original = branch(17.3);
    let pivot = Point::new(50., 40.);
    let ids = vec![1, 2, 3];
    let mut doc = original.clone();
    for degrees in [90., 180., 270.] {
        let (after, copied) = rotate(&doc, &ids, pivot, degrees, true).unwrap();
        assert_eq!(copied.len(), 3);
        let (s, c) = (degrees as f64).to_radians().sin_cos();
        for (from, to) in ids.iter().zip(&copied) {
            let before = original.atom(*from).unwrap().position;
            let x = f64::from(before.x - pivot.x);
            let y = f64::from(before.y - pivot.y);
            near(
                after.atom(*to).unwrap().position,
                Point::new(
                    (f64::from(pivot.x) + x * c - y * s) as f32,
                    (f64::from(pivot.y) + x * s + y * c) as f32,
                ),
            );
        }
        assert_eq!(after.atoms.get(..4), original.atoms.get(..4));
        assert_eq!(after.bonds.get(..2), original.bonds.get(..2));
        doc = after;
    }
    assert_eq!(doc.atoms.len(), 13);
    let json = doc.file_json().unwrap();
    let reopened = Document::from_native_file(&json).unwrap();
    assert_eq!(reopened.atoms, doc.atoms);
    assert_eq!(reopened.bonds, doc.bonds);
}

#[test]
fn partial_rotation_and_invalid_references_are_rejected_without_mutation() {
    let doc = branch(17.3);
    assert!(scope(&doc, &[1, 2], false).is_err());
    assert!(rotate(&doc, &[1, 2], Point::default(), 17.3, false).is_err());
    assert!(rotate(&doc, &[1, 2, 3], Point::new(f32::NAN, 0.), 17.3, false).is_err());
    assert!(alignment_degrees(&doc, Edge::Bond(99, 2), 0., false).is_err());
}

#[test]
fn repeated_arbitrary_inverse_turns_and_quarter_turns_keep_a_large_pinned_pivot_stable() {
    let mut original = branch(17.3);
    original.translate(&[1, 2, 3], 10_000., -7_000.);
    let pivot = original.atom(1).unwrap().position;
    let ids = vec![1, 2, 3];
    let mut doc = original.clone();
    for _ in 0..12 {
        for degrees in [17.3, -17.3, 90., -90.] {
            (doc, _) = rotate(&doc, &ids, pivot, degrees, false).unwrap();
            assert_eq!(doc.atom(1).unwrap().position, pivot);
        }
        for id in &ids {
            assert!(
                doc.atom(*id)
                    .unwrap()
                    .position
                    .distance(original.atom(*id).unwrap().position)
                    < 0.004,
                "Stored f32 coordinate precision must not accumulate visible pivot drift"
            );
        }
    }
    assert_eq!(doc.bonds, original.bonds);
}

#[test]
fn stretch_preserves_arbitrary_original_axis_and_translates_the_whole_branch() {
    for degrees in [17.3, -82.7, 143.5] {
        let doc = branch(degrees);
        let plan = Stretch::new(&doc, 1, 2).unwrap();
        assert_eq!(plan.ids, vec![2, 3]);
        let before = doc.atom(2).unwrap().position;
        let internal = doc.atom(3).unwrap().position.offset(-before.x, -before.y);
        let (s, c) = degrees.to_radians().sin_cos();
        // A perpendicular component cannot affect the committed length.
        let target = plan.dragged_length(Point::new(
            (21. * c - 73. * s) as f32,
            (21. * s + 73. * c) as f32,
        ));
        assert!((target - 63.).abs() < 0.0001);
        let after = plan.apply(&doc, target).unwrap();
        assert!((angle(&after, 1, 2) - angle(&doc, 1, 2)).abs() < 0.0001);
        assert!(
            (after
                .atom(1)
                .unwrap()
                .position
                .distance(after.atom(2).unwrap().position)
                - 63.)
                .abs()
                < 0.0001
        );
        let moved = after.atom(2).unwrap().position;
        near(
            after.atom(3).unwrap().position.offset(-moved.x, -moved.y),
            internal,
        );
        assert_eq!(after.atom(1), doc.atom(1));
        assert_eq!(after.atom(4), doc.atom(4));
        assert_eq!(after.bonds, doc.bonds);
        let native = Document::from_native_file(&after.file_json().unwrap()).unwrap();
        assert_eq!(native.atoms, after.atoms);
    }
}

#[test]
fn stretching_can_pin_either_end_without_reversing_bond_or_merging_a_nearby_atom() {
    let mut doc = branch(0.);
    doc.atom_mut(4).unwrap().position = Point::new(204., -80.);
    let plan = Stretch::new(&doc, 1, 2).unwrap();
    let after = plan.apply(&doc, 84.).unwrap();
    assert_eq!(after.atoms.len(), doc.atoms.len());
    assert_eq!(after.bonds, doc.bonds);
    assert_eq!(after.atom(4), doc.atom(4));
    let reverse = Stretch::new(&doc, 2, 1).unwrap();
    assert_eq!(reverse.ids, vec![1]);
    let after = reverse.apply(&doc, 63.).unwrap();
    assert_eq!(after.atom(2), doc.atom(2));
    assert_eq!(after.atom(3), doc.atom(3));
    assert_eq!(after.bonds, doc.bonds);
}

#[test]
fn crossing_the_fixed_end_clamps_and_ring_stretch_is_unavailable() {
    let mut doc = branch(0.);
    let plan = Stretch::new(&doc, 1, 2).unwrap();
    assert_eq!(plan.dragged_length(Point::new(-1000., 0.)), plan.minimum);
    assert!(plan.delta(0.).is_err());
    assert!(plan.delta(f32::NAN).is_err());
    let short = plan.apply(&doc, plan.minimum).unwrap();
    assert!(short.atom(2).unwrap().position.x > short.atom(1).unwrap().position.x);
    doc.add_bond(3, 1, 1, "plain");
    assert!(Stretch::new(&doc, 1, 2).unwrap_err().contains("ring bond"));
}

#[test]
fn a_branch_sharing_a_derived_attachment_with_fixed_atoms_is_rejected() {
    let mut doc = branch(17.3);
    crate::projection::add_centroid(&mut doc, &[1, 2]).unwrap();
    let before = doc.clone();
    assert!(
        Stretch::new(&doc, 1, 2)
            .unwrap_err()
            .contains("derived attachment")
    );
    assert_eq!(doc, before);
}

#[test]
fn alignment_scope_carries_a_derived_anchor_and_its_attached_fragment() {
    let mut doc = branch(17.3);
    let marker = crate::projection::add_centroid(&mut doc, &[1, 2]).unwrap();
    doc.add_bond(marker, 4, 0, "dotted");
    assert!(scope(&doc, &[1, 2, 3], false).is_err());
    let ids = scope(&doc, &[1, 2], true).unwrap();
    assert_eq!(ids, doc.all_ids());
    let delta = alignment_degrees(&doc, Edge::Bond(1, 2), 0., false).unwrap();
    let (after, _) = rotate(&doc, &ids, Point::new(50., 40.), delta, false).unwrap();
    assert_eq!(after.bonds, doc.bonds);
    let before_length = doc
        .atom(marker)
        .unwrap()
        .position
        .distance(doc.atom(4).unwrap().position);
    let after_length = after
        .atom(marker)
        .unwrap()
        .position
        .distance(after.atom(4).unwrap().position);
    assert!((after_length - before_length).abs() < 0.0001);
}
