//! Davidson regression on a realistic Lindh model Hessian after BFGS updates.

use super::*;
use crate::model_hessian::model_hessian;
use crate::optimizer::{Coordinates, Point, Update, anc::Anc};

// An all-trans n-hexane geometry with tetrahedral C/H directions, in bohr.
// Twenty atoms leave 54 internal modes, exceeding Davidson's subspace size.
fn hexane() -> (Vec<u8>, Vec<f64>) {
    let along = (2.0_f64 / 3.0).sqrt();
    let across = (1.0_f64 / 3.0).sqrt();
    let carbons: Vec<_> = (0..6)
        .map(|i| [2.91 * along * i as f64, 2.91 * across * (i % 2) as f64, 0.])
        .collect();
    let mut numbers = vec![6; 6];
    let mut positions: Vec<_> = carbons.iter().flatten().copied().collect();
    for (i, carbon) in carbons.iter().enumerate() {
        let sign = if i % 2 == 0 { -1. } else { 1. };
        let mut directions = vec![[0., sign * across, along], [0., sign * across, -along]];
        if i == 0 || i == 5 {
            directions.push([sign * along, -sign * across, 0.]);
        }
        for direction in directions {
            numbers.push(1);
            positions.extend((0..3).map(|axis| carbon[axis] + 2.06 * direction[axis]));
        }
    }
    (numbers, positions)
}

#[test]
fn small_gradient_lindh_anc_with_bfgs_updates_converges_without_dense_fallback() {
    let (numbers, positions) = hexane();
    let coordinates = Coordinates::new(&numbers, &positions, &[]).unwrap();
    let point = Point {
        rigid: coordinates.rigid(&positions),
        gradient: vec![0.; positions.len()],
        active_gradient: vec![0.; positions.len()],
        energy: 0.,
        positions,
    };
    let mut anc = Anc::build(
        &numbers,
        &coordinates,
        &point,
        0,
        None,
        None,
        &crate::optimizer::Options::default(),
    )
    .unwrap();
    let dim = anc.hessian.len().isqrt();
    assert!(dim > MAX_SUBSPACE);

    // Exact secant pairs for a distorted Lindh quadratic model provide positive
    // curvature while introducing the couplings learned during optimization.
    let nearby: Vec<_> = point
        .positions
        .iter()
        .enumerate()
        .map(|(i, x)| x + 0.4 * (0.71 * i as f64 + 0.3).sin())
        .collect();
    let target = model_hessian(&numbers, &nearby, false).unwrap();
    for update in 0..12 {
        let displacement: Vec<_> = (0..point.positions.len())
            .map(|i| 0.01 * ((i + 1) as f64 * (update + 1) as f64 * 0.37).sin())
            .collect();
        let gradient_change: Vec<_> = target
            .chunks_exact(point.positions.len())
            .map(|row| dot(row, &displacement))
            .collect();
        let s = anc.to_anc(&displacement);
        let y = anc.to_anc(&gradient_change);
        assert!(dot(&s, &y) > 1e-12 * norm(&s) * norm(&y));
        anc.update(&displacement, &gradient_change, Update::Bfgs);
    }
    assert!(
        anc.hessian
            .chunks_exact(dim)
            .enumerate()
            .any(|(i, row)| row[..i].iter().any(|x| x.abs() > 1e-5)),
        "BFGS must introduce off-diagonal ANC curvature"
    );

    let cartesian_gradient: Vec<_> = (0..point.positions.len())
        .map(|i| (0.43 * i as f64 + 0.8).sin())
        .collect();
    let mut gradient = anc.to_anc(&cartesian_gradient);
    let scale = 1e-5 / norm(&gradient);
    gradient.iter_mut().for_each(|g| *g *= scale);
    let matrix = AugmentedHessian {
        hessian: &anc.hessian,
        gradient: &gradient,
    };

    // Calling the iterative method directly ensures a dense fallback cannot
    // hide failure on the near-converged molecular ANC system.
    let iterative = matrix
        .davidson(MAX_SUBSPACE, MAX_ITERATIONS)
        .expect("Davidson must converge for a small-gradient Lindh/BFGS ANC model");
    let dense = matrix.dense_lowest_eigenvector().unwrap();
    let step: Vec<_> = iterative[..dim]
        .iter()
        .map(|x| x / iterative[dim])
        .collect();
    let exact_step: Vec<_> = dense[..dim].iter().map(|x| x / dense[dim]).collect();
    let difference: Vec<_> = step.iter().zip(&exact_step).map(|(x, y)| x - y).collect();
    let relative_error = norm(&difference) / norm(&exact_step);
    assert!(
        relative_error < 1e-6,
        "relative RFO step error {relative_error:e}"
    );
    assert!(dot(&gradient, &step) < 0.);
}
