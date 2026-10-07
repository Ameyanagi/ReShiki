use super::*;
use cosmolkit_core::{AtomSpec, BondOrder, BondSpec, Element, MoleculeBuilder};

// Analytic dodecahedral C20 topology exercises the policy independently of
// the C60 reference fixture and demonstrates that the gate is not a C60
// atom-count special case. Atom typing is explicit for this geometry test.
fn dodecahedron() -> (Molecule, Vec<[f64; 3]>) {
    let phi = (1. + 5_f64.sqrt()) / 2.;
    let mut coordinates = Vec::new();
    for x in [-1., 1.] {
        for y in [-1., 1.] {
            for z in [-1., 1.] {
                coordinates.push([x, y, z]);
            }
        }
    }
    for a in [-1., 1.] {
        for b in [-1., 1.] {
            coordinates.extend([
                [0., a / phi, b * phi],
                [a / phi, b * phi, 0.],
                [a * phi, 0., b / phi],
            ]);
        }
    }
    for point in &mut coordinates {
        *point = point.map(|value| value * 1.2);
    }
    let mut builder = MoleculeBuilder::new();
    let atoms: Vec<_> = coordinates
        .iter()
        .map(|_| builder.add_atom(AtomSpec::new(Element::C).with_hybridization(Hybridization::Sp2)))
        .collect();
    for (i, &point) in coordinates.iter().enumerate() {
        for (j, &other) in coordinates.iter().enumerate().skip(i + 1) {
            if (norm(sub(point, other)) - 2.4 / phi).abs() < 1e-8 {
                builder
                    .add_bond(BondSpec::new(atoms[i], atoms[j], BondOrder::Single))
                    .unwrap();
            }
        }
    }
    (builder.build().unwrap(), coordinates)
}

#[test]
fn ordinary_aromatic_rings_are_not_fullerene_cages() {
    let benzene = Molecule::from_smiles("c1ccccc1").unwrap();
    let fused = Molecule::from_smiles("c1ccc2cc3ccccc3cc2c1").unwrap();
    assert!(Cage::from_molecule(&benzene).is_none());
    assert!(Cage::from_molecule(&fused).is_none());
    assert!(Cage::from_molecule(&fused.with_hydrogens().unwrap()).is_none());
}

#[test]
fn supported_cage_envelope_accepts_rotations_and_modest_strain_but_rejects_folds() {
    let (mol, coordinates) = dodecahedron();
    let cage = Cage::from_molecule(&mol).expect("closed C20 pentagonal cage");
    assert_eq!(cage.faces.len(), 12);
    cage.validate(&mol, &coordinates).unwrap();
    let transformed: Vec<_> = coordinates
        .iter()
        .map(|&[x, y, z]| [z + 17., x - 11., y + 3.])
        .collect();
    cage.validate(&mol, &transformed).unwrap();
    let mut strained = coordinates.clone();
    strained[0][0] += 0.05;
    cage.validate(&mol, &strained).unwrap();
    let mut folded = coordinates.clone();
    folded[0] = folded[0].map(|value| value * 0.1);
    let error = cage.validate(&mol, &folded).unwrap_err();
    assert!(error.starts_with("cage geometry is outside supported near-convex envelope:"));
    let flattened: Vec<_> = coordinates.iter().map(|&[x, y, _]| [x, y, 0.]).collect();
    assert!(cage.validate(&mol, &flattened).is_err());
}

#[test]
fn projected_face_policy_rejects_a_warped_star_with_positive_consecutive_turns() {
    let pentagon: Vec<_> = (0..5)
        .map(|i| {
            let angle = i as f64 * std::f64::consts::TAU / 5.;
            [1.5 * angle.cos(), 1.5 * angle.sin(), 0.]
        })
        .collect();
    let face = [0, 1, 2, 3, 4];
    assert!(projected_face_is_convex(
        &face,
        &pentagon,
        [0., 0., 1.],
        1.5
    ));
    let star: Vec<_> = [0, 2, 4, 1, 3]
        .into_iter()
        .enumerate()
        .map(|(i, index)| {
            let mut point = pentagon[index];
            point[2] = if i % 2 == 0 { 0.15 } else { -0.15 };
            point
        })
        .collect();
    let mut newell = [0.; 3];
    for i in 0..star.len() {
        let contribution = cross(star[i], star[(i + 1) % star.len()]);
        for axis in 0..3 {
            newell[axis] += contribution[axis];
        }
    }
    let normal = newell.map(|value| value / norm(newell));
    for i in 0..star.len() {
        let previous = star[(i + star.len() - 1) % star.len()];
        let current = star[i];
        let next = star[(i + 1) % star.len()];
        assert!(dot(cross(sub(current, previous), sub(next, current)), normal) > 1e-6);
    }
    assert!(!projected_face_is_convex(&face, &star, normal, 1.5));
}

#[test]
fn segment_clearance_checks_interiors_endpoints_and_parallel_bonds() {
    for (p, q, r, s, squared) in [
        ([-1., 0., 0.], [1., 0., 0.], [0., -1., 0.], [0., 1., 0.], 0.),
        (
            [-1., 0., 0.],
            [1., 0., 0.],
            [0., -1., 0.6],
            [0., 1., 0.6],
            0.36,
        ),
        ([0., 0., 0.], [1., 0., 0.], [2., 1., 0.], [3., 1., 0.], 2.),
        ([0., 0., 0.], [2., 0., 0.], [1., 1., 0.], [3., 1., 0.], 1.),
    ] {
        assert!((segment_distance_squared(p, q, r, s) - squared).abs() < 1e-10);
        assert!((segment_distance_squared(r, s, p, q) - squared).abs() < 1e-10);
        assert!((segment_distance_squared(q, p, s, r) - squared).abs() < 1e-10);
    }
}
