//! Complete fullerene regression, including its closed spatial cage.
//! The fixture retains the user's InChI graph's original atom/bond indices.
use reshiki_geometry::{ForceField, Operation, Request, Response, solve};
use serde::Deserialize;
use std::collections::BTreeMap;

type Point = [f64; 3];
const FIELDS: [ForceField; 3] = [ForceField::MMFF94, ForceField::MMFF94s, ForceField::UFF];

#[derive(Deserialize)]
struct Reference {
    coordinates: Vec<Point>,
    faces: Vec<Vec<usize>>,
}

fn request() -> Request {
    serde_json::from_str(include_str!("fixtures/c60-request.json")).unwrap()
}

fn reference() -> Reference {
    serde_json::from_str(include_str!("fixtures/c60-reference.json")).unwrap()
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

fn norm(point: Point) -> f64 {
    dot(point, point).sqrt()
}

fn mean(points: impl IntoIterator<Item = Point>) -> Point {
    let mut total = [0.0; 3];
    let mut count = 0;
    for point in points {
        for axis in 0..3 {
            total[axis] += point[axis];
        }
        count += 1;
    }
    total.map(|value| value / f64::from(count))
}

fn edge(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

fn point_segment_distance(point: Point, a: Point, b: Point) -> f64 {
    let direction = sub(b, a);
    let fraction = (dot(sub(point, a), direction) / dot(direction, direction)).clamp(0.0, 1.0);
    norm(std::array::from_fn(|axis| {
        point[axis] - a[axis] - fraction * direction[axis]
    }))
}

/// The minimum is at an endpoint or the unconstrained interior closest pair.
fn segment_distance(a: Point, b: Point, c: Point, d: Point) -> f64 {
    let u = sub(b, a);
    let v = sub(d, c);
    let w = sub(a, c);
    let (uu, uv, vv, uw, vw) = (dot(u, u), dot(u, v), dot(v, v), dot(u, w), dot(v, w));
    let mut distance = point_segment_distance(a, c, d)
        .min(point_segment_distance(b, c, d))
        .min(point_segment_distance(c, a, b))
        .min(point_segment_distance(d, a, b));
    let denominator = uu * vv - uv * uv;
    if denominator > 1e-12 {
        let s = (uv * vw - vv * uw) / denominator;
        let t = (uu * vw - uv * uw) / denominator;
        if (0.0..=1.0).contains(&s) && (0.0..=1.0).contains(&t) {
            distance = distance.min(norm(std::array::from_fn(|axis| {
                w[axis] + s * u[axis] - t * v[axis]
            })));
        }
    }
    distance
}

fn assert_cage(request: &Request, coordinates: &[Point], faces: &[Vec<usize>]) {
    assert_eq!(coordinates.len(), 60);
    assert!(coordinates.iter().flatten().all(|value| value.is_finite()));
    let center = mean(coordinates.iter().copied());
    let radii: Vec<_> = coordinates
        .iter()
        .map(|&point| norm(sub(point, center)))
        .collect();
    let smallest = radii.iter().copied().fold(f64::INFINITY, f64::min);
    let largest = radii.iter().copied().fold(0.0, f64::max);
    // Broad bounds: avoid comparing a stochastic conformer against exact XYZ.
    assert!(
        smallest > 3.0 && largest < 4.2,
        "cage radii {smallest}..{largest}"
    );
    assert!(
        largest / smallest < 1.20,
        "cage is not approximately spherical"
    );
    let spans: [f64; 3] = std::array::from_fn(|axis| {
        let minimum = coordinates
            .iter()
            .map(|p| p[axis])
            .fold(f64::INFINITY, f64::min);
        let maximum = coordinates
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        maximum - minimum
    });
    assert!(
        spans.into_iter().fold(0.0, f64::max) / spans.into_iter().fold(f64::INFINITY, f64::min)
            < 1.25,
        "flattened cage: axis spans {spans:?}"
    );
    for bond in &request.bonds {
        let length = norm(sub(coordinates[bond.a], coordinates[bond.b]));
        assert!(
            (1.1..1.8).contains(&length),
            "bond {}–{} has length {length}",
            bond.a,
            bond.b
        );
    }
    // Each of the 32 graph faces must support the same convex cage. A merely
    // spherical cloud with folded faces or swapped atom positions fails here.
    for face in faces {
        let midpoint = mean(face.iter().map(|&atom| coordinates[atom]));
        let mut normal = [0.0; 3];
        for (&a, &b) in face.iter().zip(face.iter().cycle().skip(1)) {
            let contribution = cross(sub(coordinates[a], midpoint), sub(coordinates[b], midpoint));
            for axis in 0..3 {
                normal[axis] += contribution[axis];
            }
        }
        let size = norm(normal);
        assert!(size > 1.0, "degenerate face {face:?}");
        normal = normal.map(|value| value / size);
        if dot(normal, sub(midpoint, center)) < 0.0 {
            normal = normal.map(|value| -value);
        }
        assert!(dot(normal, sub(midpoint, center)) > 2.0);
        for (atom, &point) in coordinates.iter().enumerate() {
            let side = dot(normal, sub(point, midpoint));
            if face.contains(&atom) {
                assert!(
                    side.abs() < 0.20,
                    "nonplanar face {face:?}: atom {atom}, offset {side}"
                );
            } else {
                assert!(
                    side < -0.05,
                    "nonconvex cage at face {face:?}: atom {atom}, offset {side}"
                );
            }
        }
    }
    for (index, bond) in request.bonds.iter().enumerate() {
        for other in &request.bonds[index + 1..] {
            if [other.a, other.b].contains(&bond.a) || [other.a, other.b].contains(&bond.b) {
                continue;
            }
            let distance = segment_distance(
                coordinates[bond.a],
                coordinates[bond.b],
                coordinates[other.a],
                coordinates[other.b],
            );
            assert!(
                distance > 0.5,
                "intersecting/close edges {}–{} and {}–{}: {distance}",
                bond.a,
                bond.b,
                other.a,
                other.b
            );
        }
    }
}

fn assert_result(request: &Request, response: &Response, faces: &[Vec<usize>]) {
    assert_eq!(response.field, request.field);
    assert_eq!(response.original_count, 60);
    assert!(response.hydrogen_parents.is_empty());
    assert!(response.initial_energy.is_finite() && response.energy.is_finite());
    assert!(
        response.converged,
        "{:?}: {:?}",
        request.field, response.diagnostics
    );
    assert!(response.energy <= response.initial_energy + 1e-6);
    assert!(response.gradient.is_none());
    assert_cage(request, &response.coordinates, faces);
}

#[test]
fn fixture_is_the_complete_closed_c60_graph_and_independent_spatial_reference() {
    let request = request();
    request.validate().unwrap();
    assert_eq!(request.atoms.len(), 60);
    assert_eq!(request.bonds.len(), 90);
    assert!(request.atoms.iter().all(|atom| {
        atom.atomic_number == 6
            && atom.isotope == 0
            && atom.charge == 0
            && atom.explicit_h == 0
            && atom.no_implicit
            && atom.aromatic
            && atom.radical == 0
            && atom.chiral_tag == 0
    }));
    let mut degrees = [0; 60];
    let mut edges = BTreeMap::new();
    for (index, bond) in request.bonds.iter().enumerate() {
        assert!(
            bond.order == 12 && bond.aromatic && bond.stereo == 0 && bond.stereo_atoms.is_none()
        );
        degrees[bond.a] += 1;
        degrees[bond.b] += 1;
        assert!(edges.insert(edge(bond.a, bond.b), index).is_none());
    }
    assert!(degrees.iter().all(|&degree| degree == 3));
    let reference = reference();
    assert_eq!(reference.faces.len(), 32);
    assert_eq!(
        reference
            .faces
            .iter()
            .filter(|face| face.len() == 5)
            .count(),
        12
    );
    assert_eq!(
        reference
            .faces
            .iter()
            .filter(|face| face.len() == 6)
            .count(),
        20
    );
    assert_eq!(60 + reference.faces.len(), 90 + 2); // Euler's closed-cage relation.
    let mut face_uses = vec![0; 90];
    let mut vertex_uses = [0; 60];
    for face in &reference.faces {
        for (&a, &b) in face.iter().zip(face.iter().cycle().skip(1)) {
            vertex_uses[a] += 1;
            face_uses[edges[&edge(a, b)]] += 1;
        }
    }
    assert!(face_uses.iter().all(|&count| count == 2));
    assert!(vertex_uses.iter().all(|&count| count == 3));
    assert_cage(&request, &reference.coordinates, &reference.faces);
}

#[test]
fn generates_a_converged_convex_c60_cage_with_each_force_field() {
    let reference = reference();
    for field in FIELDS {
        let mut request = request();
        request.field = field;
        let original_request = serde_json::to_value(&request).unwrap();
        let response = solve(&request).unwrap_or_else(|error| panic!("{field:?}: {error}"));
        assert_result(&request, &response, &reference.faces);
        // Input atom/bond ordering is the drawing identity contract.
        assert_eq!(serde_json::to_value(&request).unwrap(), original_request);
    }
}

fn rotated(point: Point) -> Point {
    [-point[1] + 17.0, point[0] - 11.0, point[2] + 4.0]
}

#[test]
fn existing_c60_xyz_is_reused_in_original_order_and_is_rotation_independent() {
    let reference = reference();
    for field in FIELDS {
        let mut request = request();
        request.field = field;
        request.coordinates = reference.coordinates.clone();
        request.validate().unwrap();
        let mut evaluation = request.clone();
        evaluation.operation = Operation::Evaluate;
        let initial = solve(&evaluation).unwrap();
        let generated = solve(&request).unwrap();
        assert_result(&request, &generated, &reference.faces);
        assert!(
            generated
                .diagnostics
                .iter()
                .any(|line| { line.contains("reused existing original-atom 3D coordinates") }),
            "{:?}",
            generated.diagnostics
        );
        let tolerance = 1e-6 + 1e-8 * initial.energy.abs();
        assert!((generated.initial_energy - initial.energy).abs() < tolerance);
        // Rigid-transform invariance compares the same geometry. Separate
        // BFGS paths may terminate at slightly different approximate minima.
        evaluation.coordinates = generated.coordinates.iter().copied().map(rotated).collect();
        let rotated_evaluation = solve(&evaluation).unwrap();
        assert!(
            (rotated_evaluation.energy - generated.energy).abs() < tolerance,
            "{field:?}: rigid Evaluate energy {} vs {}, delta {}, tolerance {tolerance}",
            rotated_evaluation.energy,
            generated.energy,
            (rotated_evaluation.energy - generated.energy).abs()
        );
        request.coordinates = reference.coordinates.iter().copied().map(rotated).collect();
        let moved = solve(&request).unwrap();
        assert_result(&request, &moved, &reference.faces);
        assert!(
            moved
                .diagnostics
                .iter()
                .any(|line| { line.contains("reused existing original-atom 3D coordinates") })
        );
        let displacements: Vec<_> = generated
            .coordinates
            .iter()
            .zip(&moved.coordinates)
            .map(|(&before, &after)| norm(sub(rotated(before), after)))
            .collect();
        let maximum_displacement = displacements.iter().copied().fold(0.0, f64::max);
        assert!(
            (moved.energy - generated.energy).abs() < tolerance,
            "{field:?}: separately minimized energies {} vs {}, delta {}, tolerance {tolerance}; initial energies {} vs {}; maximum atom displacement {maximum_displacement} Å; diagnostics {:?} / {:?}",
            moved.energy,
            generated.energy,
            (moved.energy - generated.energy).abs(),
            moved.initial_energy,
            generated.initial_energy,
            moved.diagnostics,
            generated.diagnostics
        );
        for (atom, displacement) in displacements.into_iter().enumerate() {
            assert!(
                displacement < 0.01,
                "{field:?}: atom {atom} moved {displacement} Å under rigid transform; maximum {maximum_displacement} Å"
            );
        }
    }
}
