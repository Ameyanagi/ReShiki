use super::{RELATIVE_CUTOFF, Tangent, apply, row_gram, transpose_apply};
use crate::linalg::{dot, symmetric_eigen};

fn identity(n: usize) -> Vec<f64> {
    let mut a = vec![0.; n * n];
    for i in 0..n {
        a[i * n + i] = 1.;
    }
    a
}

fn close(actual: &[f64], expected: &[f64], tolerance: f64) {
    assert_eq!(actual.len(), expected.len());
    for (&a, &e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < tolerance, "{a} != {e}");
    }
}

#[test]
fn factored_tangent_solves_both_chain_rules_without_an_explicit_dual() {
    let n = 4;
    // Both chain rules include nontrivial off-diagonal coupling.
    let b = vec![
        0., 2., -1., 0.3, 3., -1., 0.4, 0., 1., 0.2, 2., -0.5, 0.1, -0.3, 0.6, 1.,
    ];
    let tangent = Tangent::from_projected(b.clone(), n).unwrap();
    assert!(tangent.dense_dual().is_none());
    let cartesian = [0.4, -0.7, 0.2, 0.9];
    let internal = apply(&b, &cartesian);
    close(
        &tangent.cartesian_step(&internal).unwrap(),
        &cartesian,
        1e-13,
    );
    let gradient = [0.1, -0.4, 0.3, 0.7];
    let cartesian_gradient = transpose_apply(&b, n, &gradient);
    close(
        &tangent.gradient(&cartesian_gradient).unwrap(),
        &gradient,
        1e-13,
    );
    assert!((dot(&cartesian_gradient, &cartesian) - dot(&gradient, &internal)).abs() < 1e-13);
    assert!(tangent.cartesian_step(&[1.]).is_none());
    assert!(tangent.gradient(&[1.]).is_none());
}

#[test]
fn rank_guard_retains_the_relative_singular_value_cutoff_after_chart_motion() {
    let n = 3;
    for scale in [0., 0.99e-5, 1.01e-5, 2e-5, 0.5, 1.] {
        let mut b = identity(n);
        b[0] = scale;
        // Nontrivial left and right orthogonal factors preserve the known
        // singular spectrum while preventing a pivot-magnitude rank test.
        let (s, c) = 0.37_f64.sin_cos();
        for column in 0..n {
            let (a, d) = (b[column], b[n + column]);
            b[column] = c * a - s * d;
            b[n + column] = s * a + c * d;
        }
        for row in b.chunks_exact_mut(n) {
            let (a, d) = (row[1], row[2]);
            row[1] = c * a - s * d;
            row[2] = s * a + c * d;
        }
        let values = symmetric_eigen(row_gram(&b, n), n).unwrap().0;
        let expected = values[0] > RELATIVE_CUTOFF * values[n - 1];
        assert_eq!(expected, scale > RELATIVE_CUTOFF.sqrt());
        assert_eq!(
            Tangent::from_projected(b, n).is_some(),
            expected,
            "scale {scale}"
        );
    }
}

#[test]
fn instantaneous_solve_matches_the_spectral_wilson_reciprocal() {
    let n = 4;
    let b = vec![
        0.5, 0.2, -0.1, 0.4, -0.3, 2., 0.2, -0.1, 0.1, 0.2, 1.5, 0.3, 0.2, -0.1, 0.1, 0.7,
    ];
    let tangent = Tangent::from_projected(b.clone(), n).unwrap();
    let (values, vectors) = symmetric_eigen(row_gram(&b, n), n).unwrap();
    let mut dual = vec![0.; n * n];
    for (v, lambda) in vectors.chunks_exact(n).zip(values) {
        let mode = transpose_apply(&b, n, v);
        for (&weight, row) in v.iter().zip(dual.chunks_exact_mut(n)) {
            for (entry, &x) in row.iter_mut().zip(&mode) {
                *entry += weight * x / lambda;
            }
        }
    }
    let v = [0.03, -0.02, 0.1, 0.05];
    close(&tangent.gradient(&v).unwrap(), &apply(&dual, &v), 1e-13);
    close(
        &tangent.cartesian_step(&v).unwrap(),
        &transpose_apply(&dual, n, &v),
        1e-13,
    );
}

#[test]
fn accurate_solve_recovers_when_the_rank_shift_is_not_a_contracting_preconditioner() {
    let n = 3;
    let b = vec![1.01e-5, 0., 0., 0., 1., 0., 0., 0., 0.7];
    let tangent = Tangent::from_projected(b.clone(), n).unwrap();
    let x = [0.4, -0.6, 0.8];
    let q = apply(&b, &x);
    // The shifted factor certifies rank, but its tiny first pivot would make
    // a raw chord step enormous. The error guard must select the accurate solve.
    close(&tangent.cartesian_correction(&q).unwrap(), &x, 1e-12);
    close(&tangent.cartesian_step(&q).unwrap(), &x, 1e-12);
    let gradient = [0.2, -0.3, 0.4];
    let cartesian = transpose_apply(&b, n, &gradient);
    close(&tangent.gradient(&cartesian).unwrap(), &gradient, 1e-12);
}
