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
                    assert!(old.offset(d.x, d.y).distance(drawing.endpoint(anchor, end)) < 0.001);
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
fn axis_lock_keeps_bond_constraints_and_never_leaves_the_axis() -> Result<(), String> {
    let mut doc = Document::default();
    let fixed = doc.add_atom("C", Point::default());
    let moving = doc.add_atom("C", Point::new(21., 42. * 3_f32.sqrt() / 2.));
    doc.add_bond(fixed, moving, 1, "plain");
    let drawing = BondDrawing::default();
    // Horizontally, the 120° position keeps the bond length and angle.
    let d = axis_delta(&doc, &[moving], Point::new(-45., 3.), drawing);
    assert!((d.x + 42.).abs() < 0.001 && d.y.abs() < 0.001, "{d:?}");
    let moved = doc.atom(moving).ok_or("moving")?.position.offset(d.x, d.y);
    assert!((moved.distance(Point::default()) - 42.).abs() < 0.001);
    // Vertically, no constrained position stays on the axis.
    assert_eq!(
        axis_delta(&doc, &[moving], Point::new(5., 40.), drawing),
        Point::default()
    );
    // Option/Alt frees the constraints; whole molecules are never constrained.
    assert_eq!(
        axis_delta(
            &doc,
            &[moving],
            Point::new(5., 40.),
            drawing.unconstrained(true)
        ),
        Point::new(0., 40.)
    );
    assert_eq!(
        axis_delta(&doc, &[fixed, moving], Point::new(5., 40.), drawing),
        Point::new(0., 40.)
    );
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
            assert!((moved.distance(doc.atom(id).ok_or("fixed")?.position) - 42.).abs() < 0.001);
        }
    }
    doc.atom_mut(b).ok_or("fixed")?.position.x = 200.;
    assert_eq!(
        delta(&doc, &[c], Point::new(55., -90.), BondDrawing::default()),
        Point::default()
    );
    Ok(())
}
