//! A shared constraint for the drag preview and the committed movement.
use reshiki::{
    chains::BondDrawing,
    document::{Document, Point},
};
use std::collections::{HashMap, HashSet};

pub(super) fn delta(doc: &Document, ids: &[u64], requested: Point, drawing: BondDrawing) -> Point {
    if !requested.x.is_finite() || !requested.y.is_finite() {
        return Point::default();
    }
    if !drawing.fixed_length && !drawing.fixed_angles {
        return requested;
    }
    if !drawing.length.is_finite() || drawing.length <= 0. {
        return Point::default();
    }
    let selected: HashSet<_> = doc.expand_abbreviation_selection(ids).into_iter().collect();
    let positions: HashMap<_, _> = doc.atoms.iter().map(|a| (a.id, a.position)).collect();
    // Internal bonds move rigidly. Only bonds to fixed atoms constrain translation.
    let mut centers = Vec::new();
    for bond in &doc.bonds {
        let (moving, fixed) = match (selected.contains(&bond.a), selected.contains(&bond.b)) {
            (true, false) => (bond.a, bond.b),
            (false, true) => (bond.b, bond.a),
            _ => continue,
        };
        let Some((moving, fixed)) = positions.get(&moving).zip(positions.get(&fixed)) else {
            return Point::default();
        };
        centers.push(Point::new(fixed.x - moving.x, fixed.y - moving.y));
    }
    let Some(&center) = centers.first() else {
        return requested;
    };
    let tolerance = drawing.length * 0.0001;
    let valid = |d: Point| {
        centers.iter().all(|c| {
            let angle = (d.y - c.y).atan2(d.x - c.x);
            (!drawing.fixed_length || (c.distance(d) - drawing.length).abs() <= tolerance)
                && (!drawing.fixed_angles || (angle - drawing.angle(angle)).abs() < 0.0001)
        })
    };
    let candidate = drawing.endpoint(center, requested);
    if valid(candidate) {
        return candidate;
    }
    let mut candidates = Vec::new();
    if drawing.fixed_angles {
        for step in 0..24 {
            let angle = step as f32 * std::f32::consts::PI / 12.;
            let length = if drawing.fixed_length {
                drawing.length
            } else {
                ((requested.x - center.x) * angle.cos() + (requested.y - center.y) * angle.sin())
                    .max(0.)
            };
            candidates.push(center.offset(length * angle.cos(), length * angle.sin()));
        }
    } else if let Some(other) = centers.iter().find(|c| c.distance(center) > tolerance) {
        // Equal-radius circle intersections preserve both bonds at a ring vertex.
        let distance = center.distance(*other);
        if distance <= 2. * drawing.length {
            let middle = Point::new((center.x + other.x) / 2., (center.y + other.y) / 2.);
            let height = (drawing.length.powi(2) - (distance / 2.).powi(2))
                .max(0.)
                .sqrt();
            let x = -(other.y - center.y) / distance * height;
            let y = (other.x - center.x) / distance * height;
            candidates.extend([middle.offset(x, y), middle.offset(-x, -y)]);
        }
    }
    // Conflicting constraints keep geometry intact; Option/Alt explicitly frees it.
    candidates
        .into_iter()
        .filter(|d| valid(*d))
        .min_by(|a, b| a.distance(requested).total_cmp(&b.distance(requested)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_constraints_and_free_modes_follow_bond_settings() -> Result<(), String> {
        for length in [28., 42., 70.] {
            let mut doc = Document::default();
            let anchor = Point::new(10., -20.);
            let old = anchor.offset(length, 0.);
            let fixed = doc.add_atom("C", anchor);
            let moving = doc.add_atom("N", old);
            doc.add_bond(fixed, moving, 1, "plain");
            for degrees in [-173_f32, -82., -17., 37., 103., 178.] {
                let end = anchor.offset(
                    2.3 * length * degrees.to_radians().cos(),
                    2.3 * length * degrees.to_radians().sin(),
                );
                let requested = Point::new(end.x - old.x, end.y - old.y);
                for fixed_length in [false, true] {
                    for fixed_angles in [false, true] {
                        let drawing = BondDrawing {
                            length,
                            fixed_length,
                            fixed_angles,
                        };
                        let d = delta(&doc, &[moving], requested, drawing);
                        assert!(
                            old.offset(d.x, d.y).distance(drawing.endpoint(anchor, end)) < 0.001
                        );
                        assert_eq!(
                            delta(&doc, &[moving], requested, drawing.unconstrained(true)),
                            requested
                        );
                    }
                }
            }
        }
        Ok(())
    }
    #[test]
    fn whole_molecules_and_unbonded_objects_translate_freely() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("O", Point::new(42., 0.));
        doc.add_bond(a, b, 1, "plain");
        let requested = Point::new(13., 27.);
        assert_eq!(
            delta(&doc, &[a, b], requested, BondDrawing::default()),
            requested
        );
        assert_eq!(
            delta(&doc, &[], requested, BondDrawing::default()),
            requested
        );
    }
    #[test]
    fn collapsed_ligand_moves_as_one_group_without_losing_targets() -> Result<(), String> {
        let mut doc = Document::default();
        let fe = doc.add_atom("Fe", Point::default());
        let cp = doc.add_atom("C", Point::new(84., 0.));
        doc.add_bond(fe, cp, 1, "plain");
        doc = reshiki::ligands::replace(&doc, cp, "Cp*")?;
        let before = doc.clone();
        let d = delta(&doc, &[cp], Point::new(-30., 50.), BondDrawing::default());
        doc.translate(&[cp], d.x, d.y);
        assert!(
            (doc.atom(cp)
                .ok_or("Cp*")?
                .position
                .distance(Point::default())
                - 42.)
                .abs()
                < 0.001
        );
        assert_eq!(doc.atom(fe), before.atom(fe));
        for atom in before.atoms.iter().filter(|a| a.id != fe) {
            assert_eq!(
                doc.atom(atom.id).ok_or("member")?.position,
                atom.position.offset(d.x, d.y)
            );
        }
        assert_eq!(doc.abbreviations, before.abbreviations);
        doc.validate()
    }
    #[test]
    fn multiple_neighbors_never_stretch_to_satisfy_just_one_bond() -> Result<(), String> {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::new(-21., 0.));
        let b = doc.add_atom("C", Point::new(21., 0.));
        let c = doc.add_atom("N", Point::new(0., 42. * 3_f32.sqrt() / 2.));
        doc.add_bond(a, c, 1, "plain");
        doc.add_bond(c, b, 1, "plain");
        for fixed_angles in [false, true] {
            let d = delta(
                &doc,
                &[c],
                Point::new(55., -90.),
                BondDrawing {
                    fixed_angles,
                    ..Default::default()
                },
            );
            let moved = doc.atom(c).ok_or("moving")?.position.offset(d.x, d.y);
            for id in [a, b] {
                assert!(
                    (moved.distance(doc.atom(id).ok_or("fixed")?.position) - 42.).abs() < 0.001
                );
            }
        }
        doc.atom_mut(b).ok_or("fixed")?.position.x = 200.;
        assert_eq!(
            delta(&doc, &[c], Point::new(55., -90.), BondDrawing::default()),
            Point::default()
        );
        Ok(())
    }
}
