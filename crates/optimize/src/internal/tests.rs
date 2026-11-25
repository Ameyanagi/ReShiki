//! Independent checks of the coordinate definitions and published identities.
//!
//! Central differences and Cartesian quadratic forms check the Wilson matrix
//! and the transformations of Bakken & Helgaker, J. Chem. Phys. 117, 9160
//! (2002), and Wang & Song, J. Chem. Phys. 144, 214108 (2016).
//! The BFGS secant condition follows Nocedal & Wright, *Numerical
//! Optimization*, 2nd ed., Chapter 6.

use super::{InternalModel, fragment::Fragment, primitive::Local};
use crate::linalg::{dot, norm, symmetric_eigen};
use crate::optimizer::Update;
use std::f64::consts::{PI, TAU};

type Molecule = (Vec<u8>, Vec<f64>);
type Vector = [f64; 3];

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.16e}, expected {expected:.16e}, tolerance {tolerance:.3e}"
    );
}

fn maximum_error(actual: &[f64], expected: &[f64]) -> f64 {
    assert_eq!(actual.len(), expected.len());
    actual
        .iter()
        .zip(expected)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max)
}

fn free_components(x: &[f64], fixed: &[usize]) -> Vec<usize> {
    (0..x.len()).filter(|i| !fixed.contains(&(i / 3))).collect()
}

fn water() -> Molecule {
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

fn ammonia() -> Molecule {
    let mut x = vec![0.0, 0.0, 0.0];
    for atom in 0..3 {
        let angle = TAU * atom as f64 / 3.0;
        x.extend([1.77 * angle.cos(), 1.77 * angle.sin(), -0.68]);
    }
    (vec![7, 1, 1, 1], x)
}

fn benzene() -> Molecule {
    let mut x = Vec::new();
    for radius in [2.64, 4.70] {
        for atom in 0..6 {
            let angle = TAU * atom as f64 / 6.0;
            x.extend([radius * angle.cos(), radius * angle.sin(), 0.0]);
        }
    }
    let mut z = vec![6; 6];
    z.extend([1; 6]);
    (z, x)
}

fn carbon_dioxide() -> Molecule {
    (
        vec![8, 6, 8],
        vec![-2.20, 0.0, 0.0, 0.0, 0.0, 0.0, 2.20, 0.0, 0.0],
    )
}

fn acetylene() -> Molecule {
    (
        vec![1, 6, 6, 1],
        vec![
            -3.15, 0.0, 0.0, -1.14, 0.0, 0.0, 1.14, 0.0, 0.0, 3.15, 0.0, 0.0,
        ],
    )
}

fn water_dimer() -> Molecule {
    let (mut z, mut x) = water();
    z.extend([8, 1, 1]);
    // One O--H bond points toward the other oxygen. The second monomer
    // is tilted to exercise every component of its rotational derivative.
    x.extend([5.50, 0.60, 0.30, 6.96, -0.43, 0.56, 5.92, 2.25, -0.32]);
    (z, x)
}

fn add(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|k| a[k] + b[k])
}

fn sub(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|k| a[k] - b[k])
}

fn scale(a: Vector, factor: f64) -> Vector {
    a.map(|v| factor * v)
}

fn cross(a: Vector, b: Vector) -> Vector {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: Vector) -> Vector {
    scale(a, norm(&a).recip())
}

fn point(x: &[f64], atom: usize) -> Vector {
    x[3 * atom..3 * atom + 3].try_into().unwrap()
}

// Complete a carbon's tetrahedral valence from one or two heavy-atom bonds.
fn carbon_hydrogens(x: &mut Vec<f64>, carbon: usize, neighbors: &[usize]) {
    let origin = point(x, carbon);
    let a = unit(sub(point(x, neighbors[0]), origin));
    let directions = if neighbors.len() == 1 {
        let helper = if a[2].abs() < 0.8 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let u = unit(cross(a, helper));
        let v = cross(a, u);
        (0..3)
            .map(|i| {
                let angle = TAU * i as f64 / 3.0;
                add(
                    scale(a, -1.0 / 3.0),
                    scale(
                        add(scale(u, angle.cos()), scale(v, angle.sin())),
                        (8.0_f64 / 9.0).sqrt(),
                    ),
                )
            })
            .collect::<Vec<_>>()
    } else {
        let b = unit(sub(point(x, neighbors[1]), origin));
        let projection = scale(add(a, b), -1.0 / (3.0 * (1.0 + dot(&a, &b))));
        let normal = scale(
            unit(cross(a, b)),
            (1.0 - dot(&projection, &projection)).sqrt(),
        );
        vec![add(projection, normal), sub(projection, normal)]
    };
    for direction in directions {
        x.extend(add(origin, scale(direction, 2.06)));
    }
}

fn butane() -> Molecule {
    let mut x = vec![
        -1.40, 2.45, 0.0, 0.0, 0.0, 0.0, 2.86, 0.0, 0.0, 4.26, 1.95, 1.48,
    ];
    for (atom, adjacent) in [
        (0, &[1][..]),
        (1, &[0, 2][..]),
        (2, &[1, 3][..]),
        (3, &[2][..]),
    ] {
        carbon_hydrogens(&mut x, atom, adjacent);
    }
    let mut z = vec![6; 4];
    z.extend([1; 10]);
    (z, x)
}

fn butyne() -> Molecule {
    // The central four carbons are exactly collinear. The relative methyl
    // rotation needs a torsion spanning that entire linear run.
    let mut x = vec![
        -4.04, 0.0, 0.0, -1.14, 0.0, 0.0, 1.14, 0.0, 0.0, 4.04, 0.0, 0.0,
    ];
    carbon_hydrogens(&mut x, 0, &[1]);
    carbon_hydrogens(&mut x, 3, &[2]);
    let mut z = vec![6; 4];
    z.extend([1; 6]);
    (z, x)
}

fn ethanol() -> Molecule {
    let mut x = vec![0.0, 0.0, 0.0, 2.86, 0.0, 0.0, 4.01, 2.39, 0.20];
    carbon_hydrogens(&mut x, 0, &[1]);
    carbon_hydrogens(&mut x, 1, &[0, 2]);
    x.extend([5.60, 2.01, -0.56]);
    (vec![6, 6, 8, 1, 1, 1, 1, 1, 1], x)
}

fn random(state: &mut u64) -> f64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    (*state >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0
}

fn random_vector(n: usize, length: f64, state: &mut u64) -> Vec<f64> {
    let mut vector: Vec<_> = (0..n).map(|_| random(state)).collect();
    let factor = length / norm(&vector);
    vector.iter_mut().for_each(|v| *v *= factor);
    vector
}

fn rank(rows: &[f64], n: usize) -> usize {
    let mut gram = vec![0.0; n * n];
    for row in rows.chunks_exact(n) {
        for i in 0..n {
            for j in 0..n {
                gram[i * n + j] += row[i] * row[j];
            }
        }
    }
    let (values, _) = symmetric_eigen(gram, n).unwrap();
    let cutoff = values[n - 1] * 1e-10;
    values.iter().filter(|&&value| value > cutoff).count()
}

fn delocalized_rows(model: &InternalModel, x: &[f64], free: &[usize]) -> Vec<f64> {
    let primitive = model.chart.rows(x, free).unwrap();
    let nprimitive = primitive.len() / free.len();
    let mut rows = vec![0.0; model.dimension() * free.len()];
    assert_eq!(model.basis.len(), model.dimension() * nprimitive);
    for (row, coefficients) in rows
        .chunks_exact_mut(free.len())
        .zip(model.basis.chunks_exact(nprimitive))
    {
        for (&coefficient, primitive_row) in
            coefficients.iter().zip(primitive.chunks_exact(free.len()))
        {
            for (value, &derivative) in row.iter_mut().zip(primitive_row) {
                *value += coefficient * derivative;
            }
        }
    }
    rows
}

fn cartesian_gradient(
    model: &InternalModel,
    x: &[f64],
    free: &[usize],
    internal: &[f64],
) -> Vec<f64> {
    let mut cartesian = vec![0.0; x.len()];
    for (row, &gradient) in delocalized_rows(model, x, free)
        .chunks_exact(free.len())
        .zip(internal)
    {
        for (&index, &derivative) in free.iter().zip(row) {
            cartesian[index] += derivative * gradient;
        }
    }
    cartesian
}

fn physical_hessian(model: &InternalModel, x: &[f64], free: &[usize]) -> Vec<f64> {
    let b = delocalized_rows(model, x, free);
    let dim = model.dimension();
    let n = free.len();
    let mut cartesian = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            for p in 0..dim {
                for q in 0..dim {
                    cartesian[i * n + j] +=
                        b[p * n + i] * model.hessian[p * dim + q] * b[q * n + j];
                }
            }
        }
    }
    cartesian
}

fn check_rows(model: &InternalModel, x: &[f64], free: &[usize]) {
    let analytic = model.chart.rows(x, free).unwrap();
    let count = model.chart.raw_values(x).unwrap().len();
    assert_eq!(analytic.len(), count * free.len());
    let epsilon = 1e-5;
    for (column, &index) in free.iter().enumerate() {
        let mut plus = x.to_vec();
        let mut minus = x.to_vec();
        plus[index] += epsilon;
        minus[index] -= epsilon;
        let plus_values = model.chart.raw_values(&plus).unwrap();
        let minus_values = model.chart.raw_values(&minus).unwrap();
        let difference = model.chart.differences(&plus_values, &minus_values);
        assert_eq!(difference.len(), count);
        for row in 0..count {
            let numerical = difference[row] / (2.0 * epsilon);
            let exact = analytic[row * free.len() + column];
            assert!(
                (exact - numerical).abs() <= 1e-7,
                "B row {row}, free column {column} (Cartesian {index}): analytic {exact:.16e}, central difference {numerical:.16e}"
            );
        }
    }
}

#[test]
fn every_chart_row_matches_central_differences_at_reference_and_rotated_geometries() {
    let mut seen = [false; 5];
    for (numbers, x) in [
        water(),
        ammonia(),
        benzene(),
        carbon_dioxide(),
        acetylene(),
        butane(),
        butyne(),
        water_dimer(),
    ] {
        for fixed in [&[][..], &[0][..]] {
            let free = free_components(&x, fixed);
            let model = InternalModel::build(&numbers, &free, &x, true, None).unwrap();
            for (local, _) in &model.chart.topology.locals {
                let kind = match local {
                    Local::Stretch(..) => 0,
                    Local::Bend(..) => 1,
                    Local::LinearBend(..) => 2,
                    Local::Torsion(..) => 3,
                    Local::OutOfPlane(..) => 4,
                };
                seen[kind] = true;
            }
            check_rows(&model, &x, &free);
            let rotated: Vec<f64> = x
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|p| {
                    let (s, c) = 0.31_f64.sin_cos();
                    [
                        c * p[0] - s * p[1] + 0.17,
                        s * p[0] + c * p[1] - 0.23,
                        p[2] + 0.11,
                    ]
                })
                .collect();
            check_rows(&model, &rotated, &free);
        }
    }
    assert!(seen.into_iter().all(|present| present));
}

#[test]
fn optional_hydrogen_bond_is_local_and_preserves_two_rigid_fragments() {
    let (numbers, x) = water_dimer();
    let free = free_components(&x, &[]);
    let without = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
    let with = InternalModel::build(&numbers, &free, &x, true, None).unwrap();
    let hydrogen_bond =
        |local: &Local| matches!(local, Local::Stretch(1, 3) | Local::Stretch(3, 1));
    assert!(
        !without
            .chart
            .topology
            .locals
            .iter()
            .any(|(p, _)| hydrogen_bond(p))
    );
    assert!(
        with.chart
            .topology
            .locals
            .iter()
            .any(|(p, _)| hydrogen_bond(p))
    );
    assert_eq!(without.chart.fragments.len(), 2);
    assert_eq!(with.chart.fragments.len(), 2);
    check_rows(&with, &x, &free);
}

#[test]
fn torsions_span_a_collinear_run_between_two_methyl_groups() {
    let (numbers, x) = butyne();
    let free = free_components(&x, &[]);
    let model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
    assert!(
        model
            .chart
            .topology
            .locals
            .iter()
            .any(|(p, _)| { matches!(p, Local::Torsion(_, 0, 3, _) | Local::Torsion(_, 3, 0, _)) })
    );
    let rows = model.chart.rows(&x, &free).unwrap();
    assert_eq!(
        rank(
            &rows[..model.chart.topology.locals.len() * free.len()],
            free.len()
        ),
        free.len() - 6
    );
    assert_eq!(model.dimension(), free.len());
    check_rows(&model, &x, &free);
}

#[test]
fn rigid_rotation_has_known_sign_angle_and_gyration_scale() {
    let (numbers, x) = ammonia();
    let free = free_components(&x, &[]);
    let model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
    let center: Vector = std::array::from_fn(|k| {
        x.as_chunks::<3>().0.iter().map(|p| p[k]).sum::<f64>() / numbers.len() as f64
    });
    let radius = (x
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| (0..3).map(|k| (p[k] - center[k]).powi(2)).sum::<f64>())
        .sum::<f64>()
        / numbers.len() as f64)
        .sqrt();
    let axis = unit([0.3, -0.4, 0.5]);
    let rotate = |angle: f64| -> Vec<f64> {
        let (sine, cosine) = angle.sin_cos();
        x.as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&p| {
                let product = cross(axis, p);
                std::array::from_fn::<_, 3, _>(|k| {
                    cosine * p[k]
                        + sine * product[k]
                        + (1.0 - cosine) * dot(&axis, &p) * axis[k]
                        + [1.3, -2.1, 0.7][k]
                })
            })
            .collect()
    };
    assert!(!model.needs_rebuild(&x));
    for angle in [-0.7, 0.7] {
        let rotated = rotate(angle);
        let values = model.chart.values(&rotated).unwrap();
        let offset = model.chart.topology.locals.len();
        for k in 0..3 {
            let expected_center =
                rotated.as_chunks::<3>().0.iter().map(|p| p[k]).sum::<f64>() / numbers.len() as f64;
            close(values[offset + k], expected_center, 1e-12);
            close(values[offset + 3 + k], radius * angle * axis[k], 1e-12);
        }
        assert!(!model.needs_rebuild(&rotated));
        check_rows(&model, &rotated, &free);
    }
    assert!(model.needs_rebuild(&rotate(0.95 * PI)));
}

#[test]
fn rotation_branch_cut_rejects_stale_chart_and_rebuilds_short_secant() {
    let numbers = [1, 1];
    let x = [-0.7, 0.0, 0.0, 0.7, 0.0, 0.0];
    let free = free_components(&x, &[]);
    let mut model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
    let rotate = |degrees: f64| -> Vec<f64> {
        let (sine, cosine) = degrees.to_radians().sin_cos();
        x.as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [cosine * p[0], sine * p[0], p[2]])
            .collect()
    };
    let old = rotate(160.0);
    let new = rotate(200.0);
    // Recenter before the hard validity boundary so ordinary accepted motion
    // cannot creep against it. Values remain valid at this earlier trigger.
    assert!(model.needs_rebuild(&rotate(82.0)));
    assert!(model.chart.values(&rotate(82.0)).is_some());
    let stale = model.displacement(&old, &new);
    assert!(
        stale.is_none(),
        "stale rotation chart produced a {:.12}-bohr displacement for a {:.12}-bohr rotation",
        norm(&stale.unwrap()),
        0.7 * 40.0_f64.to_radians()
    );
    assert!(model.needs_rebuild(&old));
    assert!(
        model
            .model_direction(
                &old,
                vec![1.0; model.dimension()],
                vec![-1.0; model.dimension()]
            )
            .is_none()
    );
    assert!(
        model
            .back_transform(&old, &vec![0.0; model.dimension()])
            .is_none()
    );
    let before = model.hessian.clone();
    model.update(&old, &new, &[0.0; 6], &new, Update::Bfgs);
    assert_eq!(model.hessian, before);
    assert_eq!(model.linearized_at, x);
    model.rebuild(&old).unwrap();
    let short = model.displacement(&old, &new).unwrap();
    close(norm(&short), 0.7 * 40.0_f64.to_radians(), 1e-12);
    let transformed = model.back_transform(&old, &short).unwrap();
    assert!(transformed.converged);
    assert!(maximum_error(&transformed.positions, &new) < 1e-9);
}

#[test]
fn nonparallel_rotation_endpoints_require_a_common_chart_for_rows_and_secants() {
    let (numbers, x) = ammonia();
    let free = free_components(&x, &[]);
    let mut model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
    let rotate = |degrees: f64, axis: Vector| -> Vec<f64> {
        let axis = unit(axis);
        let (sine, cosine) = degrees.to_radians().sin_cos();
        x.as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&p| {
                let product = cross(axis, p);
                std::array::from_fn::<_, 3, _>(|k| {
                    cosine * p[k] + sine * product[k] + (1.0 - cosine) * dot(&axis, &p) * axis[k]
                })
            })
            .collect()
    };
    let old = rotate(80.0, [0.0, 0.0, 1.0]);
    let new = rotate(100.0, [0.2, 0.1, 1.0]);
    assert!(!model.needs_rebuild(&old));
    assert!(model.needs_rebuild(&new));
    assert!(model.displacement(&old, &new).is_none());
    assert!(model.chart.values(&new).is_none());
    let before = model.hessian.clone();
    model.update(&old, &new, &old, &new, Update::Bfgs);
    assert_eq!(model.hessian, before);
    assert_eq!(model.linearized_at, x);

    // Even a Newton solve starting inside the chart cannot return an iterate
    // whose residual uses a different branch than its endpoint Wilson rows.
    let target = super::metric::apply(
        &model.basis,
        &model.chart.differences(
            &model.chart.raw_values(&new).unwrap(),
            &model.chart.values(&old).unwrap(),
        ),
    );
    if let Some(fallback) = model.back_transform(&old, &target) {
        assert!(!fallback.converged);
        assert!(model.displacement(&old, &fallback.positions).is_some());
    }

    model.rebuild(&old).unwrap();
    check_rows(&model, &new, &free);
    let displacement = model.displacement(&old, &new).unwrap();
    let y: Vec<_> = displacement.iter().map(|s| 3.0 * s).collect();
    let new_gradient = cartesian_gradient(&model, &new, &free, &y);
    model.update(&old, &new, &vec![0.0; x.len()], &new_gradient, Update::Bfgs);
    let hs = super::metric::apply(&model.hessian, &displacement);
    assert!(maximum_error(&hs, &y) < 1e-9);
    let transformed = model.back_transform(&old, &displacement).unwrap();
    assert!(transformed.converged);
    assert!(maximum_error(&transformed.positions, &new) < 1e-9);
}

#[test]
fn torsion_rows_remain_continuous_across_both_sides_of_pi() {
    // A trans four-carbon chain has a proper torsion on the branch cut.
    // The finite-difference stencil crosses that cut, so ordinary subtraction
    // of values would produce a spurious derivative of approximately pi / h.
    for height in [-1e-7, 0.0, 1e-7] {
        let x = vec![
            -1.40, 2.45, 0.0, 0.0, 0.0, 0.0, 2.86, 0.0, 0.0, 4.26, -2.45, height,
        ];
        let free = free_components(&x, &[]);
        let model = InternalModel::build(&[6; 4], &free, &x, false, None).unwrap();
        let values = model.chart.values(&x).unwrap();
        assert!(
            values[..model.chart.topology.locals.len()]
                .iter()
                .any(|v| (v.abs() - PI).abs() < 1e-6)
        );
        check_rows(&model, &x, &free);
    }
    let torsion = Local::Torsion(0, 1, 2, 3);
    assert_eq!(torsion.difference(-PI, 0.0), PI);
    assert_eq!(torsion.difference(PI, 0.0), PI);
    close(torsion.difference(-PI + 0.01, PI - 0.01), 0.02, 1e-15);
    close(torsion.difference(PI - 0.01, -PI + 0.01), -0.02, 1e-15);
}

#[test]
fn distorted_horn_and_nearly_linear_rotation_rows_match_central_differences() {
    let (numbers, x) = ammonia();
    let free = free_components(&x, &[]);
    let model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
    let mut distorted: Vec<f64> = x
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|p| [p[2] + 5.0, p[0] - 3.0, p[1] + 2.0])
        .collect();
    distorted[3] += 0.13;
    distorted[7] -= 0.08;
    distorted[11] += 0.17;
    check_rows(&model, &distorted, &free);
    for bending in [1e-6, 1e-4] {
        let (numbers, mut x) = carbon_dioxide();
        x[4] = bending;
        let free = free_components(&x, &[]);
        let model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
        check_rows(&model, &x, &free);
        let tilted: Vec<f64> = x
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| {
                let (s, c) = 0.61_f64.sin_cos();
                [c * p[0] + s * p[2], p[1] + 0.9, -s * p[0] + c * p[2]]
            })
            .collect();
        check_rows(&model, &tilted, &free);
    }
}

#[test]
fn singleton_fragments_are_exact_cartesian_translations() {
    let numbers = [2, 10, 18];
    let x = [0.0, 0.0, 0.0, 15.0, 2.0, 3.0, -12.0, 6.0, 8.0];
    for fixed in [&[][..], &[1][..]] {
        let free = free_components(&x, fixed);
        let model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
        assert!(model.chart.topology.locals.is_empty());
        assert_eq!(model.chart.fragments.len(), 3);
        assert_eq!(model.chart.values(&x).unwrap(), x);
        let rows = model.chart.rows(&x, &free).unwrap();
        for (row, derivatives) in rows.chunks_exact(free.len()).enumerate() {
            for (&column, &derivative) in free.iter().zip(derivatives) {
                assert_eq!(derivative, f64::from(row == column));
            }
        }
        let step: Vec<_> = (0..model.dimension())
            .map(|i| 1e-3 * (i as f64 - 2.0))
            .collect();
        let transformed = model.back_transform(&x, &step).unwrap();
        assert!(transformed.converged);
        let realized = model.displacement(&x, &transformed.positions).unwrap();
        assert!(maximum_error(&realized, &step) < 1e-12);
        for &atom in fixed {
            assert_eq!(
                &transformed.positions[3 * atom..3 * atom + 3],
                &x[3 * atom..3 * atom + 3]
            );
        }
        check_rows(&model, &x, &free);
    }
}

#[test]
fn rebuild_detection_tracks_linearity_and_is_invariant_to_translation() {
    let (numbers, x) = carbon_dioxide();
    let free = free_components(&x, &[]);
    let mut model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
    let translated: Vec<_> = x
        .iter()
        .enumerate()
        .map(|(i, v)| v + [31.0, -47.0, 23.0][i % 3])
        .collect();
    assert!(!model.needs_rebuild(&translated));
    for (degrees, expected) in [(50_f64, false), (70_f64, true)] {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let rotated: Vec<_> = x
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[a, b, c]| [cos * a - sin * b, sin * a + cos * b, c])
            .collect();
        assert_eq!(model.needs_rebuild(&rotated), expected);
    }
    let mut bent = x.clone();
    bent[4] = 0.20;
    assert!(model.needs_rebuild(&bent));
    model.rebuild(&bent).unwrap();
    assert!(!model.needs_rebuild(&bent));
    assert!(model.needs_rebuild(&x));
}

#[test]
fn bond_cutoffs_and_lindh_stiffness_have_atomic_units() {
    // Independent samples from the single-bond radius table (pm).
    for (z, pm) in [(1, 32.), (8, 63.), (26, 116.), (118, 157.)] {
        let diameter = 2. * pm * 0.01 / crate::ANGSTROM_PER_BOHR;
        for (factor, fragments) in [(1.29, 1), (1.31, 2)] {
            let x = [0., 0., 0., diameter * factor, 0., 0.];
            let graph = super::topology::Topology::generate(&[z, z], &x, false).unwrap();
            assert_eq!(graph.fragments.len(), fragments);
        }
    }
    let (numbers, x) = water();
    let graph = super::topology::Topology::generate(&numbers, &x, false).unwrap();
    let rho = |a, b| {
        let bond = sub(point(&x, a), point(&x, b));
        (0.3949 * (2.10_f64.powi(2) - dot(&bond, &bond))).exp()
    };
    for (p, k) in graph.locals {
        let expected = match p {
            Local::Stretch(a, b) => 0.45 * rho(a, b),
            Local::Bend(a, b, c) => 0.15 * rho(a, b) * rho(b, c),
            _ => panic!("unexpected water primitive {p:?}"),
        };
        close(k, expected, 1e-14);
    }
}

#[test]
fn invalid_and_degenerate_geometries_are_rejected_without_nonfinite_outputs() {
    let x = [0.0, 0.0, 0.0, 1.4, 0.0, 0.0];
    for numbers in [[0, 1], [1, 119]] {
        assert!(InternalModel::build(&numbers, &[0, 1, 2, 3, 4, 5], &x, false, None).is_err());
    }
    assert!(InternalModel::build(&[1, 1], &[0, 1, 2], &x[..3], false, None).is_err());
    assert!(InternalModel::build(&[1, 1], &[0, 1, 2, 3, 4, 5], &[0.0; 6], false, None).is_err());
    let mut nonfinite = x;
    nonfinite[4] = f64::NAN;
    assert!(InternalModel::build(&[1, 1], &[0, 1, 2, 3, 4, 5], &nonfinite, false, None).is_err());
    assert!(Fragment::new(&[], &x).is_none());
    assert!(Fragment::new(&[0, 0], &x).is_none());
    assert!(Fragment::new(&[usize::MAX], &x).is_none());
    assert!(Fragment::new(&[0, 1], &[0.0; 6]).is_none());
    let model = InternalModel::build(&[1, 1], &[0, 1, 2, 3, 4, 5], &x, false, None).unwrap();
    assert!(model.chart.values(&x[..3]).is_none());
    assert!(model.chart.rows(&nonfinite, &[0, 1, 2, 3, 4, 5]).is_none());
    assert!(model.back_transform(&x, &[0.0]).is_none());
    assert!(
        model
            .back_transform(&x, &vec![f64::NAN; model.dimension()])
            .is_none()
    );
    assert!(model.displacement(&x, &nonfinite).is_none());
    assert!(model.gradient(&x, &[0.0]).is_none());
    assert!(model.direction(&x, &nonfinite).is_none());
}

#[test]
fn organic_local_metric_has_expected_dlc_dimensions() {
    for ((numbers, x), expected) in [(water(), 3), (ammonia(), 6)] {
        let free = free_components(&x, &[]);
        let model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
        let rows = model.chart.rows(&x, &free).unwrap();
        let organic = &rows[..model.chart.topology.locals.len() * free.len()];
        assert_eq!(rank(organic, free.len()), expected);
    }
}

#[test]
fn tric_spans_all_free_components_for_nonlinear_linear_and_fragmented_molecules() {
    for (numbers, x) in [
        water(),
        ammonia(),
        benzene(),
        carbon_dioxide(),
        acetylene(),
        butyne(),
        water_dimer(),
    ] {
        for fixed in [&[][..], &[0][..], &[0, 1][..]] {
            let free = free_components(&x, fixed);
            for hbonds in [false, true] {
                let model = InternalModel::build(&numbers, &free, &x, hbonds, None).unwrap();
                assert_eq!(
                    rank(&model.chart.rows(&x, &free).unwrap(), free.len()),
                    free.len()
                );
                assert_eq!(model.dimension(), free.len());
            }
        }
    }
}

#[test]
fn tric_spans_distorted_planar_four_coordinate_center() {
    // XeF4 has no exactly opposite ligand pair here: ordinary angle rows have
    // no first-order out-of-plane response and cannot supply these modes.
    let numbers = [54, 9, 9, 9, 9];
    let x = [
        0.0, 0.0, 0.0, 3.8, 0.0, 0.0, 0.7, 3.8, 0.0, -3.8, -0.7, 0.0, 0.0, -3.8, 0.0,
    ];
    for fixed in [&[][..], &[0][..], &[0, 1][..]] {
        let free = free_components(&x, fixed);
        let chart = super::chart::Chart::new(&numbers, &x, false).unwrap();
        assert_eq!(
            rank(&chart.rows(&x, &free).unwrap(), free.len()),
            free.len()
        );
        let model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
        assert_eq!(model.dimension(), free.len());
        let mut target = x;
        target[14] += 1e-4;
        let displacement = model.displacement(&x, &target).unwrap();
        let realized = model.back_transform(&x, &displacement).unwrap();
        assert!(realized.converged);
        assert!(maximum_error(&realized.positions, &target) < 1e-7);
    }
}

#[test]
fn tric_spans_planar_centers_above_coordination_four() {
    // A fifth ligand ensures the extension is not specific to square-planar
    // coordination; the plane is tilted to exercise all Cartesian components.
    let numbers = [54, 9, 9, 9, 9, 9];
    let mut x = vec![0.0; 3];
    for ligand in 0..5 {
        let angle = TAU * ligand as f64 / 5.0;
        x.extend([3.04 * angle.cos(), 3.8 * angle.sin(), 2.28 * angle.cos()]);
    }
    let free = free_components(&x, &[]);
    let model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
    assert_eq!(
        rank(&model.chart.rows(&x, &free).unwrap(), free.len()),
        free.len()
    );
    assert_eq!(model.dimension(), free.len());
}

#[test]
fn small_random_back_transforms_round_trip_and_preserve_fixed_atoms_exactly() {
    let mut state = 0xa731_51ce_9b4d_0207;
    for (numbers, x) in [butane(), water_dimer()] {
        for fixed in [&[][..], &[0][..], &[0, 2][..]] {
            let free = free_components(&x, fixed);
            let model = InternalModel::build(&numbers, &free, &x, true, None).unwrap();
            for length in [1e-5, 1e-4, 1e-3] {
                for _ in 0..4 {
                    let requested = random_vector(model.dimension(), length, &mut state);
                    let transformed = model.back_transform(&x, &requested).unwrap();
                    assert!(transformed.converged);
                    assert!(transformed.positions.iter().all(|v| v.is_finite()));
                    let realized = model.displacement(&x, &transformed.positions).unwrap();
                    assert!(maximum_error(&realized, &requested) <= 1e-8);
                    for &atom in fixed {
                        for axis in 0..3 {
                            assert_eq!(
                                transformed.positions[3 * atom + axis].to_bits(),
                                x[3 * atom + axis].to_bits()
                            );
                        }
                    }
                    let zero = model
                        .back_transform(&transformed.positions, &vec![0.0; model.dimension()])
                        .unwrap();
                    assert!(zero.converged);
                    assert_eq!(zero.positions, transformed.positions);
                }
            }
        }
    }
}

#[test]
fn initial_models_are_symmetric_positive_definite() {
    for (numbers, x) in [ethanol(), benzene(), water_dimer()] {
        for fixed in [&[][..], &[0][..]] {
            let free = free_components(&x, fixed);
            let model = InternalModel::build(&numbers, &free, &x, true, None).unwrap();
            let dim = model.dimension();
            for i in 0..dim {
                for j in 0..dim {
                    close(
                        model.hessian[i * dim + j],
                        model.hessian[j * dim + i],
                        1e-12,
                    );
                }
            }
            let (values, _) = symmetric_eigen(model.hessian.clone(), dim).unwrap();
            assert!(
                values.iter().all(|v| v.is_finite() && *v > 0.0),
                "{values:?}"
            );
        }
    }
}

#[test]
fn gradient_transformation_obeys_cartesian_chain_rule_including_fixed_components() {
    let mut state = 0x2d70_a6e1_189b_047f;
    for (numbers, x) in [ammonia(), acetylene(), water_dimer()] {
        for fixed in [&[][..], &[0][..]] {
            let free = free_components(&x, fixed);
            let model = InternalModel::build(&numbers, &free, &x, true, None).unwrap();
            let expected = random_vector(model.dimension(), 0.03, &mut state);
            let mut gradient = cartesian_gradient(&model, &x, &free, &expected);
            for &atom in fixed {
                gradient[3 * atom..3 * atom + 3].copy_from_slice(&[13.0, -17.0, 23.0]);
            }
            let actual = model.gradient(&x, &gradient).unwrap();
            assert!(maximum_error(&actual, &expected) < 1e-9);
            let restricted: Vec<_> = free.iter().map(|&index| gradient[index]).collect();
            let restricted_result = model.gradient(&x, &restricted).unwrap();
            assert!(maximum_error(&actual, &restricted_result) < 1e-12);
            let direction = random_vector(model.dimension(), 1.0, &mut state);
            let epsilon = 1e-5;
            let plus = model
                .back_transform(
                    &x,
                    &direction.iter().map(|v| epsilon * v).collect::<Vec<_>>(),
                )
                .unwrap();
            let minus = model
                .back_transform(
                    &x,
                    &direction.iter().map(|v| -epsilon * v).collect::<Vec<_>>(),
                )
                .unwrap();
            assert!(plus.converged && minus.converged);
            let numerical = dot(
                &gradient,
                &plus
                    .positions
                    .iter()
                    .zip(&minus.positions)
                    .map(|(p, m)| p - m)
                    .collect::<Vec<_>>(),
            ) / (2.0 * epsilon);
            close(numerical, dot(&actual, &direction), 1e-7);
        }
    }
}

#[test]
fn accepted_bfgs_update_satisfies_the_secant_equation_and_keeps_positive_curvature() {
    let (numbers, x) = water_dimer();
    let free = free_components(&x, &[]);
    let mut model = InternalModel::build(&numbers, &free, &x, true, None).unwrap();
    let mut state = 0x9135_bcc0_6a78_d124;
    let requested = random_vector(model.dimension(), 1e-3, &mut state);
    let next = model.back_transform(&x, &requested).unwrap();
    assert!(next.converged);
    let displacement = model.displacement(&x, &next.positions).unwrap();
    let y: Vec<_> = displacement.iter().map(|s| 3.0 * s).collect();
    let new_gradient = cartesian_gradient(&model, &next.positions, &free, &y);
    model.update(
        &x,
        &next.positions,
        &vec![0.0; x.len()],
        &new_gradient,
        Update::Bfgs,
    );
    let hs: Vec<_> = model
        .hessian
        .chunks_exact(model.dimension())
        .map(|row| dot(row, &displacement))
        .collect();
    assert!(maximum_error(&hs, &y) < 1e-9);
    let (eigenvalues, _) = symmetric_eigen(model.hessian.clone(), model.dimension()).unwrap();
    assert!(eigenvalues.iter().all(|v| *v > 0.0));
    let before = model.hessian.clone();
    let negative_y: Vec<_> = displacement.iter().map(|s| -s).collect();
    let negative_gradient = cartesian_gradient(&model, &next.positions, &free, &negative_y);
    model.update(
        &x,
        &next.positions,
        &vec![0.0; x.len()],
        &negative_gradient,
        Update::Bfgs,
    );
    assert_eq!(model.hessian, before);
}

#[test]
fn rebuild_preserves_learned_cartesian_curvature_through_chart_and_metric_changes() {
    let (numbers, x) = water_dimer();
    let free = free_components(&x, &[0]);
    let mut model = InternalModel::build(&numbers, &free, &x, true, None).unwrap();
    let mut state = 0xc3ed_1084_5640_b29a;
    let requested = random_vector(model.dimension(), 0.01, &mut state);
    let next = model.back_transform(&x, &requested).unwrap();
    assert!(next.converged);
    let displacement = model.displacement(&x, &next.positions).unwrap();
    let y: Vec<_> = displacement
        .iter()
        .enumerate()
        .map(|(i, s)| (1.0 + i as f64 / 3.0) * s)
        .collect();
    let new_gradient = cartesian_gradient(&model, &next.positions, &free, &y);
    model.update(
        &x,
        &next.positions,
        &vec![0.0; x.len()],
        &new_gradient,
        Update::Bfgs,
    );
    let before = physical_hessian(&model, &next.positions, &free);
    model.rebuild(&next.positions).unwrap();
    let after = physical_hessian(&model, &next.positions, &free);
    assert!(
        maximum_error(&after, &before) < 1e-8,
        "Cartesian curvature error {}",
        maximum_error(&after, &before)
    );
}

#[test]
fn rebuild_carries_curvature_when_a_bond_breaks_into_two_fragments() {
    let numbers = [1, 1];
    let x = [0.0, 0.0, 0.0, 1.40, 0.0, 0.0];
    let stretched = [0.0, 0.0, 0.0, 5.0, 0.0, 0.0];
    let free = free_components(&x, &[]);
    let mut model = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
    assert_eq!(model.chart.fragments.len(), 1);
    let dim = model.dimension();
    // An anisotropic learned quadratic form makes a model reset detectable.
    model.hessian.fill(0.0);
    for i in 0..dim {
        for j in 0..dim {
            model.hessian[i * dim + j] = 0.01 * (i + 1) as f64 * (j + 1) as f64;
        }
        model.hessian[i * dim + i] += 1.0 + 0.5 * i as f64;
    }
    assert!(model.needs_rebuild(&stretched));
    let before = physical_hessian(&model, &stretched, &free);
    model.rebuild(&stretched).unwrap();
    assert_eq!(model.chart.fragments.len(), 2);
    assert_eq!(model.dimension(), dim);
    assert!(model.chart.topology.locals.is_empty());
    let after = physical_hessian(&model, &stretched, &free);
    assert!(maximum_error(&before, &after) < 1e-8);
}

#[test]
fn supplied_full_and_restricted_cartesian_hessians_transform_with_fixed_components() {
    let (numbers, x) = water_dimer();
    let free = free_components(&x, &[0, 4]);
    let ncart = x.len();
    let mut full = vec![0.0; ncart * ncart];
    for i in 0..ncart {
        for j in 0..ncart {
            full[i * ncart + j] = 0.003 * (i + 1) as f64 * (j + 1) as f64;
        }
        full[i * ncart + i] += 1.0 + i as f64 / 5.0;
    }
    let restricted: Vec<_> = free
        .iter()
        .flat_map(|&i| free.iter().map(move |&j| (i, j)))
        .map(|(i, j)| full[i * ncart + j])
        .collect();
    let from_full = InternalModel::build(&numbers, &free, &x, true, Some(&full)).unwrap();
    let from_restricted =
        InternalModel::build(&numbers, &free, &x, true, Some(&restricted)).unwrap();
    assert_eq!(from_full.dimension(), free.len());
    for model in [&from_full, &from_restricted] {
        let transformed = physical_hessian(model, &x, &free);
        assert!(maximum_error(&transformed, &restricted) < 1e-8);
    }
    assert!(maximum_error(&from_full.hessian, &from_restricted.hessian) < 1e-10);
}

#[test]
fn an_impossible_negative_bond_length_rejects_an_out_of_chart_fallback() {
    let x = [0.0, 0.0, 0.0, 1.40, 0.0, 0.0];
    let free = free_components(&x, &[]);
    let model = InternalModel::build(&[1, 1], &free, &x, false, None).unwrap();
    let nprimitive = model.chart.values(&x).unwrap().len();
    let mut primitive_step = vec![0.0; nprimitive];
    let stretch = model
        .chart
        .topology
        .locals
        .iter()
        .position(|(p, _)| matches!(p, Local::Stretch(0, 1) | Local::Stretch(1, 0)))
        .unwrap();
    // Target r = 1.40 - 1.60 = -0.20 bohr, which has no Cartesian image.
    primitive_step[stretch] = -1.60;
    let step: Vec<_> = model
        .basis
        .chunks_exact(nprimitive)
        .map(|u| dot(u, &primitive_step))
        .collect();
    // The stretch is orthogonal to all centroid and rotational coordinates.
    // Its first linear image moves the endpoints inward by 0.80 bohr each.
    // They pass through one another. The resulting reversed line is outside
    // the rotational chart and must not be returned to a direct caller.
    assert!(model.back_transform(&x, &step).is_none());
}

#[test]
fn rfo_restriction_returns_descent_with_negative_prediction_and_internal_radius() {
    let mut state = 0x41b6_d507_9e33_a298;
    for (numbers, x) in [water(), water_dimer()] {
        for fixed in [&[][..], &[0][..]] {
            let free = free_components(&x, fixed);
            let model = InternalModel::build(&numbers, &free, &x, true, None).unwrap();
            let gradient = cartesian_gradient(
                &model,
                &x,
                &free,
                &random_vector(model.dimension(), 0.01, &mut state),
            );
            let direction = model.direction(&x, &gradient).unwrap();
            for radius in [1e-4, 1e-3, 0.01] {
                let crate::optimizer::TrialStep {
                    cartesian: displacement,
                    predicted,
                    length,
                    ..
                } = direction.restricted(&x, radius).unwrap();
                assert!(predicted.is_finite() && predicted < 0.0);
                assert_eq!(displacement.len(), free.len());
                let mut positions = x.clone();
                for (&index, &step) in free.iter().zip(&displacement) {
                    positions[index] += step;
                }
                let internal = model.displacement(&x, &positions).unwrap();
                assert!(norm(&internal) <= radius + 1e-8);
                close(length, norm(&internal), 1e-8);
                let internal_gradient = model.gradient(&x, &gradient).unwrap();
                let hs: Vec<_> = model
                    .hessian
                    .chunks_exact(model.dimension())
                    .map(|r| dot(r, &internal))
                    .collect();
                let expected = (dot(&internal_gradient, &internal) + 0.5 * dot(&internal, &hs))
                    / (1. + dot(&internal, &internal));
                close(predicted, expected, 1e-10);
                let restricted_gradient: Vec<_> =
                    free.iter().map(|&index| gradient[index]).collect();
                assert!(dot(&restricted_gradient, &displacement) < 0.0);
                for &atom in fixed {
                    assert_eq!(
                        &positions[3 * atom..3 * atom + 3],
                        &x[3 * atom..3 * atom + 3]
                    );
                }
            }
        }
    }
}

#[test]
fn selected_updates_apply_endpoint_secants_in_the_nonlinear_internal_chart() {
    let (numbers, x) = water_dimer();
    let free = free_components(&x, &[0]);
    let mut state = 0xb121_658c_78f4_e902;
    let requested = random_vector(free.len(), 1e-3, &mut state);
    for update in [Update::Bfgs, Update::DampedBfgs, Update::Bofill] {
        let mut model = InternalModel::build(&numbers, &free, &x, true, None).unwrap();
        let next = model.back_transform(&x, &requested).unwrap();
        assert!(next.converged);
        let s = model.displacement(&x, &next.positions).unwrap();
        let y: Vec<_> = s.iter().map(|s| -s).collect();
        let gradient = cartesian_gradient(&model, &next.positions, &free, &y);
        let before = model.hessian.clone();
        let hs: Vec<_> = before
            .chunks_exact(s.len())
            .map(|row| dot(row, &s))
            .collect();
        model.update(&x, &next.positions, &vec![0.; x.len()], &gradient, update);
        if update == Update::Bfgs {
            assert_eq!(model.hessian, before);
            continue;
        }
        let actual: Vec<_> = model
            .hessian
            .chunks_exact(s.len())
            .map(|row| dot(row, &s))
            .collect();
        let expected = if update == Update::DampedBfgs {
            let shs = dot(&s, &hs);
            let theta = 0.8 * shs / (shs - dot(&s, &y));
            y.iter()
                .zip(&hs)
                .map(|(y, hs)| theta * y + (1. - theta) * hs)
                .collect::<Vec<_>>()
        } else {
            y
        };
        assert!(maximum_error(&actual, &expected) < 1e-9);
        let eigenvalues = symmetric_eigen(model.hessian.clone(), model.dimension())
            .unwrap()
            .0;
        assert_eq!(eigenvalues[0] > 0., update == Update::DampedBfgs);
        assert_eq!(model.positive_definite, update == Update::DampedBfgs);
        assert_eq!(&next.positions[..3], &x[..3]);
    }
}

#[test]
fn contact_curvature_preserves_the_dlc_metric_and_matches_the_free_cartesian_form() {
    use crate::model_hessian::contact_stretches;
    use crate::optimizer::Options;
    let (numbers, x) = water_dimer();
    let free = free_components(&x, &[0]);
    let base = InternalModel::build(&numbers, &free, &x, false, None).unwrap();
    let augmented = InternalModel::build_with_options(
        &numbers,
        &free,
        &x,
        false,
        None,
        &Options {
            contacts: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(base.basis, augmented.basis);
    assert_eq!(base.tangent.b, augmented.tangent.b);
    assert_eq!(base.chart.fragments.len(), augmented.chart.fragments.len());
    let n = free.len();
    for i in 0..n {
        for j in 0..n {
            assert_eq!(augmented.hessian[i * n + j], augmented.hessian[j * n + i]);
        }
    }
    let mut state = 0xf1a7_12c4_69a1_380e;
    let dx = random_vector(n, 0.01, &mut state);
    let dq = super::metric::apply(&base.tangent.b, &dx);
    let delta_h: Vec<_> = augmented
        .hessian
        .iter()
        .zip(&base.hessian)
        .map(|(a, b)| a - b)
        .collect();
    let actual = dot(&dq, &super::metric::apply(&delta_h, &dq));
    let contacts = contact_stretches(&numbers, &x);
    assert!(!contacts.is_empty());
    let expected: f64 = contacts
        .iter()
        .map(|c| c.force * dot(&c.free_row(&free), &dx).powi(2))
        .sum();
    assert!(expected > 0.);
    close(actual, expected, 1e-12);
    assert!(
        augmented
            .hessian
            .iter()
            .zip(&base.hessian)
            .any(|(a, b)| (a - b).abs() > 1e-8)
    );
}

#[test]
fn fragment_prior_uses_the_requested_curvature_floor_and_potential_curvature_is_unchanged() {
    use crate::optimizer::Options;
    let numbers = [18, 18];
    let x = [0., 0., 0., 15., 0., 0.];
    let free = [3, 4, 5];
    let options = Options {
        curvature_floor: 0.007,
        contacts: true,
        ..Options::default()
    };
    let mut model =
        InternalModel::build_with_options(&numbers, &free, &x, false, None, &options).unwrap();
    assert_eq!(model.hessian, [0.007, 0., 0., 0., 0.007, 0., 0., 0., 0.007]);
    model.rebuild(&[0., 0., 0., 16., 1., 0.]).unwrap();
    assert_eq!(model.parameters.curvature_floor, options.curvature_floor);
    assert!(model.parameters.contacts);
    assert_eq!(model.hessian, [0.007, 0., 0., 0., 0.007, 0., 0., 0., 0.007]);

    // At a live contact, optional model curvature is not added to a supplied
    // potential Hessian, which already describes that interaction.
    let x = [0., 0., 0., 2. * 1.88 / crate::ANGSTROM_PER_BOHR, 0., 0.];
    let exact = [2., 0., 0., 0., 3., 0., 0., 0., 4.];
    let with =
        InternalModel::build_with_options(&numbers, &free, &x, false, Some(&exact), &options)
            .unwrap();
    let without = InternalModel::build(&numbers, &free, &x, false, Some(&exact)).unwrap();
    assert_eq!(with.hessian, without.hessian);
}
