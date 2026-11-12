use super::*;

#[test]
fn legacy_capacity_and_first_extra_atom_are_pinned() {
    assert_eq!(
        Capacity::default(),
        Capacity {
            atoms: 512,
            bonds: 2048,
            coordinates: 4096,
        }
    );
    assert_eq!(Capacity::for_heap(258 * 1024 * 1024 - 1).atoms, 512);
    assert_eq!(Capacity::for_heap(258 * 1024 * 1024).atoms, 513);
}

#[test]
fn operational_capacity_is_bounded_by_independent_structural_limits() {
    assert!(Capacity::for_heap(0).validate().is_err());
    assert!(Capacity::for_heap(512 * 1024 - 1).validate().is_err());
    assert_eq!(Capacity::for_heap(64 * 1024 * 1024).atoms, 128);
    assert_eq!(Capacity::for_heap(256 * 1024 * 1024).atoms, 512);
    let high = Capacity::for_heap(usize::MAX);
    assert_eq!(high.atoms, MAX_ATOMS);
    assert_eq!(high.bonds, 4 * MAX_ATOMS);
    assert!(high.bonds <= MAX_BONDS);
    assert_eq!(high.coordinates, MAX_COORDINATES);
    assert!(high.validate().is_ok());
    assert!(
        Capacity {
            atoms: MAX_ATOMS + 1,
            ..high
        }
        .validate()
        .is_err()
    );
    assert!(
        Capacity {
            bonds: MAX_BONDS + 1,
            ..high
        }
        .validate()
        .is_err()
    );
    assert!(
        Capacity {
            coordinates: MAX_COORDINATES + 1,
            ..high
        }
        .validate()
        .is_err()
    );
    let mut request = ethanol(ForceField::MMFF94s);
    request.atoms = vec![request.atoms[0].clone(); 513];
    request.bonds.clear();
    assert!(request.validate_capacity(Capacity::default()).is_err());
    assert!(request.validate_capacity(high).is_ok());
    request
        .atoms
        .resize(MAX_ATOMS + 1, request.atoms[0].clone());
    assert!(request.validate().is_err());
}

fn ethanol(field: ForceField) -> Request {
    Request {
        atoms: [6, 6, 8]
            .into_iter()
            .map(|atomic_number| AtomInput {
                atomic_number,
                isotope: 0,
                charge: 0,
                explicit_h: 0,
                no_implicit: false,
                aromatic: false,
                radical: 0,
                chiral_tag: 0,
            })
            .collect(),
        bonds: [(0, 1), (1, 2)]
            .into_iter()
            .map(|(a, b)| BondInput {
                a,
                b,
                order: 1,
                aromatic: false,
                stereo: 0,
                stereo_atoms: None,
            })
            .collect(),
        field,
        relaxed: false,
        operation: Operation::Generate,
        coordinates: Vec::new(),
        fixed_atoms: Vec::new(),
        conformers: 2,
        seed: 61453,
        max_iterations: 500,
    }
}

fn molecule(numbers: &[u32], edges: &[(usize, usize, u32)], field: ForceField) -> Request {
    let mut request = ethanol(field);
    request.atoms = numbers
        .iter()
        .map(|&atomic_number| AtomInput {
            atomic_number,
            isotope: 0,
            charge: 0,
            explicit_h: 0,
            no_implicit: false,
            aromatic: false,
            radical: 0,
            chiral_tag: 0,
        })
        .collect();
    request.bonds = edges
        .iter()
        .map(|&(a, b, order)| BondInput {
            a,
            b,
            order,
            aromatic: order == 12,
            stereo: 0,
            stereo_atoms: None,
        })
        .collect();
    request.max_iterations = 1000;
    request
}

fn evaluate(request: &Request, coordinates: Vec<[f64; 3]>) -> Response {
    let mut evaluation = request.clone();
    evaluation.operation = Operation::Evaluate;
    evaluation.coordinates = coordinates;
    evaluation.fixed_atoms.clear();
    solve(&evaluation).unwrap()
}

fn close(actual: f64, expected: f64, absolute: f64, relative: f64) {
    assert!(
        (actual - expected).abs() <= absolute + relative * expected.abs(),
        "actual={actual}, expected={expected}"
    );
}

#[test]
fn rust_results_preserve_hydrogen_mapping_and_gradient_for_every_field() {
    for field in [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF] {
        let mut request = ethanol(field);
        let generated = solve(&request).unwrap();
        assert_eq!(generated.original_count, 3);
        assert_eq!(generated.coordinates.len(), 9);
        assert_eq!(generated.hydrogen_parents, [0, 0, 0, 1, 1, 2]);
        assert!(generated.energy <= generated.initial_energy + 1e-6);
        request.operation = Operation::Evaluate;
        request.coordinates = generated.coordinates;
        request.coordinates[0][0] += 0.15;
        let evaluated = solve(&request).unwrap();
        let gradient = evaluated.gradient.unwrap();
        assert_eq!(gradient.len(), request.coordinates.len());
        let step = 1e-5;
        for (index, force) in gradient.iter().enumerate() {
            for (axis, &analytic) in force.iter().enumerate() {
                let mut plus = request.coordinates.clone();
                let mut minus = plus.clone();
                plus[index][axis] += step;
                minus[index][axis] -= step;
                let numerical = (evaluate(&request, plus).energy
                    - evaluate(&request, minus).energy)
                    / (2.0 * step);
                close(analytic, numerical, 2e-5, 2e-6);
            }
        }

        let transformed = request
            .coordinates
            .iter()
            .map(|&[x, y, z]| [-y + 6.5, x - 2.0, z + 1.25])
            .collect();
        let rotated = evaluate(&request, transformed);
        close(rotated.energy, evaluated.energy, 1e-7, 1e-9);
        let rotated_gradient = rotated.gradient.unwrap();
        for (original, rotated) in gradient.iter().zip(rotated_gradient) {
            let expected = [-original[1], original[0], original[2]];
            for axis in 0..3 {
                close(rotated[axis], expected[axis], 1e-6, 1e-8);
            }
        }
    }
}

#[test]
fn rust_relax_keeps_original_and_added_hydrogen_fixed() {
    for field in [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF] {
        let mut request = ethanol(field);
        request.coordinates = solve(&request).unwrap().coordinates;
        request.operation = Operation::Relax;
        request.max_iterations = 20;
        request.fixed_atoms = vec![0, 3];
        request.coordinates[0][0] += 0.04;
        let result = solve(&request).unwrap();
        for &i in &request.fixed_atoms {
            assert_eq!(result.coordinates[i], request.coordinates[i]);
        }
        assert!(result.energy <= result.initial_energy + 1e-6);
    }
}

#[test]
fn seeded_sampling_is_repeatable_and_does_not_substitute_force_fields() {
    for field in [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF] {
        let request = ethanol(field);
        let first = solve(&request).unwrap();
        let repeated = solve(&request).unwrap();
        assert_eq!(first.field, field);
        close(first.energy, repeated.energy, 1e-8, 0.);
        for (first, repeated) in first.coordinates.iter().zip(repeated.coordinates) {
            for axis in 0..3 {
                close(first[axis], repeated[axis], 1e-8, 0.);
            }
        }
    }
    let mut borane = molecule(&[5], &[], ForceField::UFF);
    let supported = solve(&borane).unwrap();
    assert_eq!(supported.field, ForceField::UFF);
    assert_eq!(supported.hydrogen_parents, [0, 0, 0]);
    for field in [ForceField::MMFF94, ForceField::MMFF94s] {
        borane.field = field;
        assert!(solve(&borane).unwrap_err().contains("MMFF parameters"));
    }
}

#[test]
fn changing_generation_seed_samples_a_new_flexible_conformer() {
    let edges: Vec<_> = (0..9).map(|a| (a, a + 1, 1)).collect();
    let mut request = molecule(&[6; 10], &edges, ForceField::MMFF94s);
    request.seed = 0x52534b;
    request.conformers = 8;
    let first = solve(&request).unwrap();
    // ETKDG assigns consecutive seeds to its eight samples. Advancing by a
    // larger stride avoids reusing most of the previous sample batch.
    request.seed += 1_000_003;
    let next = solve(&request).unwrap();
    let repeated = solve(&request).unwrap();
    assert_eq!(first.field, next.field);
    assert_eq!(first.hydrogen_parents, next.hydrogen_parents);
    assert_eq!(next.coordinates, repeated.coordinates);
    // Compare internal distances so a rigid rotation alone cannot count as
    // another conformer of this flexible chain.
    let squared_distance = |points: &[[f64; 3]], a: usize, b: usize| {
        points[a]
            .iter()
            .zip(points[b])
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
    };
    let mut largest_change = 0_f64;
    for a in 0..request.atoms.len() {
        for b in a + 1..request.atoms.len() {
            largest_change = largest_change.max(
                (squared_distance(&first.coordinates, a, b)
                    - squared_distance(&next.coordinates, a, b))
                .abs(),
            );
        }
    }
    assert!(
        largest_change > 0.1,
        "Different seed batches should sample distinct chain conformers: {largest_change}; energies {} and {}",
        first.energy,
        next.energy
    );
}

#[test]
fn generation_seed_contract_and_invalid_geometry_fallback_are_bounded() {
    let mut request = ethanol(ForceField::UFF);
    request.coordinates = vec![[0., 0., 0.]; 2];
    assert!(request.validate().unwrap_err().contains("original-atom"));
    request.coordinates = vec![[0., 0., 0.]; 3];
    request.fixed_atoms = vec![0];
    assert!(request.validate().unwrap_err().contains("fixed atoms"));
    request.fixed_atoms.clear();
    for invalid in [
        vec![[0., 0., 0.]; 3],
        vec![[f64::NAN, 0., 0.]; 3],
        vec![[0., 0., 0.], [1.5, 0., 1.5], [2., 1., 3.]],
    ] {
        request.coordinates = invalid;
        let generated = solve(&request).unwrap();
        assert_eq!(generated.hydrogen_parents, [0, 0, 0, 1, 1, 2]);
        assert_eq!(generated.coordinates.len(), 9);
        assert!(
            generated.diagnostics[0].starts_with("existing original-atom coordinates ignored:")
        );
        assert!(generated.diagnostics[0].len() < 600);
        assert!(
            generated
                .diagnostics
                .iter()
                .any(|line| line.contains("Rust ETKDGv3"))
        );
    }
}

#[test]
fn existing_3d_seed_adds_temporary_hydrogens_and_preserves_tetrahedral_stereo() {
    let mut request = molecule(
        &[6, 9, 17, 35],
        &[(0, 1, 1), (0, 2, 1), (0, 3, 1)],
        ForceField::MMFF94s,
    );
    request.atoms[0].chiral_tag = 1;
    let embedded = solve(&request).unwrap();
    request.coordinates = embedded.coordinates[..request.atoms.len()].to_vec();
    let seeded = solve(&request).unwrap();
    assert_eq!(seeded.hydrogen_parents, [0]);
    assert_eq!(seeded.coordinates.len(), 5);
    assert!(seeded.diagnostics[0].starts_with("initialization=existing-3d;"));
    assert!(seeded.energy <= seeded.initial_energy + 1e-6);
    evaluate(&request, seeded.coordinates.clone());
    let repeated = solve(&request).unwrap();
    close(seeded.energy, repeated.energy, 1e-8, 0.);
    for (first, second) in seeded.coordinates.iter().zip(repeated.coordinates) {
        for axis in 0..3 {
            close(first[axis], second[axis], 1e-8, 0.);
        }
    }
    for point in &mut request.coordinates {
        point[2] = -point[2];
    }
    let corrected = solve(&request).unwrap();
    assert!(corrected.diagnostics[0].starts_with("existing original-atom coordinates ignored:"));
    assert!(corrected.diagnostics[0].contains("tetrahedral stereo"));
    evaluate(&request, corrected.coordinates);
    assert_eq!(request.atoms[0].chiral_tag, 1);
}

#[test]
fn aromatic_hydrogens_and_amide_variant_parameters_are_preserved() {
    let mut benzene = molecule(
        &[6; 6],
        &[
            (0, 1, 12),
            (1, 2, 12),
            (2, 3, 12),
            (3, 4, 12),
            (4, 5, 12),
            (5, 0, 12),
        ],
        ForceField::MMFF94,
    );
    for atom in &mut benzene.atoms {
        atom.aromatic = true;
    }
    for field in [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF] {
        benzene.field = field;
        let result = solve(&benzene).unwrap();
        assert_eq!(result.coordinates.len(), 12);
        assert_eq!(result.hydrogen_parents, [0, 1, 2, 3, 4, 5]);
        assert!(result.energy <= result.initial_energy + 1e-6);
    }

    let mut amide = molecule(
        &[6, 6, 8, 7],
        &[(0, 1, 1), (1, 2, 2), (1, 3, 1)],
        ForceField::MMFF94,
    );
    let mut displaced = solve(&amide).unwrap().coordinates;
    // Displace nitrogen normal to the carbonyl plane, independent of the
    // embedded conformer's orientation, to exercise the improper terms.
    let a: [f64; 3] = std::array::from_fn(|i| displaced[0][i] - displaced[1][i]);
    let b: [f64; 3] = std::array::from_fn(|i| displaced[2][i] - displaced[1][i]);
    let normal = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let length = normal.iter().map(|x| x * x).sum::<f64>().sqrt();
    assert!(length > 1e-8);
    for (coordinate, direction) in displaced[3].iter_mut().zip(normal) {
        *coordinate += 0.5 * direction / length;
    }
    let mmff94 = evaluate(&amide, displaced.clone()).energy;
    amide.field = ForceField::MMFF94s;
    let mmff94s = evaluate(&amide, displaced).energy;
    assert!((mmff94 - mmff94s).abs() > 1e-3);
}

#[test]
fn tetrahedral_stereo_with_implicit_and_original_isotope_hydrogen_is_checked() {
    for tag in [1, 2] {
        let mut request = molecule(
            &[6, 9, 17, 35],
            &[(0, 1, 1), (0, 2, 1), (0, 3, 1)],
            ForceField::MMFF94s,
        );
        request.atoms[0].chiral_tag = tag;
        let generated = solve(&request).unwrap();
        assert_eq!(generated.hydrogen_parents, [0]);
        evaluate(&request, generated.coordinates.clone());
        request.operation = Operation::Evaluate;
        request.coordinates = generated.coordinates;
        for point in &mut request.coordinates {
            point[2] = -point[2];
        }
        assert!(solve(&request).unwrap_err().contains("tetrahedral stereo"));
        for point in &mut request.coordinates {
            point[2] = 0.;
        }
        assert!(solve(&request).is_err());
    }
    // Explicit deuterium is an original atom, not a disposable calculation H.
    let mut original_hydrogen = molecule(
        &[1, 6, 9, 17, 35],
        &[(0, 1, 1), (1, 2, 1), (1, 3, 1), (1, 4, 1)],
        ForceField::MMFF94,
    );
    original_hydrogen.atoms[0].isotope = 2;
    original_hydrogen.atoms[0].no_implicit = true;
    original_hydrogen.atoms[1].chiral_tag = 1;
    let result = solve(&original_hydrogen).unwrap();
    assert_eq!(result.original_count, 5);
    assert_eq!(result.coordinates.len(), 5);
    assert!(result.hydrogen_parents.is_empty());
    evaluate(&original_hydrogen, result.coordinates);
}

#[test]
fn e_z_and_cis_trans_references_reject_inversion_and_degeneracy() {
    for stereo in [2, 3, 4, 5] {
        let mut request = molecule(
            &[6, 6, 6, 6],
            &[(0, 1, 1), (1, 2, 2), (2, 3, 1)],
            ForceField::MMFF94,
        );
        request.bonds[1].stereo = stereo;
        request.bonds[1].stereo_atoms = Some([0, 3]);
        let generated = solve(&request).unwrap();
        evaluate(&request, generated.coordinates.clone());
        request.operation = Operation::Evaluate;
        request.coordinates = generated.coordinates;
        // Rotate one designated substituent by 180 degrees around the bond.
        let axis: [f64; 3] =
            std::array::from_fn(|i| request.coordinates[2][i] - request.coordinates[1][i]);
        let reference: [f64; 3] =
            std::array::from_fn(|i| request.coordinates[3][i] - request.coordinates[2][i]);
        let axis2: f64 = axis.iter().map(|x| x * x).sum();
        let projection: f64 = axis.iter().zip(reference).map(|(a, b)| a * b).sum::<f64>() / axis2;
        request.coordinates[3] = std::array::from_fn(|i| {
            request.coordinates[2][i] + 2. * projection * axis[i] - reference[i]
        });
        assert!(solve(&request).is_err());
        request.coordinates[0] = request.coordinates[1];
        assert!(solve(&request).is_err());
    }
}

#[test]
fn evaluation_rejects_collapsed_stretched_and_overlapping_geometry() {
    let mut request = ethanol(ForceField::MMFF94);
    let generated = solve(&request).unwrap();
    request.operation = Operation::Evaluate;
    request.coordinates = generated.coordinates.clone();
    request.coordinates[0][0] += 10.;
    assert!(solve(&request).is_err());
    request.coordinates = generated.coordinates.clone();
    request.coordinates[0] = request.coordinates[1];
    assert!(solve(&request).is_err());
    request.coordinates = generated.coordinates;
    request.coordinates[3] = request.coordinates[8];
    assert!(solve(&request).is_err());
    request.fixed_atoms.push(0);
    assert!(solve(&request).unwrap_err().contains("fixed atoms"));
}

#[test]
fn invalid_requests_and_domain_errors_leave_solver_usable() {
    let mut request = ethanol(ForceField::UFF);
    request.bonds[0].a = usize::MAX;
    assert!(solve(&request).unwrap_err().contains("bond"));
    let mut request = ethanol(ForceField::MMFF94);
    request.atoms[0].atomic_number = 30;
    assert!(!solve(&request).unwrap_err().is_empty());
    let mut request = ethanol(ForceField::UFF);
    request.atoms[0].radical = 1;
    assert!(solve(&request).unwrap_err().contains("radicals"));
    let mut request = ethanol(ForceField::UFF);
    request.bonds[0].order = 17;
    assert!(solve(&request).unwrap_err().contains("coordination"));
    let mut request = ethanol(ForceField::UFF);
    request.bonds.pop();
    assert!(solve(&request).unwrap_err().contains("connected"));
    // A domain failure must not poison subsequent solver operations.
    assert!(solve(&ethanol(ForceField::MMFF94)).is_ok());
}

fn relaxed_complex(
    numbers: &[u32],
    edges: &[(usize, usize, u32)],
    coordinates: Vec<[f64; 3]>,
) -> Request {
    let mut request = molecule(numbers, edges, ForceField::UFF);
    request.relaxed = true;
    request.coordinates = coordinates;
    request.max_iterations = 2000;
    request
}

fn planar_ring(count: usize, radius: f64) -> Vec<[f64; 3]> {
    (0..count)
        .map(|i| {
            let angle = std::f64::consts::TAU * i as f64 / count as f64;
            [radius * angle.cos(), radius * angle.sin(), 0.]
        })
        .collect()
}

fn crown(order: u32) -> Request {
    let mut numbers: Vec<_> = (0..18).map(|i| if i % 3 == 0 { 8 } else { 6 }).collect();
    numbers.push(19);
    let mut edges: Vec<_> = (0..18).map(|i| (i, (i + 1) % 18, 1)).collect();
    edges.extend((0..18).step_by(3).map(|i| (i, 18, order)));
    let mut coordinates = planar_ring(18, 3.5);
    for i in (0..18).step_by(3) {
        coordinates[i] = coordinates[i].map(|v| v * 2.8 / 3.5);
    }
    coordinates.push([0.; 3]);
    let mut request = relaxed_complex(&numbers, &edges, coordinates);
    request.atoms[18].charge = 1;
    request
}

fn ammine(metal: u32, charge: i32, count: usize) -> Request {
    let mut numbers = vec![metal];
    numbers.extend(vec![7; count]);
    let edges: Vec<_> = (1..=count).map(|i| (i, 0, 17)).collect();
    let mut coordinates = vec![[0.; 3]];
    coordinates.extend(planar_ring(count, 2.1));
    let mut request = relaxed_complex(&numbers, &edges, coordinates);
    request.atoms[0].charge = charge;
    for atom in &mut request.atoms[1..] {
        atom.explicit_h = 3;
        atom.no_implicit = true;
    }
    request
}

fn cisplatin() -> Request {
    let mut request = relaxed_complex(
        &[78, 7, 7, 17, 17],
        &[(1, 0, 17), (2, 0, 17), (0, 3, 1), (0, 4, 1)],
        vec![
            [0., 0., 0.],
            [2.1, 0., 0.],
            [0., 2.1, 0.],
            [-2.3, 0., 0.],
            [0., -2.3, 0.],
        ],
    );
    request.atoms[0].charge = 2;
    for atom in &mut request.atoms[1..3] {
        atom.explicit_h = 3;
        atom.no_implicit = true;
    }
    request
}

fn distance(points: &[[f64; 3]], a: usize, b: usize) -> f64 {
    points[a]
        .iter()
        .zip(points[b])
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn angle(points: &[[f64; 3]], a: usize, center: usize, b: usize) -> f64 {
    let dot = (0..3)
        .map(|i| (points[a][i] - points[center][i]) * (points[b][i] - points[center][i]))
        .sum::<f64>();
    (dot / (distance(points, a, center) * distance(points, b, center)))
        .clamp(-1., 1.)
        .acos()
        .to_degrees()
}

fn octahedral(points: &[[f64; 3]]) {
    let mut trans = 0;
    for a in 1..=6 {
        for b in a + 1..=6 {
            let degrees = angle(points, a, 0, b);
            if degrees > 135. {
                close(degrees, 180., 12., 0.);
                trans += 1;
            } else {
                close(degrees, 90., 12., 0.);
            }
        }
    }
    assert_eq!(trans, 3);
}

#[derive(Deserialize)]
struct RelaxedReference {
    name: String,
    request: Request,
    energy: f64,
    optimized_energy: f64,
    hydrogen_parents: Vec<usize>,
    optimized_coordinates: Vec<[f64; 3]>,
}

fn relaxed_references() -> Vec<RelaxedReference> {
    #[derive(Deserialize)]
    struct Fixture {
        rdkit_version: String,
        cases: Vec<RelaxedReference>,
    }
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/relaxed-geometry-rdkit.json"
    ))
    .unwrap();
    assert_eq!(fixture.rdkit_version, RDKIT_VERSION);
    fixture.cases
}

fn assert_relaxed_coordination_matches_reference(
    coordinates: &[[f64; 3]],
    reference: &RelaxedReference,
) -> (f64, f64) {
    let mut largest_distance_difference = 0_f64;
    let mut largest_angle_difference = 0_f64;
    for a in 1..reference.request.atoms.len() {
        largest_distance_difference = largest_distance_difference.max(
            (distance(coordinates, 0, a) - distance(&reference.optimized_coordinates, 0, a)).abs(),
        );
        for b in a + 1..reference.request.atoms.len() {
            largest_angle_difference = largest_angle_difference.max(
                (angle(coordinates, a, 0, b) - angle(&reference.optimized_coordinates, a, 0, b))
                    .abs(),
            );
        }
    }
    assert!(
        largest_distance_difference < 0.02,
        "{} distances differ by {largest_distance_difference} Å",
        reference.name
    );
    assert!(
        largest_angle_difference < 2.,
        "{} angles differ by {largest_angle_difference}°",
        reference.name
    );
    (largest_distance_difference, largest_angle_difference)
}

#[test]
fn relaxed_crown_potassium_keeps_drawn_bonds_and_hydrogen_counts() {
    let mut all_distances = Vec::new();
    for order in [1, 17] {
        let request = crown(order);
        let result = solve(&request).unwrap();
        assert_eq!(result.field, ForceField::UFF);
        assert!(result.energy.is_finite() && result.energy < result.initial_energy);
        assert_eq!(result.hydrogen_parents.len(), 24);
        assert!(
            result
                .hydrogen_parents
                .iter()
                .all(|&i| i < 18 && i % 3 != 0)
        );
        let distances: Vec<_> = (0..18)
            .step_by(3)
            .map(|i| distance(&result.coordinates, i, 18))
            .collect();
        assert!(
            result
                .approximations
                .iter()
                .any(|s| s == "Started from the drawn coordinates")
        );
        assert!(result.approximations.iter().any(|note| {
            note == "K: drawn contacts are modeled as UFF bonds and come out short"
        }));
        eprintln!(
            "crown order {order}: K–O {distances:?}; {:?}",
            result.approximations
        );
        all_distances.extend(distances);
        let mut strict = request;
        strict.relaxed = false;
        assert_eq!(
            solve(&strict).unwrap_err(),
            "atom 18: query, metal, or unsupported element"
        );
    }
    assert!(
        all_distances.iter().all(|d| (2.30..=2.45).contains(d)),
        "K–O: {all_distances:?}"
    );
}

#[test]
fn relaxed_cisplatin_matches_rdkit_uff_and_discloses_square_planar_distortion() {
    let reference = relaxed_references()
        .into_iter()
        .find(|case| case.name == "cisplatin")
        .unwrap();
    let mut request = reference.request.clone();
    request.operation = Operation::Relax;
    let result = solve(&request).unwrap();
    assert_eq!(result.field, ForceField::UFF);
    assert!(result.converged);
    assert!(result.energy < result.initial_energy);
    close(result.initial_energy, reference.energy, 1e-7, 1e-10);
    close(result.energy, reference.optimized_energy, 0.05, 0.);
    assert_eq!(result.hydrogen_parents, [1, 1, 1, 2, 2, 2]);
    assert!(
        result
            .approximations
            .iter()
            .any(|s| s == "UFF type Pt4+2 (square planar) for Pt")
    );
    assert!(
        result
            .approximations
            .iter()
            .any(|note| { note == "Pt: UFF square planar angles can bend trans ligands" })
    );
    assert_eq!(result.coordinates, solve(&request).unwrap().coordinates);
    assert_relaxed_coordination_matches_reference(&result.coordinates, &reference);
}

#[test]
fn relaxed_cobalt_ammine_and_chelates_are_octahedral() {
    let ammine = ammine(27, 3, 6);
    let mut chelate = ammine.clone();
    for atom in &mut chelate.atoms[1..] {
        atom.explicit_h = 2;
    }
    for pair in 0..3 {
        let first = chelate.atoms.len();
        for offset in 0..2 {
            chelate.atoms.push(AtomInput {
                atomic_number: 6,
                explicit_h: 0,
                no_implicit: false,
                ..chelate.atoms[1].clone()
            });
            let direction = std::f64::consts::TAU * (pair * 2) as f64 / 6.
                + (offset as f64 + 0.5) * std::f64::consts::PI / 6.;
            chelate
                .coordinates
                .push([3.1 * direction.cos(), 3.1 * direction.sin(), 0.]);
        }
        for (a, b) in [
            (1 + pair * 2, first),
            (first, first + 1),
            (first + 1, 2 + pair * 2),
        ] {
            chelate.bonds.push(BondInput {
                a,
                b,
                order: 1,
                aromatic: false,
                stereo: 0,
                stereo_atoms: None,
            });
        }
    }
    for (name, request) in [("Co(NH3)6", ammine), ("Co(en)3", chelate)] {
        let result = solve(&request).unwrap();
        octahedral(&result.coordinates);
        assert!(
            result
                .approximations
                .iter()
                .any(|s| s == "UFF type Co6+3 (octahedral) for Co")
        );
        assert!(!result.hydrogen_parents.contains(&0));
        eprintln!(
            "{name} Co–N {:?}",
            (1..7)
                .map(|i| distance(&result.coordinates, 0, i))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn relaxed_iron_carbonyl_and_copper_report_approximate_geometry() {
    let mut numbers = vec![26];
    numbers.extend([6, 8].repeat(5));
    let mut edges = Vec::new();
    let mut coordinates = vec![[0.; 3]];
    for (i, direction) in planar_ring(5, 1.).into_iter().enumerate() {
        edges.extend([(0, 1 + 2 * i, 1), (1 + 2 * i, 2 + 2 * i, 3)]);
        coordinates.push(direction.map(|v| v * 1.8));
        coordinates.push(direction.map(|v| v * 2.95));
    }
    let mut iron = relaxed_complex(&numbers, &edges, coordinates);
    for atom in &mut iron.atoms {
        atom.no_implicit = true;
    }
    let result = solve(&iron).unwrap();
    assert!(result.hydrogen_parents.is_empty());
    assert!(
        result
            .approximations
            .iter()
            .any(|s| s.contains("Fe") && s.contains("5 neighbors drawn")),
        "{:?}",
        result.approximations
    );
    assert!(
        result
            .approximations
            .iter()
            .any(|s| s.contains("Fe6+2") || s.contains("Fe3+2"))
    );
    iron.relaxed = false;
    assert_eq!(
        solve(&iron).unwrap_err(),
        "atom 0: query, metal, or unsupported element"
    );
    let copper = solve(&ammine(29, 2, 4)).unwrap();
    assert!(
        copper
            .approximations
            .iter()
            .any(|s| s.contains("Cu3+1") && s.contains("tetrahedral")),
        "{:?}",
        copper.approximations
    );
    eprintln!(
        "Fe: {:?}; Cu: {:?}",
        result.approximations, copper.approximations
    );
}

#[test]
fn relaxed_notes_deduplicate_elements_keep_stable_order_and_format_drawn_charges() {
    for drawn_charge in [0, 2] {
        let mut request = relaxed_complex(
            &[29, 8, 12, 8, 29],
            &[(0, 1, 1), (1, 2, 1), (2, 3, 1), (3, 4, 1)],
            vec![
                [0., 0., 0.],
                [2., 0., 0.],
                [3.5, 1., 0.],
                [5.2, 0.3, 0.2],
                [7., 1., 0.],
            ],
        );
        request.operation = Operation::Evaluate;
        for atom in &mut request.atoms {
            atom.no_implicit = true;
        }
        request.atoms[0].charge = drawn_charge;
        request.atoms[4].charge = drawn_charge;
        request.atoms[2].charge = 2;
        let charge_text = if drawn_charge == 0 { "0" } else { "+2" };
        let expected = vec![
            "UFF type Cu3+1 (tetrahedral) for Cu".to_string(),
            "Cu: 1 neighbors drawn; UFF type Cu3+1 assumes 4".to_string(),
            format!("Cu: UFF type Cu3+1 assumes charge +1; drawn {charge_text}"),
            "UFF type Mg3+2 (tetrahedral) for Mg".to_string(),
            "Mg: 2 neighbors drawn; UFF type Mg3+2 assumes 4".to_string(),
            "Mg: drawn contacts are modeled as UFF bonds and come out short".to_string(),
        ];
        assert_eq!(solve(&request).unwrap().approximations, expected);
        // Put Mg before Cu in the input without changing the calculation graph.
        request.atoms.rotate_left(2);
        request.coordinates.rotate_left(2);
        for bond in &mut request.bonds {
            bond.a = (bond.a + 3) % 5;
            bond.b = (bond.b + 3) % 5;
        }
        assert_eq!(solve(&request).unwrap().approximations, expected);
    }
}

#[test]
fn relaxed_hypervalent_main_group_and_extra_bond_orders_use_uff() {
    let mut request = relaxed_complex(
        &[54, 9, 9, 9, 9],
        &[(0, 1, 1), (0, 2, 1), (0, 3, 1), (0, 4, 1)],
        vec![
            [0., 0., 0.],
            [2., 0., 0.],
            [0., 2., 0.],
            [-2., 0., 0.],
            [0., -2., 0.],
        ],
    );
    let result = solve(&request).unwrap();
    assert!(result.hydrogen_parents.is_empty());
    assert!(
        result
            .approximations
            .iter()
            .any(|note| { note == "UFF type Xe4+4 (square planar) for Xe" })
    );
    assert!(
        result
            .approximations
            .iter()
            .any(|note| { note == "Xe: UFF square planar angles can bend trans ligands" })
    );
    request.relaxed = false;
    assert_eq!(
        solve(&request).unwrap_err(),
        "atom 0: query, metal, or unsupported element"
    );
    for order in [4, 7] {
        let mut request =
            relaxed_complex(&[6, 6], &[(0, 1, order)], vec![[0., 0., 0.], [1.4, 0., 0.]]);
        for atom in &mut request.atoms {
            atom.no_implicit = true;
        }
        let result = solve(&request).unwrap();
        assert!(result.energy.is_finite());
    }
}

#[test]
fn relaxed_errors_name_uff_or_missing_atoms_and_strict_defaults_survive_wire() {
    let mut platinum = cisplatin();
    for field in [ForceField::MMFF94, ForceField::MMFF94s] {
        platinum.field = field;
        assert!(solve(&platinum).unwrap_err().contains("UFF"));
    }
    let mut unsupported = relaxed_complex(&[118], &[], vec![[0.; 3]]);
    unsupported.atoms[0].no_implicit = true;
    let message = solve(&unsupported).unwrap_err();
    assert!(
        message.contains("Og") && message.contains("UFF") && !message.contains("atom 0"),
        "{message}"
    );
    for mut request in [cisplatin(), ammine(27, 3, 6), ammine(29, 2, 4)] {
        request.relaxed = false;
        assert_eq!(
            solve(&request).unwrap_err(),
            "atom 0: query, metal, or unsupported element"
        );
    }
    let mut json = serde_json::to_value(ethanol(ForceField::UFF)).unwrap();
    assert!(!json.as_object().unwrap().contains_key("relaxed"));
    json.as_object_mut().unwrap().remove("relaxed");
    let legacy: Request = serde_json::from_value(json).unwrap();
    assert!(!legacy.relaxed);
    let result = solve(&legacy).unwrap();
    assert!(result.approximations.is_empty());
    let mut json = serde_json::to_value(result).unwrap();
    assert!(!json.as_object().unwrap().contains_key("approximations"));
    json.as_object_mut().unwrap().remove("approximations");
    assert!(
        serde_json::from_value::<Response>(json)
            .unwrap()
            .approximations
            .is_empty()
    );
}

#[test]
fn relaxed_implicit_hydrogens_radicals_and_stereo_still_follow_the_drawing() {
    let mut nitrogen_donors = ammine(27, 3, 6);
    for atom in &mut nitrogen_donors.atoms[1..] {
        atom.explicit_h = 0;
        atom.no_implicit = false;
    }
    let result = solve(&nitrogen_donors).unwrap();
    assert_eq!(
        result.hydrogen_parents,
        (1..=6).flat_map(|i| [i; 3]).collect::<Vec<_>>()
    );
    let mut radical = molecule(&[6], &[], ForceField::UFF);
    radical.relaxed = true;
    radical.atoms[0].radical = 1;
    assert_eq!(solve(&radical).unwrap().hydrogen_parents, [0, 0, 0]);
    let mut chiral = molecule(
        &[6, 9, 17, 35],
        &[(0, 1, 1), (0, 2, 1), (0, 3, 1)],
        ForceField::UFF,
    );
    chiral.relaxed = true;
    chiral.atoms[0].chiral_tag = 1;
    let result = solve(&chiral).unwrap();
    assert_eq!(result.hydrogen_parents, [0]);
    chiral.coordinates = result.coordinates[..chiral.atoms.len()]
        .iter()
        .map(|point| [point[0], point[1], 0.])
        .collect();
    let regenerated = solve(&chiral).unwrap();
    assert!(regenerated.approximations.iter().any(|note| {
        note == "Started from generated coordinates to preserve drawn stereochemistry"
    }));
    chiral.operation = Operation::Evaluate;
    chiral.coordinates = result.coordinates;
    solve(&chiral).unwrap();
    for point in &mut chiral.coordinates {
        point[2] = -point[2];
    }
    assert!(solve(&chiral).unwrap_err().contains("tetrahedral stereo"));
}

#[test]
fn relaxed_evaluation_keeps_finite_bond_and_overlap_checks() {
    let mut request = cisplatin();
    let result = solve(&request).unwrap();
    request.operation = Operation::Evaluate;
    for replacement in [
        [f64::NAN, 0., 0.],
        result.coordinates[0],
        result.coordinates[2],
    ] {
        request.coordinates = result.coordinates.clone();
        request.coordinates[1] = replacement;
        assert!(solve(&request).is_err());
    }
}

#[test]
fn relaxed_uff_matches_pinned_rdkit_energy_and_optimized_coordination() {
    for case in relaxed_references() {
        let evaluated = solve(&case.request).unwrap();
        close(evaluated.energy, case.energy, 1e-7, 1e-10);
        assert_eq!(evaluated.hydrogen_parents, case.hydrogen_parents);
        let mut request = case.request.clone();
        request.operation = Operation::Relax;
        let optimized = solve(&request).unwrap();
        let (largest_distance_difference, largest_angle_difference) =
            assert_relaxed_coordination_matches_reference(&optimized.coordinates, &case);
        eprintln!(
            "{}: native energy {:.12}, RDKit {:.12}; optimized maximum differences {:.6} Å, {:.6}°",
            case.name,
            evaluated.energy,
            case.energy,
            largest_distance_difference,
            largest_angle_difference
        );
    }
}

#[test]
fn relaxed_ethanol_retains_all_fields_without_approximation_notes() {
    for field in [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF] {
        let mut request = ethanol(field);
        request.relaxed = true;
        let result = solve(&request).unwrap();
        assert_eq!(result.hydrogen_parents, [0, 0, 0, 1, 1, 2]);
        assert_eq!(result.field, field);
        assert!(result.approximations.is_empty());
    }
}

#[test]
fn relaxed_implicit_hydrogens_respect_drawn_ionic_charge() {
    for (number, charge, expected_parents) in
        [(8, -1, vec![0, 0, 0]), (7, 1, vec![0, 0, 0, 1, 1, 1])]
    {
        let mut request = molecule(&[6, number], &[(0, 1, 1)], ForceField::UFF);
        request.relaxed = true;
        request.atoms[1].charge = charge;
        let result = solve(&request).unwrap();
        assert_eq!(result.hydrogen_parents, expected_parents);
    }
}
