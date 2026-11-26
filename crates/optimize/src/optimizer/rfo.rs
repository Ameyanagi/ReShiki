//! Shared rational-function, secant-update and DIIS calculations.
//!
//! Coordinates are supplied by the caller. Every vector in one proposal must
//! use the same model chart, including both endpoint secant gradients.
//!
//! Full references: docs/references/xtb.md.

mod diis;
mod positive;

use super::Update;
use crate::linalg::{dot, norm, symmetric_eigen};
pub(crate) use diis::{diis_direction, is_positive_definite};
#[cfg(test)]
pub(crate) use diis::{energy_coefficients, residual_coefficients};

/// Lowest eigenvector of [H g; gᵀ 0], stored in model coordinates.
pub(crate) fn dense_lowest_eigenvector(hessian: &[f64], gradient: &[f64]) -> Option<Vec<f64>> {
    let n = gradient.len();
    let stride = n + 1;
    let mut matrix = vec![0.; stride * stride];
    for (i, row) in hessian.chunks_exact(n).enumerate() {
        matrix[i * stride..i * stride + n].copy_from_slice(row);
        matrix[i * stride + n] = gradient[i];
        matrix[n * stride + i] = gradient[i];
    }
    let (_, mut vectors) = symmetric_eigen(matrix, stride).ok()?;
    vectors.truncate(stride);
    Some(vectors)
}

/// Banerjee et al., J. Phys. Chem. 89, 52 (1985) \[Banerjee1985PRFO\]. Normalizing the
/// homogeneous component to one gives the RFO displacement g·s = λ < 0.
pub(crate) fn eigenvector_step(vector: &[f64], gradient: &[f64]) -> Option<Vec<f64>> {
    let n = gradient.len();
    if vector[n] == 0. {
        return None;
    }
    Some(vector[..n].iter().map(|v| v / vector[n]).collect())
}

pub(crate) fn dense_step(hessian: &[f64], gradient: &[f64]) -> Option<Vec<f64>> {
    if gradient.iter().all(|&x| x == 0.) {
        return None;
    }
    eigenvector_step(&dense_lowest_eigenvector(hessian, gradient)?, gradient)
}

/// Lowest-root RFO displacement. A positive-definite H has one negative
/// augmented root, found by a shifted solve with an accuracy check. A failed
/// solve retains the dense eigenproblem. Small systems also use the dense
/// solve, avoiding an iterative solve's setup and roundoff on tiny modes.
pub(crate) fn step(hessian: &[f64], gradient: &[f64], positive_definite: bool) -> Option<Vec<f64>> {
    const DENSE_DIMENSION: usize = 20;
    if !positive_definite || gradient.len() <= DENSE_DIMENSION {
        return dense_step(hessian, gradient);
    }
    if gradient.iter().all(|&x| x == 0.) {
        return None;
    }
    positive::step(hessian, gradient).or_else(|| dense_step(hessian, gradient))
}

pub(crate) fn prediction(hessian: &[f64], gradient: &[f64], step: &[f64]) -> Option<f64> {
    let n = gradient.len();
    let hs: Vec<_> = hessian.chunks_exact(n).map(|row| dot(row, step)).collect();
    let value = (dot(gradient, step) + 0.5 * dot(step, &hs)) / (1. + dot(step, step));
    (value.is_finite() && value < 0.).then_some(value)
}

/// Apply the selected published update and its numerical curvature safeguards.
pub(crate) fn update(
    hessian: &mut [f64],
    positive_definite: &mut bool,
    s: &[f64],
    y: &mut [f64],
    update: Update,
) {
    let n = s.len();
    let mut ys = dot(y, s);
    if update == Update::Bfgs && (!ys.is_finite() || ys <= 1e-12 * norm(y) * norm(s)) {
        return;
    }
    let hs: Vec<_> = hessian.chunks_exact(n).map(|row| dot(row, s)).collect();
    let shs = dot(s, &hs);
    if update == Update::Bofill {
        bofill(hessian, positive_definite, s, y, &hs, ys, shs);
        return;
    }
    if !shs.is_finite() || shs <= 0. {
        return;
    }
    if update == Update::DampedBfgs {
        if !ys.is_finite() {
            return;
        }
        // Nocedal & Wright, Numerical Optimization, Procedure 18.2 [Powell1978Damping].
        // Replacing y by r ensures s·r >= 0.2 s·Hs.
        if ys < 0.2 * shs {
            let theta = 0.8 * shs / (shs - ys);
            for (yi, hsi) in y.iter_mut().zip(&hs) {
                *yi = theta * *yi + (1. - theta) * hsi;
            }
            ys = dot(y, s);
        }
        if !ys.is_finite() || ys <= 0. {
            return;
        }
    }
    for (i, row) in hessian.chunks_exact_mut(n).enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            *x += y[i] * y[j] / ys - hs[i] * hs[j] / shs;
        }
    }
}

fn bofill(
    hessian: &mut [f64],
    positive_definite: &mut bool,
    s: &[f64],
    y: &[f64],
    hs: &[f64],
    ys: f64,
    shs: f64,
) {
    // Farkas & Schlegel, J. Chem. Phys. 111, 10806 (1999),
    // doi:10.1063/1.480484 [Farkas1999Update]: minimization blend of SR1 and BFGS,
    // with phi = |(y − Hs)·s| / (|y − Hs| |s|).
    let residual: Vec<_> = y.iter().zip(hs).map(|(yi, hsi)| yi - hsi).collect();
    let snorm = norm(s);
    let rnorm = norm(&residual);
    if !(snorm.is_finite() && snorm > 0. && rnorm.is_finite() && rnorm > 0.) {
        return;
    }
    let rs = dot(&residual, s);
    let sr1 = rs.is_finite() && rs.abs() > 1e-12 * rnorm * snorm;
    let bfgs = ys.is_finite()
        && ys.abs() > 1e-12 * norm(y) * snorm
        && shs.is_finite()
        && shs.abs() > 1e-12 * norm(hs) * snorm;
    // If one component is numerically undefined, use the other in full;
    // both component updates satisfy the same secant equation.
    let phi = match (sr1, bfgs) {
        (true, true) => ((rs.abs() / rnorm) / snorm).min(1.),
        (true, false) => 1.,
        (false, true) => 0.,
        (false, false) => return,
    };
    let n = s.len();
    for i in 0..n {
        for j in 0..=i {
            let mut value = hessian[i * n + j];
            if phi > 0. {
                // Cancel |r·s| analytically to avoid a small SR1 divisor.
                value += if bfgs {
                    rs.signum() * (residual[i] / rnorm) * (residual[j] / snorm)
                } else {
                    residual[i] * residual[j] / rs
                };
            }
            if phi < 1. {
                value += (1. - phi) * (y[i] * y[j] / ys - hs[i] * hs[j] / shs);
            }
            hessian[i * n + j] = value;
            hessian[j * n + i] = value;
        }
    }
    *positive_definite &= (phi == 0. || rs > 0.) && (phi == 1. || (ys > 0. && shs > 0.));
}
