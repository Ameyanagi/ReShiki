// Frozen pre-refactor snap behavior for encounter-order and floating-point parity.
use super::*;

fn reference_snap_ring(
    doc: &mut Document,
    ids: &[u64],
    delta: Point,
    radius: f32,
) -> Option<Vec<u64>> {
    if doc
        .groups
        .iter()
        .any(|g| g.members.iter().any(|id| ids.contains(id)))
    {
        return None;
    }
    struct Candidate {
        score: f32,
        source: [u64; 2],
        target: [u64; 2],
        points: Vec<Point>,
    }
    let ring = isolated_ring(doc, ids)?;
    let mut best: Option<Candidate> = None;
    for (a, b) in ring
        .iter()
        .zip(ring.iter().cycle().skip(1))
        .take(ring.len())
    {
        let source = [*a, *b];
        let a = doc.atom(source[0])?.position;
        let b = doc.atom(source[1])?.position;
        let sx = b.x - a.x;
        let sy = b.y - a.y;
        let source_length_sq = sx * sx + sy * sy;
        if source_length_sq < 0.001 {
            continue;
        }
        let midpoint = Point::new((a.x + b.x) / 2.0 + delta.x, (a.y + b.y) / 2.0 + delta.y);
        for bond in &doc.bonds {
            if ids.contains(&bond.a)
                || ids.contains(&bond.b)
                || bond.order != 1
                || bond.display != "plain"
            {
                continue;
            }
            let Some((ta, tb)) = doc.atom(bond.a).zip(doc.atom(bond.b)) else {
                continue;
            };
            if [ta, tb].iter().any(|a| {
                a.element != "C"
                    || a.charge != 0
                    || a.isotope != 0
                    || a.aromatic
                    || doc
                        .bonds
                        .iter()
                        .filter(|b| b.a == a.id || b.b == a.id)
                        .map(|b| b.order as u32)
                        .sum::<u32>()
                        > 3
            }) {
                continue;
            }
            let target_midpoint = Point::new(
                (ta.position.x + tb.position.x) / 2.0,
                (ta.position.y + tb.position.y) / 2.0,
            );
            if midpoint.distance(target_midpoint) > radius {
                continue;
            }
            let length = ta.position.distance(tb.position);
            if length < 0.001 {
                continue;
            }
            for target in [[ta, tb], [tb, ta]] {
                let tx = target[1].position.x - target[0].position.x;
                let ty = target[1].position.y - target[0].position.y;
                let cosine_scale = (tx * sx + ty * sy) / source_length_sq;
                let sine_scale = (ty * sx - tx * sy) / source_length_sq;
                let points: Vec<_> = ring
                    .iter()
                    .map(|id| {
                        let p = doc.atom(*id)?.position;
                        let x = p.x - a.x;
                        let y = p.y - a.y;
                        Some(target[0].position.offset(
                            cosine_scale * x - sine_scale * y,
                            sine_scale * x + cosine_scale * y,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let mut score = 0.0;
                for (id, p) in ring.iter().zip(&points) {
                    let moved = doc.atom(*id)?.position.offset(delta.x, delta.y);
                    score += (p.distance(moved) / length).powi(2) * 0.1;
                    if !source.contains(id) {
                        for other in &doc.atoms {
                            if !ids.contains(&other.id) {
                                score +=
                                    (0.6 - p.distance(other.position) / length).max(0.0).powi(2)
                                        * 1000.0;
                            }
                        }
                    }
                }
                if best.as_ref().is_none_or(|best| score < best.score) {
                    best = Some(Candidate {
                        score,
                        source,
                        target: [target[0].id, target[1].id],
                        points,
                    });
                }
            }
        }
    }
    let Candidate {
        source,
        target,
        points,
        ..
    } = best?;
    let mapped = |id| {
        if id == source[0] {
            target[0]
        } else if id == source[1] {
            target[1]
        } else {
            id
        }
    };
    doc.invalidate_chemistry(&[source[0], source[1], target[0], target[1]]);
    for (id, point) in ring.iter().zip(points) {
        if !source.contains(id) {
            doc.atom_mut(*id)?.position = point;
        }
    }
    doc.atoms.retain(|a| !source.contains(&a.id));
    doc.bonds
        .retain(|b| !(source.contains(&b.a) && source.contains(&b.b)));
    for bond in &mut doc.bonds {
        bond.a = mapped(bond.a);
        bond.b = mapped(bond.b);
    }
    Some(ring.into_iter().map(mapped).collect())
}

#[test]
fn snap_target_cache_preserves_baseline_documents_ids_ties_and_float_bits() {
    for size in 3..=8 {
        for reversed in [false, true] {
            for variant in 0..14 {
                let mut doc = Document::default();
                let ta = doc.add_atom("C", Point::new(-21., 0.));
                let tb = doc.add_atom("C", Point::new(21., 0.));
                doc.add_bond(ta, tb, 1, "plain");
                let other_a = doc.add_atom("C", Point::new(-21., 84.));
                let other_b = doc.add_atom("C", Point::new(21., 84.));
                doc.add_bond(other_a, other_b, 1, "plain");
                let moving: Vec<_> = (0..size)
                    .map(|i| {
                        let angle = i as f32 * std::f32::consts::TAU / size as f32 + 0.37;
                        doc.add_atom(
                            "C",
                            Point::new(250. + 31. * angle.cos(), 200. + 31. * angle.sin()),
                        )
                    })
                    .collect();
                for (&a, &b) in moving.iter().zip(moving.iter().cycle().skip(1)).take(size) {
                    doc.add_bond(a, b, 1, "plain");
                }
                match variant {
                    1 => doc.atom_mut(ta).unwrap().charge = 1,
                    2 => doc.atom_mut(ta).unwrap().isotope = 13,
                    3 => doc.atom_mut(ta).unwrap().aromatic = true,
                    4 => doc.bonds[0].order = 2,
                    5 => doc.bonds[0].b = 999,
                    6 => {
                        for i in 0..3 {
                            let id = doc.add_atom("O", Point::new(-80., i as f32 * 40.));
                            doc.add_bond(ta, id, 1, "plain");
                        }
                    }
                    7 => doc.atom_mut(tb).unwrap().position = doc.atom(ta).unwrap().position,
                    8 => doc.atom_mut(ta).unwrap().position.x = f32::from_bits(0x7fc0_1234),
                    9 => {
                        for &id in &moving {
                            doc.atom_mut(id).unwrap().position = Point::new(250., 200.);
                        }
                    }
                    10 => doc.groups.push(crate::grouping::Group {
                        id: doc.next_id(),
                        members: moving.clone(),
                        integral: false,
                    }),
                    11 => doc
                        .bonds
                        .retain(|b| moving.contains(&b.a) && moving.contains(&b.b)),
                    12 => doc.atom_mut(moving[2]).unwrap().position.y = f32::from_bits(0x7fc0_5678),
                    13 => {
                        doc.atom_mut(other_a).unwrap().position = Point::new(-21., 0.);
                        doc.atom_mut(other_b).unwrap().position = Point::new(21., 0.);
                    }
                    _ => (),
                }
                let mut selected = moving.clone();
                if reversed {
                    selected.rotate_left(1);
                    doc.atoms.reverse();
                    doc.bonds.reverse();
                    for bond in &mut doc.bonds {
                        std::mem::swap(&mut bond.a, &mut bond.b);
                    }
                }
                let a = doc.atom(selected[0]).unwrap().position;
                let b = doc.atom(selected[1]).unwrap().position;
                for (offset, radius) in [(0., 5.), (5., 5.), (5.0001, 5.), (0., 0.), (0., f32::NAN)]
                {
                    let delta = Point::new(-(a.x + b.x) / 2., -(a.y + b.y) / 2. + offset);
                    let mut expected = doc.clone();
                    let expected_ids = reference_snap_ring(&mut expected, &selected, delta, radius);
                    let mut actual = doc.clone();
                    let actual_ids = snap_ring(&mut actual, &selected, delta, radius);
                    assert_eq!(
                        actual_ids, expected_ids,
                        "size {size}, reversed {reversed}, variant {variant}"
                    );
                    assert_eq!(
                        serde_json::to_value(&actual).unwrap(),
                        serde_json::to_value(&expected).unwrap()
                    );
                    for (actual, expected) in actual.atoms.iter().zip(&expected.atoms) {
                        assert_eq!(actual.position.x.to_bits(), expected.position.x.to_bits());
                        assert_eq!(actual.position.y.to_bits(), expected.position.y.to_bits());
                    }
                }
            }
        }
    }
}

#[test]
fn nearest_bond_keeps_strict_radius_degenerate_segments_and_encounter_order() {
    let mut doc = Document::default();
    for point in [
        Point::new(0., 0.),
        Point::new(42., 0.),
        Point::new(0., 10.),
        Point::new(42., 10.),
    ] {
        doc.add_atom("C", point);
    }
    doc.add_bond(1, 2, 1, "plain");
    doc.add_bond(3, 4, 1, "plain");
    assert_eq!(nearest_bond(&doc, Point::new(21., 5.), 6.), Some(0));
    assert_eq!(nearest_bond(&doc, Point::new(21., 5.), 5.), None);
    assert_eq!(nearest_bond(&doc, Point::new(-3., 0.), 4.), Some(0));
    doc.bonds.reverse();
    assert_eq!(nearest_bond(&doc, Point::new(21., 5.), 6.), Some(0));
    for bond in &mut doc.bonds {
        std::mem::swap(&mut bond.a, &mut bond.b);
    }
    assert_eq!(nearest_bond(&doc, Point::new(21., 5.), 6.), Some(0));
    doc.atoms[3].position = doc.atoms[2].position;
    assert_eq!(nearest_bond(&doc, Point::new(0., 10.), 1.), Some(0));
    assert_eq!(nearest_bond(&doc, Point::new(f32::NAN, 0.), 6.), None);
    assert_eq!(nearest_bond(&doc, Point::default(), f32::NAN), None);
    doc.contract(&[1, 2], "Et", "Et").unwrap();
    assert_eq!(nearest_bond(&doc, Point::new(21., 0.), 1.), None);
}

#[test]
fn selection_membership_keeps_collapsed_groups_boundary_stereo_and_object_order() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    let c = doc.add_atom("C", Point::new(63., 36.373));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    doc.contract(&[b, c], "OMe", "MeO").unwrap();
    let caption = doc.next_id();
    doc.annotations.push(crate::document::Annotation {
        id: caption,
        position: Point::new(20., 80.),
        text: "caption".into(),
        format: Default::default(),
    });
    doc.group_selection(&[b, c, caption]).unwrap();
    doc.bonds[0].stereo = Some("E".into());
    doc.bonds[0].stereo_atoms = vec![a, c];
    doc.bonds[0].indicator.offset = Some(Point::new(3., 4.));
    doc.bonds[1].indicator.offset = Some(Point::new(-5., 6.));
    let selected = [b, caption];
    let duplicated = [caption, 999, b, c, b, caption];
    let expected = selection(&doc, &selected);
    let actual = selection(&doc, &duplicated);
    assert_eq!(actual, expected);
    assert_eq!(actual.all_ids(), [b, c, caption]);
    let mut expected = doc.clone();
    let mut actual = doc.clone();
    let convert = |p: Point| Point::new(-p.y + 0.125, p.x - 0.75);
    let vector = |p: Point| Point::new(-p.y, p.x);
    map_positions(&mut expected, &selected, convert, vector);
    map_positions(&mut actual, &duplicated, convert, vector);
    assert_eq!(actual, expected);
    assert_eq!(actual.atom(a), doc.atom(a));
    assert_eq!(actual.bonds[0].stereo, None);
    assert_eq!(
        actual.bonds[0].indicator.offset,
        doc.bonds[0].indicator.offset
    );
    assert_eq!(
        actual.bonds[1].indicator.offset,
        Some(vector(Point::new(-5., 6.)))
    );
    for (actual, expected) in actual.atoms.iter().zip(&expected.atoms) {
        assert_eq!(actual.position.x.to_bits(), expected.position.x.to_bits());
        assert_eq!(actual.position.y.to_bits(), expected.position.y.to_bits());
    }
}
