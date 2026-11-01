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
        scene::bond_rails(&FaceIndex::new(doc), bond)[1].trim,
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

fn narrow_projected_ring() -> Document {
    let mut doc = Document::default();
    let ids: Vec<_> = [
        Point::new(0., 0.),
        Point::new(16., 0.),
        Point::new(9., 40.),
        Point::new(7., 40.),
    ]
    .into_iter()
    .map(|position| {
        let id = doc.add_atom("C", position);
        doc.atom_mut(id).unwrap().depth = position.y;
        id
    })
    .collect();
    for (index, (a, b)) in closed_edges(&ids).enumerate() {
        doc.add_bond(a, b, if index == 0 { 2 } else { 1 }, "plain");
    }
    doc
}

#[test]
fn overlapping_projected_rail_setbacks_omit_reversed_and_tiny_forward_spans() {
    let doc = narrow_projected_ring();
    let bond = &doc.bonds[0];
    let index = FaceIndex::new(&doc);
    let face = index.face(bond).unwrap();
    let inner = scene::bond_rails(&index, bond)[1];
    let start = doc.atom(bond.a).unwrap().position;
    let end = doc.atom(bond.b).unwrap().position;
    let half_width = doc.drawing_style.line_width() / 2.;
    assert!(inner.trim.0 + inner.trim.1 > start.distance(end));
    let tiny_trim = (start.distance(end) - 0.05) / 2.;
    for trim in [inner.trim, (tiny_trim, tiny_trim)] {
        let d = inner.offset.0 * face.side;
        let first = start.offset(face.inward.x * d + trim.0, face.inward.y * d);
        let last = end.offset(face.inward.x * d - trim.1, face.inward.y * d);
        // Both endpoints survive face clipping even when the setbacks cross.
        assert_inside(&face, first, half_width);
        assert_inside(&face, last, half_width);
        assert!(
            matches!(
                index.rail_depth(bond, start, end, inner.offset.0, trim, half_width),
                Some(None)
            ),
            "A projected rail with no drawable forward span must be omitted"
        );
    }
    let visible_trim = (start.distance(end) - 0.2) / 2.;
    assert!(matches!(
        index.rail_depth(
            bond,
            start,
            end,
            inner.offset.0,
            (visible_trim, visible_trim),
            half_width,
        ),
        Some(Some(_))
    ));
}

#[test]
fn omitted_projected_inner_rail_does_not_cut_a_phantom_crossing_gap() {
    let mut doc = narrow_projected_ring();
    let a = doc.add_atom("C", Point::new(8., 7.));
    let b = doc.add_atom("C", Point::new(8., 11.));
    for id in [a, b] {
        doc.atom_mut(id).unwrap().depth = -20.;
    }
    doc.add_bond(a, b, 1, "plain");
    let rear = doc.bonds.len() - 1;
    assert!(
        crate::crossings::gaps(&doc)[rear].is_empty(),
        "The omitted inner rail cut a rear bond beyond the backbone's ink"
    );
    doc.bonds[0].order = 1;
    assert!(crate::crossings::gaps(&doc)[rear].is_empty());
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
        Point::new(spacing, spacing),
        Point::new(42. - spacing, spacing),
    );
    assert!(scene::primitives(&doc).iter().any(|primitive| matches!(primitive,
        scene::Primitive::Line(a,b,_) if a.distance(expected.0) <0.001 && b.distance(expected.1)<0.001)));
}

#[test]
fn rear_opacity_preserves_cage_xyz_and_exact_projected_rail_at_100_percent() {
    for source in [
        include_str!("../../../../../tests/fixtures/projected-double-bonds/c60.rsk"),
        include_str!("../../../../../tests/fixtures/projected-double-bonds/c70.rsk"),
    ] {
        let mut doc = Document::from_json(source.as_bytes()).unwrap();
        let ids = doc.all_ids();
        let before = doc.clone();
        let rails: Vec<_> = doc
            .bonds
            .iter()
            .filter(|b| b.order == 2)
            .map(|b| secondary(&doc, b))
            .collect();
        let svg = scene::svg(&doc);
        for opacity in [0., 0.5, 1.] {
            crate::depth_appearance::set_rear_opacity(&mut doc, &ids, opacity).unwrap();
            assert_eq!(doc.atoms, before.atoms);
            assert_eq!(doc.bonds, before.bonds);
            assert_eq!(
                doc.bonds
                    .iter()
                    .filter(|b| b.order == 2)
                    .map(|b| secondary(&doc, b))
                    .collect::<Vec<_>>(),
                rails
            );
            assert!(!crate::transaction::chemistry_changed(&before, &doc));
            assert!(!scene::primitives(&doc).is_empty());
        }
        assert_eq!(scene::svg(&doc), svg);
    }
}

fn assert_same_face(actual: Option<&Face>, expected: Option<&Face>) {
    match (actual, expected) {
        (Some(a), Some(b)) => {
            assert_eq!(a.atoms, b.atoms);
            assert_eq!(a.points, b.points);
            assert_eq!(a.side, b.side);
            assert_eq!(a.tangent, b.tangent);
            assert_eq!(a.inward, b.inward);
            assert_eq!(a.origin, b.origin);
            assert_eq!(a.end, b.end);
            assert_eq!(a.tangent_xyz, b.tangent_xyz);
            assert_eq!(a.inward_xyz, b.inward_xyz);
        }
        (None, None) => {}
        _ => panic!("Memoization changed face presence"),
    }
}

fn assert_same_rail(actual: Option<Option<RailDepth>>, expected: Option<Option<RailDepth>>) {
    match (actual, expected) {
        (Some(Some(a)), Some(Some(b))) => {
            assert_eq!(a.points, b.points);
            assert_eq!(a.support, b.support);
            match (a.field, b.field) {
                (
                    DepthField::Bond { a, b, wa, wb },
                    DepthField::Bond {
                        a: c,
                        b: d,
                        wa: wc,
                        wb: wd,
                    },
                ) => assert_eq!((a, b, wa, wb), (c, d, wc, wd)),
                _ => panic!("Projected rail lost its bond depth field"),
            }
        }
        (Some(None), Some(None)) | (None, None) => {}
        _ => panic!("Memoization changed rail presence or collapse"),
    }
}

fn indexed_secondary(index: &FaceIndex<'_>, bond: &Bond) -> Option<Option<RailDepth>> {
    let doc = index.doc();
    let side = match scene::effective_double_position(doc, bond) {
        DoublePosition::Left => -1.,
        DoublePosition::Right => 1.,
        _ => panic!("Projected ring lost its backbone"),
    };
    let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
    index.rail_depth(
        bond,
        doc.atom(bond.a).unwrap().position,
        doc.atom(bond.b).unwrap().position,
        spacing * side,
        scene::bond_rails(index, bond)[1].trim,
        doc.drawing_style.line_width() / 2.,
    )
}

#[test]
fn memoized_faces_and_rails_agree_with_single_queries() {
    let mut edge_on = ring(0.);
    for atom in &mut edge_on.atoms {
        atom.depth = atom.position.y;
        atom.position.y = 0.;
    }
    let documents = [
        Document::from_json(
            include_str!("../../../../../tests/fixtures/projected-double-bonds/c60.rsk").as_bytes(),
        )
        .unwrap(),
        Document::from_json(
            include_str!("../../../../../tests/fixtures/projected-double-bonds/c70.rsk").as_bytes(),
        )
        .unwrap(),
        ring(70.),
        edge_on,
    ];
    let mut collapsed = false;
    for doc in &documents {
        let index = FaceIndex::new(doc);
        for bond in &doc.bonds {
            let mut queries = vec![bond.clone()];
            if bond.order == 2 {
                let mut reversed = bond.clone();
                reversed.reverse();
                queries.push(reversed);
            }
            for query in &queries {
                for _ in 0..2 {
                    let cached = index.face(query);
                    assert_same_face(cached.as_deref(), face(doc, query).as_ref());
                    if let Some(cached) = cached {
                        assert!(Rc::ptr_eq(&cached, &index.face(query).unwrap()));
                    }
                    assert_eq!(index.automatic_side(query), automatic_side(doc, query));
                    assert_eq!(index.face_atoms(query), face_atoms(doc, query));
                    if query.order == 2 {
                        let actual = indexed_secondary(&index, query);
                        collapsed |= matches!(actual, Some(None));
                        let side = index.automatic_side(query).unwrap();
                        let spacing = doc.drawing_style.bond_length_world
                            * doc.drawing_style.bond_spacing_ratio;
                        let expected = rail_depth(
                            doc,
                            query,
                            doc.atom(query.a).unwrap().position,
                            doc.atom(query.b).unwrap().position,
                            spacing * side,
                            scene::bond_rails(&index, query)[1].trim,
                            doc.drawing_style.line_width() / 2.,
                        );
                        assert_same_rail(actual, expected);
                    }
                }
            }
        }
    }
    assert!(collapsed, "Agreement must cover Some(None) collapsed rails");
}

#[test]
fn indexed_face_choice_is_stable_under_reordering_and_bond_reversal() {
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
    let index = FaceIndex::new(&doc);
    let reordered_index = FaceIndex::new(&reordered);
    for bond in doc.bonds.iter().filter(|b| b.order == 2) {
        let reversed = reordered
            .bonds
            .iter()
            .find(|b| b.a == bond.b && b.b == bond.a)
            .unwrap();
        let original_face = index.face(bond).unwrap();
        let reversed_face = reordered_index.face(reversed).unwrap();
        assert_eq!(original_face.atoms, reversed_face.atoms);
        assert_eq!(original_face.points, reversed_face.points);
        assert_eq!(original_face.side, -reversed_face.side);
        match (
            indexed_secondary(&index, bond).unwrap(),
            indexed_secondary(&reordered_index, reversed).unwrap(),
        ) {
            (Some(a), Some(b)) => {
                assert!(a.points.0.distance(b.points.1) < 0.002);
                assert!(a.points.1.distance(b.points.0) < 0.002);
            }
            (None, None) => {}
            _ => panic!("Reversal changed indexed secondary visibility"),
        }
    }
}

#[test]
fn indexed_search_budget_is_per_query() {
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
    let ring_start = dense.bonds.len();
    let ring_ids = editing::ring(&mut dense, Point::new(1000., 1000.), 6, false, 0.);
    projection::tilt(&mut dense, &ring_ids, 70., true);
    let index = FaceIndex::new(&dense);
    for bond in dense.bonds.iter().take(5) {
        assert_eq!(index.paths(bond), paths(&dense, bond));
        assert!(index.paths(bond).is_empty());
        assert_same_face(index.face(bond).as_deref(), face(&dense, bond).as_ref());
        assert!(index.face(bond).is_none());
        assert!(index.memo.borrow().contains_key(&(bond.a, bond.b)));
    }
    let ring_bond = &dense.bonds[ring_start];
    assert_same_face(
        index.face(ring_bond).as_deref(),
        face(&dense, ring_bond).as_ref(),
    );
    assert!(
        index.face(ring_bond).is_some(),
        "An exhausted query spent the next query's budget"
    );
}
