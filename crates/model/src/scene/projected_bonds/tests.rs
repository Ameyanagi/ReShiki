use super::*;
use crate::{bonds::DoublePosition, editing, projection, scene};

fn ring(degrees: f32) -> Document {
    let mut doc = Document::default();
    let ids = editing::ring(&mut doc, Point::default(), 6, false, 0.);
    for (index, bond) in doc.bonds.iter_mut().enumerate() {
        if index % 2 == 0 {
            bond.order = 2;
        }
    }
    projection::tilt(&mut doc, &ids, degrees, true);
    projection::tilt(&mut doc, &ids, 20., false);
    doc
}

fn secondary(doc: &Document, bond: &Bond) -> Option<(Point, Point)> {
    let side = match scene::effective_double_position(doc, bond) {
        DoublePosition::Left => -1.,
        DoublePosition::Right => 1.,
        _ => panic!("Projected ring lost its backbone"),
    };
    let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
    rail(
        doc,
        bond,
        doc.atom(bond.a).unwrap().position,
        doc.atom(bond.b).unwrap().position,
        spacing * side,
        spacing * 0.75,
        doc.drawing_style.line_width() / 2.,
    )
    .expect("Valid retained face")
}

fn assert_inside(face: &Face, p: Point, margin: f32) {
    let orientation = face
        .points
        .iter()
        .zip(face.points.iter().cycle().skip(1))
        .map(|(a, b)| cross_xy(*a, *b))
        .sum::<f32>()
        .signum();
    for (a, b) in face.points.iter().zip(face.points.iter().cycle().skip(1)) {
        let distance = orientation * cross_xy(minus(*b, *a), minus(p, *a)) / a.distance(*b);
        assert!(
            distance >= margin - 0.002,
            "Rail outside projected face: {distance}"
        );
    }
}

#[test]
fn tilted_ring_uses_foreshortened_xyz_offset_and_shared_scene_rails() {
    let doc = ring(70.);
    let original = doc.clone();
    let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
    let primitives = scene::primitives(&doc);
    let mut contracted = false;
    for bond in doc.bonds.iter().filter(|b| b.order == 2) {
        let face = face(&doc, bond).unwrap();
        let gap = spacing * cross_xy(face.tangent, face.inward).abs()
            / face.tangent.distance(Point::default());
        contracted |= gap < spacing * 0.6;
        let (first, last) = secondary(&doc, bond).unwrap();
        assert_inside(&face, first, doc.drawing_style.line_width() / 2.);
        assert_inside(&face, last, doc.drawing_style.line_width() / 2.);
        assert!(
            primitives
                .iter()
                .any(|p| matches!(p, scene::Primitive::Line(a,b,_)
            if a.distance(first) < 0.002 && b.distance(last) < 0.002)),
            "Shared scene failed to use the projected secondary rail"
        );
        // Its signed screen separation is the projected XYZ offset, not the
        // fixed spacing from the earlier renderer.
        let a = doc.atom(bond.a).unwrap().position;
        let b = doc.atom(bond.b).unwrap().position;
        let separation = cross_xy(minus(b, a), minus(first, a)).abs() / a.distance(b);
        assert!((separation - gap).abs() < 0.002);
        // An independent plane normal from three retained atom coordinates
        // gives the analytic orthographic gap d*|n.z|/|t.xy|.
        let a3 = xyz(doc.atom(bond.a).unwrap());
        let b3 = xyz(doc.atom(bond.b).unwrap());
        let normal = doc
            .atoms
            .iter()
            .find_map(|atom| unit(cross(sub(b3, a3), sub(xyz(atom), a3))))
            .unwrap();
        let [_, _, nz] = normal;
        let t = xy(unit(sub(b3, a3)).unwrap());
        let analytic = spacing * nz.abs() / t.distance(Point::default());
        assert!((separation - analytic).abs() < 0.002);
    }
    assert!(contracted, "Tilt must visibly foreshorten a secondary rail");
    assert_eq!(
        doc, original,
        "Rendering changed molecular coordinates or chemistry"
    );
}

#[test]
fn frozen_optimized_c60_and_c70_rails_remain_in_their_projected_faces() {
    for (fixture, count, doubles) in [
        (
            include_str!("../../../../../tests/fixtures/projected-double-bonds/c60.rsk"),
            60,
            30,
        ),
        (
            include_str!("../../../../../tests/fixtures/projected-double-bonds/c70.rsk"),
            70,
            35,
        ),
    ] {
        let doc: Document = serde_json::from_str(fixture).unwrap();
        doc.validate().unwrap();
        let original = doc.clone();
        assert_eq!(doc.atoms.len(), count);
        assert_eq!(doc.bonds.iter().filter(|b| b.order == 2).count(), doubles);
        let mut visible = 0;
        for bond in doc.bonds.iter().filter(|b| b.order == 2) {
            let face = face(&doc, bond).expect("Optimized cage local face");
            if let Some((first, last)) = secondary(&doc, bond) {
                visible += 1;
                assert_inside(&face, first, doc.drawing_style.line_width() / 2.);
                assert_inside(&face, last, doc.drawing_style.line_width() / 2.);
                let a = doc.atom(bond.a).unwrap().position;
                let b = doc.atom(bond.b).unwrap().position;
                let spacing =
                    doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
                assert!(first.distance(a) <= a.distance(b) + spacing);
                assert!(last.distance(b) <= a.distance(b) + spacing);
            }
        }
        assert!(
            visible >= doubles / 2,
            "Cage lost ordinary visible secondary rails"
        );
        assert!(!scene::svg(&doc).is_empty());
        assert_eq!(doc, original);
    }
}

#[test]
fn face_and_rail_are_stable_under_reordering_and_bond_reversal() {
    let doc: Document = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/projected-double-bonds/c70.rsk"
    ))
    .unwrap();
    let mut reordered = doc.clone();
    reordered.atoms.reverse();
    reordered.bonds.reverse();
    for bond in &mut reordered.bonds {
        bond.reverse();
    }
    for bond in doc.bonds.iter().filter(|b| b.order == 2) {
        let reversed = reordered
            .bonds
            .iter()
            .find(|b| b.a == bond.b && b.b == bond.a)
            .unwrap();
        assert_eq!(
            face(&doc, bond).unwrap().points,
            face(&reordered, reversed).unwrap().points
        );
        assert_eq!(
            scene::effective_double_position(&doc, bond).reversed(),
            scene::effective_double_position(&reordered, reversed)
        );
        match (secondary(&doc, bond), secondary(&reordered, reversed)) {
            (Some((a, b)), Some((c, d))) => {
                assert!(a.distance(d) < 0.002);
                assert!(b.distance(c) < 0.002);
            }
            (None, None) => {}
            _ => panic!("Reversal changed secondary visibility"),
        }
    }
}

#[test]
fn missing_depth_retains_2d_styles_and_edge_on_faces_do_not_restore_screen_spacing() {
    let mut planar = ring(0.);
    // The helper's depth translation alone must not turn a 2D diagram into
    // a projected face or alter manual positions and dashed/bold rails.
    for a in &mut planar.atoms {
        a.depth = 0.;
    }
    for position in [
        DoublePosition::Auto,
        DoublePosition::Left,
        DoublePosition::Right,
        DoublePosition::Center,
    ] {
        for bond in &mut planar.bonds {
            bond.double_position = position;
            bond.secondary_display = Some("dashed".into());
        }
        let before = scene::svg(&planar);
        for a in &mut planar.atoms {
            a.depth = 100.;
        }
        assert_eq!(scene::svg(&planar), before);
        assert!(planar.bonds.iter().all(|b| face(&planar, b).is_none()));
        for a in &mut planar.atoms {
            a.depth = 0.;
        }
    }
    let mut edge_on = ring(0.);
    for a in &mut edge_on.atoms {
        a.depth = a.position.y;
        a.position.y = 0.;
    }
    for b in edge_on.bonds.iter().filter(|b| b.order == 2) {
        assert!(
            secondary(&edge_on, b).is_none(),
            "Edge-on face restored a screen gap"
        );
        assert!(matches!(
            scene::effective_double_position(&edge_on, b),
            DoublePosition::Left | DoublePosition::Right
        ));
    }
}

#[test]
fn acute_projected_face_clipping_can_only_shorten_and_can_omit_a_rail() {
    let triangle = [Point::new(0., 0.), Point::new(40., 0.), Point::new(2., 10.)];
    let first = Point::new(0., 3.);
    let last = Point::new(40., 3.);
    let (a, b) = clip(first, last, &triangle, 0.5).unwrap();
    assert!(a.x > first.x && b.x < last.x);
    assert_eq!(a.y, 3.);
    assert_eq!(b.y, 3.);
    assert!(clip(Point::new(0., 11.), Point::new(40., 11.), &triangle, 0.5).is_none());
    let crossed = [
        Point::new(0., 0.),
        Point::new(30., 20.),
        Point::new(0., 20.),
        Point::new(40., 0.),
    ];
    assert!(!simple(&crossed));
    assert!(clip(first, last, &crossed, 0.5).is_none());
}

#[test]
fn dense_graphs_and_long_cycles_use_the_bounded_legacy_fallback() {
    let mut dense = Document::default();
    for index in 0..100 {
        let id = dense.add_atom("C", Point::new(index as f32, (index % 3) as f32));
        dense.atom_mut(id).unwrap().depth = (index % 7) as f32;
    }
    let ids = dense.all_ids();
    for (index, a) in ids.iter().enumerate() {
        for b in ids.iter().skip(index + 1) {
            dense.add_bond(*a, *b, 1, "plain");
        }
    }
    assert!(
        paths(&dense, &dense.bonds[0]).is_empty(),
        "Dense graph exhausted the bounded search"
    );
    let mut long = Document::default();
    let ids: Vec<_> = (0..30)
        .map(|index| {
            let angle = index as f32 * std::f32::consts::TAU / 30.;
            long.add_atom("C", Point::new(200. * angle.cos(), 200. * angle.sin()))
        })
        .collect();
    for (a, b) in ids.iter().zip(ids.iter().cycle().skip(1)) {
        long.add_bond(*a, *b, 1, "plain");
    }
    projection::tilt(&mut long, &ids, 70., true);
    assert!(
        face(&long, &long.bonds[0]).is_none(),
        "Unsupported large cycle guessed a local face"
    );
}

#[test]
fn an_unusable_warped_shortest_face_keeps_the_established_fallback() {
    let mut doc = Document::default();
    for (position, depth) in [
        (Point::new(0., 0.), 0.),
        (Point::new(42., 0.), 0.),
        (Point::new(42., 42.), -100.),
        (Point::new(0., 42.), 100.),
    ] {
        let id = doc.add_atom("C", position);
        doc.atom_mut(id).unwrap().depth = depth;
    }
    for (a, b, order) in [(1, 2, 2), (2, 3, 1), (3, 4, 1), (4, 1, 1)] {
        doc.add_bond(a, b, order, "plain");
    }
    let bond = &doc.bonds[0];
    assert!(face(&doc, bond).is_none());
    assert_eq!(
        scene::effective_double_position(&doc, bond),
        DoublePosition::Right
    );
    let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
    let expected = (
        Point::new(spacing * 0.75, spacing),
        Point::new(42. - spacing * 0.75, spacing),
    );
    assert!(scene::primitives(&doc).iter().any(|primitive| matches!(primitive,
        scene::Primitive::Line(a,b,_) if a.distance(expected.0) <0.001 && b.distance(expected.1)<0.001)));
}
