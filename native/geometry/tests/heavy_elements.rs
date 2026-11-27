//! Heavy-element drawings and rough ligand placement for xTB refinement.
//! Fixture provenance is in fixtures/README.md.
use reshiki_geometry::{AtomInput, BondInput, ForceField, Operation, Request, Response, solve};

fn request(json: &str) -> Request {
    let request: Request = serde_json::from_str(json).unwrap();
    request.validate().unwrap();
    request
}

fn distance(response: &Response, a: usize, b: usize) -> f64 {
    let [p, q] = [a, b].map(|i| response.coordinates[i]);
    (0..3).map(|i| (p[i] - q[i]).powi(2)).sum::<f64>().sqrt()
}

fn single_bond(a: usize, b: usize) -> BondInput {
    BondInput {
        a,
        b,
        order: 1,
        aromatic: false,
        stereo: 0,
        stereo_atoms: None,
    }
}

fn atom(atomic_number: u32) -> AtomInput {
    AtomInput {
        atomic_number,
        isotope: 0,
        charge: 0,
        explicit_h: 0,
        no_implicit: true,
        aromatic: false,
        radical: 0,
        chiral_tag: 0,
    }
}

fn flat_star(centre: u32, ligand: u32, count: usize) -> Request {
    let mut atoms = vec![atom(centre)];
    atoms.extend((0..count).map(|_| atom(ligand)));
    let mut coordinates = vec![[0.; 3]];
    // Start the two-coordinate case bent as well as flat.
    let step = std::f64::consts::TAU / count.max(3) as f64;
    coordinates.extend((0..count).map(|i| {
        let angle = step * i as f64;
        [2.1 * angle.cos(), 2.1 * angle.sin(), 0.]
    }));
    Request {
        atoms,
        bonds: (1..=count).map(|i| single_bond(0, i)).collect(),
        relaxed: true,
        rough: false,
        field: ForceField::UFF,
        operation: Operation::Generate,
        ensemble: None,
        coordinates,
        fixed_atoms: Vec::new(),
        conformers: 1,
        seed: 61453,
        max_iterations: 500,
    }
}

fn ligand_angles(response: &Response, centre: usize, ligands: &[usize]) -> Vec<f64> {
    let mut angles = Vec::new();
    for (index, &a) in ligands.iter().enumerate() {
        for &b in &ligands[index + 1..] {
            let [u, v] = [a, b].map(|i| {
                std::array::from_fn::<_, 3, _>(|axis| {
                    response.coordinates[i][axis] - response.coordinates[centre][axis]
                })
            });
            let cosine = (0..3).map(|i| u[i] * v[i]).sum::<f64>()
                / (distance(response, a, centre) * distance(response, b, centre));
            angles.push(cosine.clamp(-1., 1.).acos().to_degrees());
        }
    }
    angles
}

#[test]
fn rough_flat_heavy_element_drawings_spread_ligands_for_xtb_refinement() {
    for (name, symbol, centre, ligand, count) in [
        ("CeF3", "Ce", 58, 9, 3),
        ("ThCl4", "Th", 90, 17, 4),
        ("XeF4", "Xe", 54, 9, 4),
        ("XeF2", "Xe", 54, 9, 2),
    ] {
        for field in [ForceField::UFF, ForceField::MMFF94s] {
            let mut request = flat_star(centre, ligand, count);
            request.field = field;
            let refusal = solve(&request).unwrap_err();
            assert!(
                refusal.starts_with("ReShiki can't build a reliable 3D shape for this yet"),
                "{name}: {refusal}"
            );

            request.rough = true;
            let response = solve(&request).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(response.field, ForceField::UFF);
            assert_eq!(response.original_count, request.atoms.len());
            assert_eq!(response.coordinates.len(), request.atoms.len());
            assert!(response.hydrogen_parents.is_empty());
            assert!(
                response
                    .coordinates
                    .iter()
                    .flatten()
                    .all(|value| value.is_finite())
            );
            assert!(response.approximations.contains(&format!(
                "Rough start for an xTB refinement: {symbol} ligands spread out"
            )));

            let ligands: Vec<_> = (1..=count).collect();
            let angles = ligand_angles(&response, 0, &ligands);
            let lengths: Vec<_> = ligands.iter().map(|&i| distance(&response, 0, i)).collect();
            // Run with --nocapture to include all measured values in the QA report.
            println!(
                "{name} {field:?}: ligand angles (degrees) = {angles:.6?}; bond lengths (A) = {lengths:.6?}"
            );
            let expected = if count == 2 { 180. } else { 109.5 };
            for degrees in angles {
                assert!(
                    (degrees - expected).abs() <= 2.,
                    "{name}: {degrees} degrees"
                );
            }
            for length in lengths {
                assert!((1.5..=3.5).contains(&length), "{name}: {length} A");
            }
        }
    }
}

#[test]
fn rough_krypton_difluoride_without_coordinates_is_linear() {
    let mut request = flat_star(36, 9, 2);
    request.rough = true;
    request.field = ForceField::MMFF94s;
    // The GUI omits coordinates for flat drawings, so Generate uses ETKDG.
    request.coordinates.clear();
    request.validate().unwrap();

    let response = solve(&request).unwrap();
    assert_eq!(response.field, ForceField::UFF);
    assert_eq!(response.original_count, 3);
    assert_eq!(response.coordinates.len(), 3);
    let degrees = ligand_angles(&response, 0, &[1, 2])[0];
    assert!((degrees - 180.).abs() <= 2., "F-Kr-F: {degrees} degrees");
}

#[test]
fn rough_thorium_methoxide_keeps_ligands_and_added_hydrogens_intact() {
    let mut request = flat_star(90, 8, 4);
    request.rough = true;
    for oxygen in 1..=4 {
        let carbon = request.atoms.len();
        let mut methyl = atom(6);
        methyl.explicit_h = 3;
        request.atoms.push(methyl);
        request.bonds.push(single_bond(oxygen, carbon));
        request
            .coordinates
            .push(request.coordinates[oxygen].map(|v| v * 3.5 / 2.1));
    }
    let response = solve(&request).unwrap();
    assert_eq!(response.original_count, 9);
    assert_eq!(
        response.hydrogen_parents,
        [5, 5, 5, 6, 6, 6, 7, 7, 7, 8, 8, 8]
    );
    for oxygen in 1..=4 {
        let carbon = oxygen + 4;
        let length = distance(&response, oxygen, carbon);
        assert!((1.35..=1.45).contains(&length), "O-C {length} A");
    }
    let ch_lengths: Vec<_> = response
        .hydrogen_parents
        .iter()
        .enumerate()
        .map(|(index, &parent)| distance(&response, parent, response.original_count + index))
        .collect();
    println!("Th(OMe)4 C-H lengths (A) = {ch_lengths:.6?}");
    for length in ch_lengths {
        assert!((1.0..=1.2).contains(&length), "C-H {length} A");
    }
}

// These starts already span 3D, so Generate and Relax use the same seed.
// Center it to match generation's optimizer origin.
fn rough_and_uff(mut request: Request) -> (Response, Response) {
    let origin = std::array::from_fn::<_, 3, _>(|axis| {
        request.coordinates.iter().map(|p| p[axis]).sum::<f64>() / request.coordinates.len() as f64
    });
    for point in &mut request.coordinates {
        for axis in 0..3 {
            point[axis] -= origin[axis];
        }
    }
    let uff = solve(&Request {
        operation: Operation::Relax,
        ..request.clone()
    })
    .unwrap();
    request.rough = true;
    let rough = solve(&request).unwrap();
    assert!(
        !rough
            .approximations
            .iter()
            .any(|note| note.contains("ligands spread out")),
        "{:?}",
        rough.approximations
    );
    (rough, uff)
}

#[test]
fn rough_cerium_chelate_keeps_uff_carbon_oxygen_lengths() {
    let mut request = flat_star(58, 8, 3);
    request.atoms[3] = atom(9);
    request.atoms.push(atom(6));
    request.bonds.extend([single_bond(1, 4), single_bond(2, 4)]);
    request.coordinates = vec![
        [0., 0., 0.],
        [2., 0., 0.],
        [0., 2., 0.],
        [-1.7, -1.2, 0.4],
        [1.6, 1.6, 0.3],
    ];
    let (rough, uff) = rough_and_uff(request);
    for oxygen in [1, 2] {
        let [actual, expected] = [&rough, &uff].map(|r| distance(r, oxygen, 4));
        assert!(
            (actual - expected).abs() <= 0.02,
            "O{oxygen}-C: rough {actual} A, UFF {expected} A"
        );
    }
}

#[test]
fn rough_bridged_cerium_dimer_keeps_uff_coordinates() {
    let mut request = flat_star(58, 9, 3);
    request.atoms = [58, 58, 8, 9, 9, 9, 9].into_iter().map(atom).collect();
    request.bonds = [(0, 2), (1, 2), (0, 3), (0, 4), (1, 5), (1, 6)]
        .into_iter()
        .map(|(a, b)| single_bond(a, b))
        .collect();
    request.coordinates = vec![
        [-2., 0., 0.],
        [2., 0., 0.],
        [0., 0., 0.],
        [-2., -2., 0.5],
        [-2., 2., -0.5],
        [2., -2., -0.5],
        [2., 2., 0.5],
    ];
    let (rough, uff) = rough_and_uff(request);
    for (actual, expected) in rough
        .coordinates
        .iter()
        .flatten()
        .zip(uff.coordinates.iter().flatten())
    {
        assert!((actual - expected).abs() < 1e-6);
    }
}

#[test]
fn rough_directly_bonded_donors_keep_uff_coordinates() {
    let mut request = flat_star(58, 8, 3);
    request.atoms[3] = atom(9);
    request.bonds.push(single_bond(1, 2));
    request.coordinates = vec![
        [0., 0., 0.],
        [2., 0., 0.],
        [1.5, 1.3, 0.3],
        [-2., -0.5, -0.5],
    ];
    let (rough, uff) = rough_and_uff(request);
    for (actual, expected) in rough
        .coordinates
        .iter()
        .flatten()
        .zip(uff.coordinates.iter().flatten())
    {
        assert!((actual - expected).abs() < 1e-6);
    }
}

#[test]
fn rough_option_preserves_the_existing_uranium_hexafluoride_geometry() {
    let request = request(include_str!("fixtures/uf6-request.json"));
    let baseline = solve(&request).unwrap();
    let rough = solve(&Request {
        rough: true,
        ..request.clone()
    })
    .unwrap();
    assert_eq!(rough.coordinates, baseline.coordinates);
    assert_eq!(rough.hydrogen_parents, baseline.hydrogen_parents);
    assert_eq!(rough.energy, baseline.energy);
    assert_eq!(rough.approximations, baseline.approximations);
    assert_octahedral(&request, &rough, 1);
}

/// The six ligand angles at `centre`: twelve near 90° and three near 180°.
fn assert_octahedral(request: &Request, response: &Response, centre: usize) {
    let ligands: Vec<_> = request
        .bonds
        .iter()
        .filter_map(|b| {
            (b.a == centre)
                .then_some(b.b)
                .or((b.b == centre).then_some(b.a))
        })
        .collect();
    assert_eq!(ligands.len(), 6);
    let mut trans = 0;
    for (index, &a) in ligands.iter().enumerate() {
        for &b in &ligands[index + 1..] {
            let [u, v] = [a, b].map(|i| {
                let [p, c] = [response.coordinates[i], response.coordinates[centre]];
                [p[0] - c[0], p[1] - c[1], p[2] - c[2]]
            });
            let cosine = (0..3).map(|i| u[i] * v[i]).sum::<f64>()
                / (distance(response, a, centre) * distance(response, b, centre));
            let degrees = cosine.clamp(-1., 1.).acos().to_degrees();
            let expected = if degrees > 135. {
                trans += 1;
                180.
            } else {
                90.
            };
            assert!(
                (degrees - expected).abs() < 5.,
                "{a}-{centre}-{b}: {degrees}°"
            );
        }
    }
    assert_eq!(trans, 3);
}

#[test]
fn flat_uranium_hexafluoride_drawing_becomes_an_octahedron() {
    // On main two fluorines collapsed to 30° (F···F 1.19 Å): UFF's octahedral
    // angle term has a minimum at 0° and UFF has no 1-3 repulsion.
    // The view asks for MMFF94s and gets UFF; the Optimization panel asks for UFF.
    for field in [ForceField::UFF, ForceField::MMFF94s] {
        let request = Request {
            field,
            ..request(include_str!("fixtures/uf6-request.json"))
        };
        let response = solve(&request).unwrap();
        assert_eq!(response.field, ForceField::UFF);
        assert_octahedral(&request, &response, 1);
        for fluorine in [0, 2, 3, 4, 5, 6] {
            let length = distance(&response, 1, fluorine);
            assert!((2.1..2.25).contains(&length), "U-F {length} A");
        }
    }
}

#[test]
fn hexanitratocerate_keeps_its_nitrates_intact() {
    // Lifting only the donor oxygens tore the planar nitrates apart (N-O
    // 0.71 Å); on main they collapsed into each other (0.74 Å contacts).
    let request = Request {
        field: ForceField::UFF,
        ..request(include_str!("fixtures/ce-nitrate6-request.json"))
    };
    let response = solve(&request).unwrap();
    assert_octahedral(&request, &response, 0);
    for bond in &request.bonds {
        let length = distance(&response, bond.a, bond.b);
        let range = if bond.a == 0 || bond.b == 0 {
            2.1..2.6
        } else {
            1.15..1.45
        };
        assert!(range.contains(&length), "{bond:?}: {length} A");
    }
    let n = response.coordinates.len();
    for a in 0..n {
        for b in a + 1..n {
            assert!(distance(&response, a, b) > 1.1, "atoms {a} and {b} overlap");
        }
    }
}
