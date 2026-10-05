//! Reported embedding failures, using the application's native InChI graphs.
//! Fixture provenance and exact source InChIs are in fixtures/README.md.
use reshiki_geometry::{ForceField, Operation, Request, Response, solve};

type Point = [f64; 3];

fn taxol() -> Request {
    serde_json::from_str(include_str!("fixtures/taxol70-request.json")).unwrap()
}

fn user_c36() -> Request {
    serde_json::from_str(include_str!("fixtures/user-c36-request.json")).unwrap()
}

fn sub(a: Point, b: Point) -> Point {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn dot(a: Point, b: Point) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn assert_geometry_and_identity(request: &Request, response: &Response, added_h: usize) {
    let original_count = request.atoms.len();
    assert_eq!(response.field, request.field);
    assert_eq!(response.original_count, original_count);
    assert_eq!(response.coordinates.len(), original_count + added_h);
    assert_eq!(response.hydrogen_parents.len(), added_h);
    assert!(
        response
            .coordinates
            .iter()
            .flatten()
            .all(|value| value.is_finite())
    );
    assert!(response.initial_energy.is_finite() && response.energy.is_finite());
    assert!(response.energy <= response.initial_energy + 1e-6);
    assert!(response.gradient.is_none());
    // InChI supplies all H counts explicitly. Original graph hydrogens must
    // retain their indices; only the remaining H atoms may be appended.
    assert!(request.atoms.iter().all(|atom| atom.no_implicit));
    let expected_parents: Vec<_> = request
        .atoms
        .iter()
        .enumerate()
        .flat_map(|(index, atom)| std::iter::repeat_n(index, atom.explicit_h as usize))
        .collect();
    assert_eq!(response.hydrogen_parents, expected_parents);
    let mut bonds: Vec<_> = request.bonds.iter().map(|bond| (bond.a, bond.b)).collect();
    bonds.extend(
        response
            .hydrogen_parents
            .iter()
            .enumerate()
            .map(|(offset, &parent)| {
                assert!(parent < original_count);
                assert_ne!(request.atoms[parent].atomic_number, 1);
                (parent, original_count + offset)
            }),
    );
    let mut neighbors = vec![Vec::new(); response.coordinates.len()];
    for (a, b) in bonds {
        let delta = sub(response.coordinates[a], response.coordinates[b]);
        let length = dot(delta, delta).sqrt();
        assert!(
            (0.65..2.15).contains(&length),
            "{:?}: bond {a}–{b} length {length}",
            request.field
        );
        neighbors[a].push(b);
        neighbors[b].push(a);
    }
    for (a, &point) in response.coordinates.iter().enumerate() {
        for (offset, &other) in response.coordinates[a + 1..].iter().enumerate() {
            let delta = sub(point, other);
            assert!(
                dot(delta, delta) > 0.25,
                "atoms {a} and {} overlap",
                a + 1 + offset
            );
        }
    }
    for (atom, input) in request.atoms.iter().enumerate() {
        if input.chiral_tag == 0 {
            continue;
        }
        // These Taxol stereocenters all have four graph neighbors, including
        // the retained explicit H. The ordered scalar triple product verifies
        // their configuration independently of CIP labels or wedge drawing.
        let adjacent = &neighbors[atom];
        assert_eq!(adjacent.len(), 4);
        let origin = response.coordinates[adjacent[0]];
        let volume = dot(
            sub(response.coordinates[adjacent[1]], origin),
            cross(
                sub(response.coordinates[adjacent[2]], origin),
                sub(response.coordinates[adjacent[3]], origin),
            ),
        );
        let signed = if input.chiral_tag == 1 {
            volume
        } else {
            -volume
        };
        assert!(
            signed > 0.1,
            "atom {atom} lost its ordered configuration: {volume}"
        );
    }
}

fn assert_same_coordinate_evaluation(request: &Request, generated: &Response) {
    let mut evaluation = request.clone();
    evaluation.operation = Operation::Evaluate;
    evaluation.coordinates = generated.coordinates.clone();
    let response = solve(&evaluation).unwrap();
    assert_eq!(response.coordinates, generated.coordinates);
    assert_eq!(response.hydrogen_parents, generated.hydrogen_parents);
    assert!(
        (response.energy - generated.energy).abs() <= 1e-6 + 1e-8 * generated.energy.abs(),
        "{:?}: generated energy {} vs evaluated {}",
        request.field,
        generated.energy,
        response.energy
    );
    let gradient = response.gradient.unwrap();
    assert_eq!(gradient.len(), generated.coordinates.len());
    assert!(
        gradient
            .iter()
            .flatten()
            .all(|component| component.is_finite())
    );
}

#[test]
fn app_taxol_graph_preserves_original_stereo_hydrogens_during_generation() {
    let request = taxol();
    request.validate().unwrap();
    assert_eq!(request.atoms.len(), 70);
    assert_eq!(request.bonds.len(), 76);
    assert_eq!(request.field, ForceField::MMFF94s);
    assert_eq!(request.conformers, 8);
    let original_h: Vec<_> = request
        .atoms
        .iter()
        .enumerate()
        .filter_map(|(index, atom)| (atom.atomic_number == 1).then_some(index))
        .collect();
    assert_eq!(original_h, (62..70).collect::<Vec<_>>());
    assert_eq!(
        request
            .atoms
            .iter()
            .filter(|atom| atom.chiral_tag != 0)
            .count(),
        11
    );
    let source = serde_json::to_value(&request).unwrap();
    let generated = solve(&request).unwrap();
    assert_geometry_and_identity(&request, &generated, 43);
    // A valid preview remains useful when the bounded 500-iteration force-field
    // pass has not converged. Do not confuse that state with embedding failure.
    assert_same_coordinate_evaluation(&request, &generated);
    assert_eq!(serde_json::to_value(&request).unwrap(), source);
}

#[test]
fn user_c36_graph_generates_with_each_field_without_fullerene_shortcut() {
    for field in [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF] {
        let mut request = user_c36();
        request.field = field;
        request.conformers = 1;
        request.validate().unwrap();
        assert_eq!(request.atoms.len(), 36);
        assert_eq!(request.bonds.len(), 42);
        assert!(
            request
                .atoms
                .iter()
                .all(|atom| atom.atomic_number == 6 && atom.chiral_tag == 0)
        );
        assert!(request.bonds.iter().all(|bond| bond.stereo == 0));
        let source = serde_json::to_value(&request).unwrap();
        let generated = solve(&request).unwrap_or_else(|error| panic!("{field:?}: {error}"));
        assert_geometry_and_identity(&request, &generated, 24);
        assert!(
            generated.converged,
            "{field:?}: {:?}",
            generated.diagnostics
        );
        assert!(
            generated
                .diagnostics
                .iter()
                .all(|line| !line.contains("cage-ETDG")),
            "C36 is not a fullerene: {:?}",
            generated.diagnostics
        );
        assert_same_coordinate_evaluation(&request, &generated);
        assert_eq!(serde_json::to_value(&request).unwrap(), source);
    }
}
