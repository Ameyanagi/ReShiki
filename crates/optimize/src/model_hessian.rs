//! Lindh's positive semidefinite model Hessian in Cartesian atomic units.

mod contacts;

pub(crate) use contacts::{BOND_FACTOR, contact_stretches, covalent_radius};

use std::collections::HashMap;

// R. Lindh et al., Chem. Phys. Lett. 241, 423–428 (1995),
// doi:10.1016/0009-2614(95)00646-L. Parameters and the distance-weight
// convention were checked against Table I and Eqs. (37)–(41) in V. Bakken
// and T. Helgaker, J. Chem. Phys. 117, 9160–9174 (2002),
// doi:10.1063/1.1515483 (author copy: trygvehelgaker.no/Publications/
// JChemPhys_117_09160_2002.pdf). No implementation source was used.
pub(crate) const ALPHA: [[f64; 3]; 3] = [
    [1.0, 0.3949, 0.3949],
    [0.3949, 0.28, 0.28],
    [0.3949, 0.28, 0.28],
];
pub(crate) const R_REF: [[f64; 3]; 3] =
    [[1.35, 2.10, 2.53], [2.10, 2.87, 3.40], [2.53, 3.40, 3.40]];
pub(crate) const STRETCH: f64 = 0.45;
pub(crate) const BEND: f64 = 0.15;
pub(crate) const TORSION: f64 = 0.005;

// Numerical pruning choices, rather than fitted parameters. Discard bends and
// torsions within one degree of collinearity, where their coordinates become
// singular. The separate rho and rho-product floors keep both the neighbor
// graph and its local paths small; both apply before multiplying by k.
const MIN_SIN_SQUARED: f64 = 0.000_304_586_490_452_134_93; // sin²(1°)
const MIN_RHO: f64 = 1e-4;
const MIN_RHO_PRODUCT: f64 = 1e-4;
const MIN_DISTANCE_SQUARED: f64 = 1e-16;

type Vector = [f64; 3];

#[derive(Clone, Copy)]
struct Neighbor {
    atom: usize,
    vector: Vector,
    distance: f64,
    rho: f64,
}

/// Construct `Bᵀ diag(k) B` as a flat, row-major Cartesian Hessian.
///
/// Positions are in bohr and the result is in hartree/bohr². Every distinct stretch,
/// bend, and proper torsion with a significant Lindh force constant contributes
/// an analytic Wilson B row. The reference internal coordinates equal those at
/// the supplied geometry, where the internal-coordinate energy gradients vanish.
/// Translations and rotations are null modes.
///
/// The publication fits periods 1–3. Elements in later periods use the period-3
/// parameters as an explicit extrapolation. A pair enters the spatial neighbor
/// list when ρ > 1e-4. Bends and torsions require a product of their constituent
/// pair weights above 1e-4 and angles more than one degree from collinearity.
/// The local path enumeration scales with local coordination. The dense output
/// requires O(N²) space.
pub(crate) fn model_hessian(
    numbers: &[u8],
    positions: &[f64],
    contacts: bool,
) -> Result<Vec<f64>, String> {
    let dimension = numbers
        .len()
        .checked_mul(3)
        .ok_or_else(|| "model Hessian dimension overflow".to_owned())?;
    if positions.len() != dimension {
        return Err("model Hessian requires three coordinates per atom".to_owned());
    }
    if numbers.iter().any(|&number| !(1..=118).contains(&number)) {
        return Err("model Hessian atomic numbers must be in 1..=118".to_owned());
    }
    if positions.iter().any(|value| !value.is_finite()) {
        return Err("model Hessian coordinates must be finite".to_owned());
    }
    let size = dimension
        .checked_mul(dimension)
        .ok_or_else(|| "model Hessian size overflow".to_owned())?;
    let mut hessian = Vec::new();
    hessian
        .try_reserve_exact(size)
        .map_err(|error| format!("cannot allocate model Hessian: {error}"))?;
    hessian.resize(size, 0.0);
    let neighbors = neighbor_list(numbers, positions)?;

    for (j, adjacent) in neighbors.iter().enumerate() {
        for edge in adjacent.iter().filter(|edge| edge.atom > j) {
            let weight = STRETCH * edge.rho;
            let unit = scale(edge.vector, edge.distance.recip());
            add_outer_product(
                &mut hessian,
                dimension,
                weight,
                &[(j, scale(unit, -1.0)), (edge.atom, unit)],
            );
        }

        for (first, left) in adjacent.iter().enumerate() {
            for right in &adjacent[first + 1..] {
                let product = left.rho * right.rho;
                if product <= MIN_RHO_PRODUCT {
                    continue;
                }
                if let Some(derivatives) = bend_derivatives(*left, *right) {
                    add_outer_product(
                        &mut hessian,
                        dimension,
                        BEND * product,
                        &[
                            (left.atom, derivatives[0]),
                            (j, derivatives[1]),
                            (right.atom, derivatives[2]),
                        ],
                    );
                }
            }
        }

        // A central edge j < k owns each chain i-j-k-l exactly once (its
        // reversal is the same internal coordinate). All atoms must differ.
        for middle in adjacent.iter().filter(|edge| edge.atom > j) {
            let k = middle.atom;
            for left in adjacent.iter().filter(|edge| edge.atom != k) {
                for right in neighbors[k]
                    .iter()
                    .filter(|edge| edge.atom != j && edge.atom != left.atom)
                {
                    let product = left.rho * middle.rho * right.rho;
                    if product <= MIN_RHO_PRODUCT {
                        continue;
                    }
                    if let Some(derivatives) = torsion_derivatives(*left, *middle, *right) {
                        add_outer_product(
                            &mut hessian,
                            dimension,
                            TORSION * product,
                            &[
                                (left.atom, derivatives[0]),
                                (j, derivatives[1]),
                                (k, derivatives[2]),
                                (right.atom, derivatives[3]),
                            ],
                        );
                    }
                }
            }
        }
    }
    if contacts {
        for contact in contact_stretches(numbers, positions) {
            add_outer_product(
                &mut hessian,
                dimension,
                contact.force,
                &[
                    (contact.a, scale(contact.unit, -1.)),
                    (contact.b, contact.unit),
                ],
            );
        }
    }
    for row in 0..dimension {
        for column in 0..row {
            hessian[row * dimension + column] = hessian[column * dimension + row];
        }
    }
    if hessian.iter().any(|value| !value.is_finite()) {
        return Err("model Hessian overflow for the supplied geometry".to_owned());
    }
    Ok(hessian)
}

pub(crate) fn period(number: u8) -> usize {
    match number {
        1..=2 => 0,
        3..=10 => 1,
        _ => 2,
    }
}

fn neighbor_list(numbers: &[u8], positions: &[f64]) -> Result<Vec<Vec<Neighbor>>, String> {
    // The largest distance at which any row-pair rho can exceed its floor.
    // A cell width equal to that distance requires only the 27 adjacent cells.
    let mut cutoff_squared: f64 = 0.0;
    for a in 0..3 {
        for b in 0..3 {
            cutoff_squared = cutoff_squared.max(R_REF[a][b].powi(2) - MIN_RHO.ln() / ALPHA[a][b]);
        }
    }
    let cell_width = cutoff_squared.sqrt();
    let mut cells: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    let mut neighbors = vec![Vec::new(); numbers.len()];
    for i in 0..numbers.len() {
        let point = position(positions, i);
        let mut cell = [0_i64; 3];
        for axis in 0..3 {
            let value = (point[axis] / cell_width).floor();
            // Stay within the exactly represented integer range, with room for
            // the neighboring cells. This also avoids saturating float casts.
            if value.abs() > (1_u64 << 52) as f64 {
                return Err("model Hessian coordinates exceed the spatial grid range".to_owned());
            }
            cell[axis] = value as i64;
        }
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let key = [cell[0] + dx, cell[1] + dy, cell[2] + dz];
                    let Some(previous) = cells.get(&key) else {
                        continue;
                    };
                    for &j in previous {
                        let vector = subtract(position(positions, j), point);
                        let distance_squared = dot(vector, vector);
                        if distance_squared < MIN_DISTANCE_SQUARED {
                            return Err(format!("model Hessian atoms {j} and {i} coincide"));
                        }
                        if distance_squared > cutoff_squared {
                            continue;
                        }
                        let a = period(numbers[i]);
                        let b = period(numbers[j]);
                        let rho = (ALPHA[a][b] * (R_REF[a][b].powi(2) - distance_squared)).exp();
                        if rho <= MIN_RHO {
                            continue;
                        }
                        let distance = distance_squared.sqrt();
                        neighbors[i].push(Neighbor {
                            atom: j,
                            vector,
                            distance,
                            rho,
                        });
                        neighbors[j].push(Neighbor {
                            atom: i,
                            vector: scale(vector, -1.0),
                            distance,
                            rho,
                        });
                    }
                }
            }
        }
        cells.entry(cell).or_default().push(i);
    }
    // Deterministic accumulation order, independent of the spatial placement.
    for adjacent in &mut neighbors {
        adjacent.sort_unstable_by_key(|edge| edge.atom);
    }
    Ok(neighbors)
}

fn bend_derivatives(left: Neighbor, right: Neighbor) -> Option<[Vector; 3]> {
    let u = scale(left.vector, left.distance.recip());
    let v = scale(right.vector, right.distance.recip());
    let normal = cross(u, v);
    let sin_squared = dot(normal, normal);
    if sin_squared <= MIN_SIN_SQUARED {
        return None;
    }
    let normal = scale(normal, sin_squared.sqrt().recip());
    let first = scale(cross(u, normal), left.distance.recip());
    let last = scale(cross(normal, v), right.distance.recip());
    Some([first, scale(add(first, last), -1.0), last])
}

fn torsion_derivatives(left: Neighbor, middle: Neighbor, right: Neighbor) -> Option<[Vector; 4]> {
    let u = scale(left.vector, left.distance.recip());
    let v = scale(right.vector, right.distance.recip());
    let w = scale(middle.vector, middle.distance.recip());
    let uw = cross(u, w);
    let vw = cross(v, w);
    let sin_u_squared = dot(uw, uw);
    let sin_v_squared = dot(vw, vw);
    if sin_u_squared <= MIN_SIN_SQUARED || sin_v_squared <= MIN_SIN_SQUARED {
        return None;
    }
    // Differentiate the signed angle between the two outer bond planes.
    // Endpoint derivatives set the plane rotations; the two central derivatives
    // also contain the change in the direction of the central bond.
    let first = scale(uw, (left.distance * sin_u_squared).recip());
    let last = scale(vw, -(right.distance * sin_v_squared).recip());
    let axis_change = subtract(
        scale(uw, dot(u, w) / (middle.distance * sin_u_squared)),
        scale(vw, dot(v, w) / (middle.distance * sin_v_squared)),
    );
    let second = subtract(axis_change, first);
    let third = scale(add(axis_change, last), -1.0);
    Some([first, second, third, last])
}

fn add_outer_product<const N: usize>(
    hessian: &mut [f64],
    dimension: usize,
    weight: f64,
    row: &[(usize, Vector); N],
) {
    for &(atom_a, a) in row {
        for &(atom_b, b) in row {
            for (axis_a, &derivative_a) in a.iter().enumerate() {
                let i = 3 * atom_a + axis_a;
                for (axis_b, &derivative_b) in b.iter().enumerate() {
                    let j = 3 * atom_b + axis_b;
                    if i <= j {
                        hessian[i * dimension + j] += weight * (derivative_a * derivative_b);
                    }
                }
            }
        }
    }
}

fn position(positions: &[f64], atom: usize) -> Vector {
    [
        positions[3 * atom],
        positions[3 * atom + 1],
        positions[3 * atom + 2],
    ]
}

fn add(a: Vector, b: Vector) -> Vector {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn subtract(a: Vector, b: Vector) -> Vector {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: Vector, factor: f64) -> Vector {
    [a[0] * factor, a[1] * factor, a[2] * factor]
}

fn dot(a: Vector, b: Vector) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vector, b: Vector) -> Vector {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hydrogen_matches_hand_calculation() {
        let hessian = model_hessian(&[1, 1], &[0.0, 0.0, 0.0, 1.4, 0.0, 0.0], false).unwrap();
        // 0.45 exp(1.0 * (1.35² - 1.40²)) = 0.3921904574987212.
        let expected = 0.392_190_457_498_721_2;
        for row in 0..6 {
            for column in 0..6 {
                let value = match (row, column) {
                    (0, 0) | (3, 3) => expected,
                    (0, 3) | (3, 0) => -expected,
                    _ => 0.0,
                };
                assert!((hessian[6 * row + column] - value).abs() < 1e-14);
            }
        }
        let (eigenvalues, _) = crate::linalg::symmetric_eigen(hessian, 6).unwrap();
        assert!((eigenvalues[5] - 2.0 * expected).abs() < 1e-13);
        assert!(eigenvalues[..5].iter().all(|value| value.abs() < 1e-13));
    }

    fn water() -> (Vec<u8>, Vec<f64>) {
        let angle = 104.5_f64.to_radians();
        (
            vec![8, 1, 1],
            vec![
                0.0,
                0.0,
                0.0,
                1.81,
                0.0,
                0.0,
                1.81 * angle.cos(),
                1.81 * angle.sin(),
                0.0,
            ],
        )
    }

    fn ethane() -> (Vec<u8>, Vec<f64>) {
        let mut positions = vec![-1.45, 0.0, 0.0, 1.45, 0.0, 0.0];
        let radius = 2.06 * (8.0_f64 / 9.0).sqrt();
        for side in [-1.0, 1.0] {
            for k in 0..3 {
                let angle = (120.0 * k as f64 + if side > 0.0 { 60.0 } else { 0.0 }).to_radians();
                positions.extend_from_slice(&[
                    side * (1.45 + 2.06 / 3.0),
                    radius * angle.cos(),
                    radius * angle.sin(),
                ]);
            }
        }
        (vec![6, 6, 1, 1, 1, 1, 1, 1], positions)
    }

    #[test]
    fn water_and_ethane_are_symmetric_psd_with_rigid_null_modes() {
        for (numbers, positions) in [water(), ethane()] {
            let hessian = model_hessian(&numbers, &positions, false).unwrap();
            let dimension = positions.len();
            for i in 0..dimension {
                for j in 0..dimension {
                    assert_eq!(hessian[i * dimension + j], hessian[j * dimension + i]);
                }
            }
            let (eigenvalues, _) =
                crate::linalg::symmetric_eigen(hessian.clone(), dimension).unwrap();
            assert!(
                eigenvalues.iter().all(|&value| value >= -1e-10),
                "{eigenvalues:?}"
            );
            assert_eq!(
                eigenvalues
                    .iter()
                    .filter(|&&value| value.abs() < 1e-9)
                    .count(),
                6
            );
            for axis in 0..3 {
                let mut translation = vec![0.0; dimension];
                let mut rotation = vec![0.0; dimension];
                let mut direction = [0.0; 3];
                direction[axis] = 1.0;
                for atom in 0..numbers.len() {
                    translation[3 * atom + axis] = 1.0;
                    rotation[3 * atom..3 * atom + 3]
                        .copy_from_slice(&cross(direction, position(&positions, atom)));
                }
                for displacement in [translation, rotation] {
                    for row in hessian.chunks_exact(dimension) {
                        let product: f64 = row.iter().zip(&displacement).map(|(a, b)| a * b).sum();
                        assert!(product.abs() < 1e-10, "rigid mode residual {product}");
                    }
                }
            }
        }
    }

    #[test]
    fn rigid_transforms_rotate_the_hessian() {
        // A cyclic permutation is a proper rotation and exercises all axes.
        for (numbers, positions) in [water(), ethane()] {
            let hessian = model_hessian(&numbers, &positions, false).unwrap();
            let transformed: Vec<f64> = positions
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|p| [p[1] + 15.0, p[2] - 8.0, p[0] + 4.0])
                .collect();
            let rotated = model_hessian(&numbers, &transformed, false).unwrap();
            let dimension = positions.len();
            for i in 0..dimension {
                let original_i = i / 3 * 3 + (i % 3 + 1) % 3;
                for j in 0..dimension {
                    let original_j = j / 3 * 3 + (j % 3 + 1) % 3;
                    assert!(
                        (rotated[i * dimension + j] - hessian[original_i * dimension + original_j])
                            .abs()
                            < 2e-12
                    );
                }
            }
        }
    }

    fn edge(vector: Vector) -> Neighbor {
        Neighbor {
            atom: 0,
            vector,
            distance: dot(vector, vector).sqrt(),
            rho: 1.0,
        }
    }

    fn signed_torsion(points: &[Vector; 4]) -> f64 {
        let u = subtract(points[0], points[1]);
        let v = subtract(points[3], points[2]);
        let axis = subtract(points[2], points[1]);
        let w = scale(axis, dot(axis, axis).sqrt().recip());
        let a = cross(u, w);
        let b = cross(v, w);
        dot(w, cross(a, b)).atan2(dot(a, b))
    }

    #[test]
    fn analytic_bend_and_torsion_rows_match_coordinate_derivatives() {
        let points = [
            [0.3, 1.1, -0.4],
            [-0.2, 0.1, 0.3],
            [1.4, -0.3, 0.5],
            [2.0, 1.0, 1.3],
        ];
        let bend = bend_derivatives(
            edge(subtract(points[0], points[1])),
            edge(subtract(points[2], points[1])),
        )
        .unwrap();
        let torsion = torsion_derivatives(
            edge(subtract(points[0], points[1])),
            edge(subtract(points[2], points[1])),
            edge(subtract(points[3], points[2])),
        )
        .unwrap();
        let epsilon = 1e-6;
        for atom in 0..4 {
            for axis in 0..3 {
                let mut plus = points;
                let mut minus = points;
                plus[atom][axis] += epsilon;
                minus[atom][axis] -= epsilon;
                let derivative = (signed_torsion(&plus) - signed_torsion(&minus)) / (2.0 * epsilon);
                assert!(
                    (torsion[atom][axis] - derivative).abs() < 2e-9,
                    "torsion {atom} {axis}"
                );
                if atom < 3 {
                    let angle = |p: &[Vector; 4]| {
                        let u = subtract(p[0], p[1]);
                        let v = subtract(p[2], p[1]);
                        (dot(u, v) / (dot(u, u) * dot(v, v)).sqrt()).acos()
                    };
                    let derivative = (angle(&plus) - angle(&minus)) / (2.0 * epsilon);
                    assert!(
                        (bend[atom][axis] - derivative).abs() < 2e-9,
                        "bend {atom} {axis}"
                    );
                }
            }
        }
    }

    #[test]
    fn linear_and_sparse_geometries_are_finite() {
        let linear = model_hessian(
            &[1, 6, 6, 1],
            &[-4.0, 0.0, 0.0, -2.0, 0.0, 0.0, 2.0, 0.0, 0.0, 4.0, 0.0, 0.0],
            false,
        )
        .unwrap();
        assert!(linear.iter().all(|value| value.is_finite()));
        let numbers = vec![6; 1_000];
        let positions: Vec<f64> = (0..numbers.len())
            .flat_map(|atom| [30.0 * atom as f64, 0.0, 0.0])
            .collect();
        let neighbors = neighbor_list(&numbers, &positions).unwrap();
        assert!(neighbors.iter().all(Vec::is_empty));
    }

    #[test]
    fn skips_bends_and_torsions_within_one_degree_of_linear() {
        for degrees in [0.9_f64, 1.1, 178.9, 179.1] {
            let angle = degrees.to_radians();
            let left = edge([angle.cos(), angle.sin(), 0.0]);
            let middle = edge([1.0, 0.0, 0.0]);
            let right = edge([0.0, 1.0, 1.0]);
            let retain = (1.0..179.0).contains(&degrees);
            assert_eq!(bend_derivatives(left, middle).is_some(), retain);
            assert_eq!(torsion_derivatives(left, middle, right).is_some(), retain);
        }
    }

    #[test]
    fn spatial_graph_obeys_the_pair_weight_floor() {
        let cutoff = (R_REF[0][0].powi(2) - MIN_RHO.ln() / ALPHA[0][0]).sqrt();
        for (distance, retain) in [(cutoff - 1e-6, true), (cutoff + 1e-6, false)] {
            let graph = neighbor_list(&[1, 1], &[0.0, 0.0, 0.0, distance, 0.0, 0.0]).unwrap();
            assert_eq!(!graph[0].is_empty(), retain);
            assert_eq!(!graph[1].is_empty(), retain);
        }
    }

    #[test]
    fn validates_inputs_and_handles_single_atoms() {
        assert_eq!(model_hessian(&[], &[], false).unwrap(), Vec::<f64>::new());
        assert_eq!(
            model_hessian(&[18], &[1.0, 2.0, 3.0], false).unwrap(),
            vec![0.0; 9]
        );
        assert!(model_hessian(&[0], &[0.0; 3], false).is_err());
        assert!(model_hessian(&[119], &[0.0; 3], false).is_err());
        assert!(model_hessian(&[1], &[0.0; 2], false).is_err());
        assert!(model_hessian(&[1], &[f64::NAN, 0.0, 0.0], false).is_err());
        assert!(model_hessian(&[1, 1], &[0.0; 6], false).is_err());
    }
    #[test]
    fn optional_contact_is_the_radial_rank_one_curvature_with_rigid_null_modes() {
        let r = 2. * 1.88 / crate::ANGSTROM_PER_BOHR;
        let x = [0., 0., 0., r, 0., 0.];
        let base = model_hessian(&[18, 18], &x, false).unwrap();
        let augmented = model_hessian(&[18, 18], &x, true).unwrap();
        assert!(base.iter().all(|v| *v == 0.));
        for i in 0..6 {
            for j in 0..6 {
                let expected = match (i, j) {
                    (0, 0) | (3, 3) => 0.001,
                    (0, 3) | (3, 0) => -0.001,
                    _ => 0.,
                };
                assert!((augmented[i * 6 + j] - expected).abs() < 1e-15);
            }
        }
        let (eigenvalues, _) = crate::linalg::symmetric_eigen(augmented, 6).unwrap();
        assert!(eigenvalues[..5].iter().all(|v| v.abs() < 1e-15));
        assert!((eigenvalues[5] - 0.002).abs() < 1e-15);
    }
}
