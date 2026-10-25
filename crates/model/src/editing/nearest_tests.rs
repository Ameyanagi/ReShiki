use super::*;

// Preserve the original exhaustive queries as independent behavioral oracles.
fn previous_group(doc: &Document, point: Point, radius: f32) -> Option<u64> {
    doc.abbreviations.iter().rev().find_map(|group| {
        if crate::models3d::hides(doc, group.anchor) {
            return None;
        }
        let atom = doc.atom(group.anchor)?;
        let (lo, hi) = crate::scene::atom_label_bounds(atom, doc)?;
        (point.x >= lo.x - radius
            && point.x <= hi.x + radius
            && point.y >= lo.y - radius
            && point.y <= hi.y + radius)
            .then_some(group.anchor)
    })
}

fn previous_atom(doc: &Document, point: Point, radius: f32) -> Option<u64> {
    doc.atoms
        .iter()
        .filter(|a| doc.atom_drawn(a.id) && a.position.distance(point) < radius)
        .min_by(|a, b| {
            a.position
                .distance(point)
                .total_cmp(&b.position.distance(point))
        })
        .map(|a| a.id)
        .or_else(|| previous_group(doc, point, radius))
}

fn previous_bond(doc: &Document, point: Point, radius: f32) -> Option<usize> {
    doc.bonds
        .iter()
        .enumerate()
        .filter(|(_, bond)| doc.bond_drawn(bond.a, bond.b))
        .filter_map(|(index, bond)| {
            let a = doc.atom(bond.a)?.position;
            let b = doc.atom(bond.b)?.position;
            let distance = crate::graphics::segment_distance(point, a, b);
            (distance < radius).then_some((index, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|hit| hit.0)
}

fn check(doc: &Document, points: &[Point], radii: &[f32]) {
    for &point in points {
        for &radius in radii {
            assert_eq!(
                crate::abbreviations::label_hit(doc, point, radius),
                previous_group(doc, point, radius)
            );
            assert_eq!(
                doc.nearest(point, radius),
                previous_atom(doc, point, radius)
            );
            assert_eq!(
                nearest_bond(doc, point, radius),
                previous_bond(doc, point, radius)
            );
        }
    }
}

#[test]
fn indexed_nearest_queries_match_exhaustive_gallery_hits_and_group_labels() {
    let doc = Document::from_json(include_bytes!(
        "../../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let mut points: Vec<_> = doc
        .atoms
        .iter()
        .step_by(64)
        .flat_map(|atom| [atom.position, atom.position.offset(8., -6.)])
        .collect();
    points.extend(doc.bonds.iter().step_by(64).map(|bond| {
        let a = doc.atom(bond.a).unwrap().position;
        let b = doc.atom(bond.b).unwrap().position;
        Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.)
    }));
    for group in &doc.abbreviations {
        let atom = doc.atom(group.anchor).unwrap();
        if let Some((lo, hi)) = crate::scene::atom_label_bounds(atom, &doc) {
            points.extend([lo, hi, Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.)]);
        }
    }
    points.push(doc.bounds().1.offset(100., 100.));
    check(&doc, &points, &[0., 7., 10., 250.]);
}

#[test]
fn nearest_queries_keep_ties_strict_radius_missing_endpoints_and_first_duplicate_id() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(-20., -10.));
    let b = doc.add_atom("C", Point::new(20., -10.));
    let c = doc.add_atom("C", Point::new(-20., 10.));
    let d = doc.add_atom("C", Point::new(20., 10.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(c, d, 1, "plain");
    assert_eq!(doc.nearest(Point::new(-20., 0.), 11.), Some(a));
    assert_eq!(nearest_bond(&doc, Point::default(), 11.), Some(0));
    assert_eq!(nearest_bond(&doc, Point::default(), 10.), None);
    let mut duplicate = doc.atoms[0].clone();
    duplicate.position = Point::new(1000., 1000.);
    doc.atoms.push(duplicate);
    doc.bonds
        .push(crate::document::Bond::plain(a, 999, 1, "plain"));
    check(
        &doc,
        &[
            Point::default(),
            Point::new(-20., 0.),
            Point::new(999., 999.),
        ],
        &[f32::NAN, -1., 0., 10., 11., f32::INFINITY],
    );
}

#[test]
fn nearest_queries_skip_hidden_2d_atoms_and_bonds_but_keep_drawn_neighbors() {
    use crate::models3d::{Display, ForceField, Model3d, Placed};
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(-20., 0.));
    let b = doc.add_atom("O", Point::new(20., 0.));
    doc.add_bond(a, b, 1, "plain");
    let c = doc.add_atom("C", Point::new(-20., 20.));
    let d = doc.add_atom("O", Point::new(20., 20.));
    doc.add_bond(c, d, 1, "plain");
    let mut model = Model3d::new(
        &doc,
        vec![
            Placed {
                id: a,
                xyz: [-1., 0., 0.],
            },
            Placed {
                id: b,
                xyz: [1., 0., 0.],
            },
        ],
        vec![],
        ForceField::Mmff94s,
    );
    model.display = Display::ThreeD;
    doc.models.push(model);
    assert_eq!(nearest_bond(&doc, Point::default(), 10.), None);
    assert_eq!(nearest_bond(&doc, Point::default(), 30.), Some(1));
    check(
        &doc,
        &[Point::default(), Point::new(-20., 0.), Point::new(20., 20.)],
        &[7., 10., 30.],
    );
}
